//! FAT32 high-level filesystem API (MVP).

use alloc::vec::Vec;

use crate::bpb::Bpb;
use crate::device::BlockDevice;
use crate::dir::{to_short_name_83, DirEntry};
use crate::error::{Error, Result};
use crate::fat::{cluster_to_lba, find_free_cluster, read_fat_entry, write_fat_entry, EOC_MIN};

/// FAT32 filesystem handle.
pub struct Fat32<D: BlockDevice> {
    dev: D,
    bpb: Bpb,
}

impl<D: BlockDevice> Fat32<D> {
    /// Mount a FAT32 volume by reading and parsing sector 0.
    pub fn mount(mut dev: D) -> Result<Self> {
        let mut boot = [0u8; 512];
        dev.read_sector(0, &mut boot)?;
        let bpb = Bpb::parse(&boot)?;
        Ok(Self { dev, bpb })
    }

    /// Return parsed BPB info.
    pub fn bpb(&self) -> &Bpb {
        &self.bpb
    }

    /// Read the root directory entries (8.3 only, skipping LFN in this MVP).
    pub fn list_root(&self) -> Result<Vec<DirEntry>> {
        let mut out = Vec::new();
        let mut cluster = self.bpb.root_cluster;

        loop {
            let base_lba = cluster_to_lba(&self.bpb, cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u64) {
                let mut buf = [0u8; 512];
                self.dev.read_sector(base_lba + s, &mut buf)?;
                for i in 0..16 {
                    let mut rec = [0u8; 32];
                    rec.copy_from_slice(&buf[i * 32..i * 32 + 32]);
                    if let Some(e) = DirEntry::parse(&rec)? {
                        // Skip deleted / LFN placeholders
                        if e.attr == 0x0F || (e.first_cluster == 0 && e.file_size == 0 && e.raw_name == [0; 11]) {
                            continue;
                        }
                        out.push(e);
                    } else {
                        return Ok(out);
                    }
                }
            }

            let next = read_fat_entry(&self.dev, &self.bpb, cluster)?;
            if next >= EOC_MIN {
                break;
            }
            if next < 2 {
                return Err(Error::Corrupt);
            }
            cluster = next;
        }

        Ok(out)
    }

    /// Read a file by short name (8.3 only) from root directory.
    pub fn read_file_root(&self, name: &str) -> Result<Vec<u8>> {
        let target = to_short_name_83(name)?;
        let entries = self.list_root()?;

        let mut found = None;
        for e in entries {
            if e.raw_name == target {
                found = Some(e);
                break;
            }
        }
        let e = found.ok_or(Error::NotFound)?;
        if e.first_cluster < 2 {
            return Err(Error::Corrupt);
        }

        let mut remaining = e.file_size as usize;
        let mut data = Vec::with_capacity(remaining);
        let mut cluster = e.first_cluster;

        while remaining > 0 {
            let base_lba = cluster_to_lba(&self.bpb, cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u64) {
                let mut buf = [0u8; 512];
                self.dev.read_sector(base_lba + s, &mut buf)?;
                let take = remaining.min(512);
                data.extend_from_slice(&buf[..take]);
                remaining -= take;
                if remaining == 0 {
                    break;
                }
            }
            if remaining == 0 {
                break;
            }
            let next = read_fat_entry(&self.dev, &self.bpb, cluster)?;
            if next >= EOC_MIN {
                return Err(Error::Corrupt);
            }
            cluster = next;
        }

        Ok(data)
    }

    /// Create or overwrite a root file (8.3) and write `content` persistently.
    ///
    /// MVP limitations:
    /// - allocates a new cluster chain (does not free old chains if overwriting)
    /// - writes FAT #0 only (not mirrored to FAT #1 if present)
    /// - writes into root directory only
    pub fn write_file_root(&mut self, name: &str, content: &[u8]) -> Result<()> {
        let short = to_short_name_83(name)?;
        let clusters_needed = clusters_for_len(&self.bpb, content.len());
        if clusters_needed == 0 {
            return Err(Error::InvalidName);
        }

        // 1) Allocate cluster chain
        let mut chain = Vec::with_capacity(clusters_needed);
        let mut next_search = 2u32;
        for _ in 0..clusters_needed {
            let c = find_free_cluster(&self.dev, &self.bpb, next_search)?;
            // Reserve quickly
            write_fat_entry(&mut self.dev, &self.bpb, c, 0x0FFFFFFF)?;
            chain.push(c);
            next_search = c + 1;
        }
        // Link chain
        for i in 0..chain.len() {
            let cur = chain[i];
            let val = if i + 1 < chain.len() { chain[i + 1] } else { 0x0FFFFFFF };
            write_fat_entry(&mut self.dev, &self.bpb, cur, val)?;
        }

        // 2) Write data to clusters
        let mut offset = 0usize;
        for &cluster in &chain {
            let base_lba = cluster_to_lba(&self.bpb, cluster);
            for s in 0..(self.bpb.sectors_per_cluster as u64) {
                let mut sector = [0u8; 512];
                let remain = content.len().saturating_sub(offset);
                if remain != 0 {
                    let take = remain.min(512);
                    sector[..take].copy_from_slice(&content[offset..offset + take]);
                    offset += take;
                }
                self.dev.write_sector(base_lba + s, &sector)?;
            }
        }

        // 3) Create directory entry in root (first free slot)
        let first_cluster = chain[0];
        let rec = DirEntry::build_short_file(short, first_cluster, content.len() as u32);
        self.write_root_dir_entry_first_free(&rec)?;

        Ok(())
    }

    fn write_root_dir_entry_first_free(&mut self, rec: &[u8; 32]) -> Result<()> {
        let mut cluster = self.bpb.root_cluster;

        loop {
            let base_lba = cluster_to_lba(&self.bpb, cluster);

            for s in 0..(self.bpb.sectors_per_cluster as u64) {
                let lba = base_lba + s;
                let mut buf = [0u8; 512];
                self.dev.read_sector(lba, &mut buf)?;

                for i in 0..16 {
                    let first = buf[i * 32];
                    if first == 0x00 || first == 0xE5 {
                        buf[i * 32..i * 32 + 32].copy_from_slice(rec);
                        self.dev.write_sector(lba, &buf)?;
                        return Ok(());
                    }
                }
            }

            let next = read_fat_entry(&self.dev, &self.bpb, cluster)?;
            if next >= EOC_MIN {
                return Err(Error::DirFull);
            }
            if next < 2 {
                return Err(Error::Corrupt);
            }
            cluster = next;
        }
    }

    /// Consume the filesystem and return the underlying device (useful in tests).
    pub fn into_device(self) -> D {
        self.dev
    }
}

fn clusters_for_len(bpb: &Bpb, len: usize) -> usize {
    let bytes_per_cluster = (bpb.sectors_per_cluster as usize) * 512;
    (len + bytes_per_cluster - 1) / bytes_per_cluster
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::MemDevice;

    fn make_tiny_fat32_image() -> std::vec::Vec<u8> {
        // Minimal FAT32-like image for tests.
        let total_sectors = 200u32;
        let mut img = vec![0u8; (total_sectors as usize) * 512];

        // Boot sector @ LBA0
        let bs = &mut img[0..512];
        bs[510] = 0x55;
        bs[511] = 0xAA;

        // bytes_per_sector = 512
        bs[11..13].copy_from_slice(&512u16.to_le_bytes());
        // sectors_per_cluster = 1
        bs[13] = 1;
        // reserved_sectors = 32
        bs[14..16].copy_from_slice(&32u16.to_le_bytes());
        // num_fats = 1
        bs[16] = 1;
        // root_entry_count = 0
        bs[17..19].copy_from_slice(&0u16.to_le_bytes());
        // total_sectors_32
        bs[32..36].copy_from_slice(&total_sectors.to_le_bytes());
        // fat_size_32 = 1 sector
        bs[36..40].copy_from_slice(&1u32.to_le_bytes());
        // root_cluster = 2
        bs[44..48].copy_from_slice(&2u32.to_le_bytes());
        // fsinfo sector
        bs[48..50].copy_from_slice(&1u16.to_le_bytes());

        // FAT @ LBA = reserved = 32
        let fat_lba = 32usize;
        let fat = &mut img[fat_lba * 512..fat_lba * 512 + 512];
        fat[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes()); // cluster 0
        fat[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes()); // cluster 1
        fat[8..12].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes()); // cluster 2 root EOC

        img
    }

    #[test]
    fn mount_and_write_and_read() {
        let img = make_tiny_fat32_image();
        let dev = MemDevice::new(img);

        let mut fs = Fat32::mount(dev).expect("mount");
        fs.write_file_root("HELLO.TXT", b"abc").expect("write");

        let data = fs.read_file_root("HELLO.TXT").expect("read");
        assert_eq!(data, b"abc");
    }
}
