//! Block device abstraction.
//!
//! FAT32 is built on top of a sector-based device (usually 512 bytes per sector).

use crate::error::Result;

/// A minimal sector-based device.
///
/// Implementations must be able to read/write 512-byte sectors.
///
/// In `no_std`, you typically implement this trait for:
/// - a memory-mapped block device
/// - a driver
/// - an in-memory disk image (for tests)
pub trait BlockDevice {
    /// Read a 512-byte sector at `lba` into `buf`.
    fn read_sector(&self, lba: u64, buf: &mut [u8; 512]) -> Result<()>;

    /// Write a 512-byte sector at `lba` from `buf`.
    fn write_sector(&mut self, lba: u64, buf: &[u8; 512]) -> Result<()>;
}

#[cfg(test)]
use crate::error::Error;

/// Simple in-memory block device for tests.
///
/// Stores a full disk image inside a `Vec<u8>` (sector-aligned).
#[cfg(test)]
pub struct MemDevice {
    data: std::vec::Vec<u8>,
}

#[cfg(test)]
impl MemDevice {
    pub fn new(data: std::vec::Vec<u8>) -> Self {
        assert!(data.len() % 512 == 0);
        Self { data }
    }

    pub fn into_inner(self) -> std::vec::Vec<u8> {
        self.data
    }
}

#[cfg(test)]
impl BlockDevice for MemDevice {
    fn read_sector(&self, lba: u64, buf: &mut [u8; 512]) -> Result<()> {
        let off = (lba as usize) * 512;
        if off + 512 > self.data.len() {
            return Err(Error::Io);
        }
        buf.copy_from_slice(&self.data[off..off + 512]);
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8; 512]) -> Result<()> {
        let off = (lba as usize) * 512;
        if off + 512 > self.data.len() {
            return Err(Error::Io);
        }
        self.data[off..off + 512].copy_from_slice(buf);
        Ok(())
    }
}
