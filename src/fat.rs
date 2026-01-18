//! FAT table helpers (FAT32).

use crate::bpb::Bpb;
use crate::device::BlockDevice;
use crate::error::{Error, Result};

/// FAT32 end-of-chain marker threshold.
pub const EOC_MIN: u32 = 0x0FFFFFF8;

fn le_u32(x: &[u8]) -> u32 {
    u32::from_le_bytes([x[0], x[1], x[2], x[3]])
}
fn write_le_u32(dst: &mut [u8], v: u32) {
    let b = v.to_le_bytes();
    dst[0..4].copy_from_slice(&b);
}

/// Compute LBA of FAT region start.
pub fn fat_start_lba(bpb: &Bpb) -> u64 {
    bpb.reserved_sectors as u64
}

/// Compute LBA of data region start.
pub fn data_start_lba(bpb: &Bpb) -> u64 {
    fat_start_lba(bpb) + (bpb.num_fats as u64) * (bpb.fat_size_32 as u64)
}

/// Convert cluster number to first sector LBA.
pub fn cluster_to_lba(bpb: &Bpb, cluster: u32) -> u64 {
    // Cluster numbers start at 2.
    let first_data = data_start_lba(bpb);
    first_data + ((cluster - 2) as u64) * (bpb.sectors_per_cluster as u64)
}

/// Read FAT entry (next cluster) for `cluster`.
pub fn read_fat_entry<D: BlockDevice>(dev: &D, bpb: &Bpb, cluster: u32) -> Result<u32> {
    let fat_offset = cluster as u64 * 4;
    let sector = fat_start_lba(bpb) + (fat_offset / 512);
    let off = (fat_offset % 512) as usize;

    let mut buf = [0u8; 512];
    dev.read_sector(sector, &mut buf)?;
    let v = le_u32(&buf[off..off + 4]) & 0x0FFFFFFF;
    Ok(v)
}

/// Write FAT entry for `cluster` (updates only FAT #0 in this MVP).
///
/// For a “proper” implementation, you should mirror to all FATs.
pub fn write_fat_entry<D: BlockDevice>(
    dev: &mut D,
    bpb: &Bpb,
    cluster: u32,
    value: u32,
) -> Result<()> {
    let fat_offset = cluster as u64 * 4;
    let sector = fat_start_lba(bpb) + (fat_offset / 512);
    let off = (fat_offset % 512) as usize;

    let mut buf = [0u8; 512];
    dev.read_sector(sector, &mut buf)?;
    write_le_u32(&mut buf[off..off + 4], value & 0x0FFFFFFF);
    dev.write_sector(sector, &buf)?;
    Ok(())
}

/// Find a free cluster by scanning the FAT (very naive).
pub fn find_free_cluster<D: BlockDevice>(dev: &D, bpb: &Bpb, start_from: u32) -> Result<u32> {
    let mut c = if start_from < 2 { 2 } else { start_from };
    let max_iters = 1_000_000u32;

    for _ in 0..max_iters {
        let v = read_fat_entry(dev, bpb, c)?;
        if v == 0 {
            return Ok(c);
        }
        c += 1;
    }
    Err(Error::NoSpace)
}
