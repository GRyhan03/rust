//! FAT32 BPB / boot sector parsing.

use crate::error::{Error, Result};

/// Parsed FAT32 BPB (Boot Parameter Block) fields required by this MVP.
#[derive(Debug, Clone, Copy)]
pub struct Bpb {
    /// Bytes per sector (usually 512).
    pub bytes_per_sector: u16,
    /// Sectors per cluster (power of two).
    pub sectors_per_cluster: u8,
    /// Reserved sectors before the FAT region.
    pub reserved_sectors: u16,
    /// Number of FATs (usually 2).
    pub num_fats: u8,
    /// Total sectors (FAT32 uses 32-bit field).
    pub total_sectors_32: u32,
    /// FAT size in sectors (FAT32 field).
    pub fat_size_32: u32,
    /// Root directory first cluster.
    pub root_cluster: u32,
    /// FSInfo sector (optional, not used in MVP).
    pub fsinfo_sector: u16,
}

fn le_u16(x: &[u8]) -> u16 {
    u16::from_le_bytes([x[0], x[1]])
}
fn le_u32(x: &[u8]) -> u32 {
    u32::from_le_bytes([x[0], x[1], x[2], x[3]])
}

impl Bpb {
    /// Parse FAT32 BPB from a 512-byte boot sector.
    pub fn parse(boot: &[u8; 512]) -> Result<Self> {
        // Signature check (0x55AA at the end)
        if boot[510] != 0x55 || boot[511] != 0xAA {
            return Err(Error::InvalidBootSector);
        }

        let bytes_per_sector = le_u16(&boot[11..13]);
        let sectors_per_cluster = boot[13];
        let reserved_sectors = le_u16(&boot[14..16]);
        let num_fats = boot[16];
        let root_entry_count = le_u16(&boot[17..19]); // must be 0 for FAT32
        let fat_size_16 = le_u16(&boot[22..24]);

        let total_sectors_32 = le_u32(&boot[32..36]);
        let fat_size_32 = le_u32(&boot[36..40]);
        let root_cluster = le_u32(&boot[44..48]);
        let fsinfo_sector = le_u16(&boot[48..50]);

        // Minimal validation for FAT32.
        if bytes_per_sector != 512 {
            return Err(Error::InvalidBootSector);
        }
        if root_entry_count != 0 {
            return Err(Error::NotFat32);
        }
        if fat_size_16 != 0 {
            return Err(Error::NotFat32);
        }
        if fat_size_32 == 0 || root_cluster < 2 {
            return Err(Error::InvalidBootSector);
        }
        if sectors_per_cluster == 0 || (sectors_per_cluster & (sectors_per_cluster - 1)) != 0 {
            return Err(Error::InvalidBootSector);
        }
        if reserved_sectors == 0 || num_fats == 0 {
            return Err(Error::InvalidBootSector);
        }

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors_32,
            fat_size_32,
            root_cluster,
            fsinfo_sector,
        })
    }
}
