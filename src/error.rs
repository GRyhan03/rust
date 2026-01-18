//! Errors for the FAT32 library.

/// Result alias used by this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// Errors returned by the FAT32 parser / filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Underlying device I/O error.
    Io,
    /// The boot sector is invalid or unsupported.
    InvalidBootSector,
    /// Not a FAT32 volume (or fields not supported).
    NotFat32,
    /// The requested file was not found.
    NotFound,
    /// Directory is full (no free entry).
    DirFull,
    /// No free cluster available.
    NoSpace,
    /// The provided name is invalid (only 8.3 supported in this MVP).
    InvalidName,
    /// Internal corruption (FAT chain, cluster values).
    Corrupt,
}
