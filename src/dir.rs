//! Directory entry parsing (8.3 only in this MVP).

use crate::error::{Error, Result};

/// A parsed 8.3 directory entry (short name only).
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// 11 bytes name (8 + 3) as stored on disk.
    pub raw_name: [u8; 11],
    pub attr: u8,
    pub first_cluster: u32,
    pub file_size: u32,
}

fn le_u16(x: &[u8]) -> u16 {
    u16::from_le_bytes([x[0], x[1]])
}
fn le_u32(x: &[u8]) -> u32 {
    u32::from_le_bytes([x[0], x[1], x[2], x[3]])
}

impl DirEntry {
    /// Parse a directory entry from a 32-byte record.
    pub fn parse(rec: &[u8; 32]) -> Result<Option<Self>> {
        let first = rec[0];
        if first == 0x00 {
            // End of directory.
            return Ok(None);
        }
        if first == 0xE5 {
            // Deleted (skip)
            return Ok(Some(Self {
                raw_name: [0; 11],
                attr: 0,
                first_cluster: 0,
                file_size: 0,
            }));
        }

        let attr = rec[11];
        // Skip LFN entries (attr == 0x0F).
        if attr == 0x0F {
            return Ok(Some(Self {
                raw_name: [0; 11],
                attr,
                first_cluster: 0,
                file_size: 0,
            }));
        }

        let mut raw_name = [0u8; 11];
        raw_name.copy_from_slice(&rec[0..11]);

        let hi = le_u16(&rec[20..22]) as u32;
        let lo = le_u16(&rec[26..28]) as u32;
        let first_cluster = (hi << 16) | lo;
        let file_size = le_u32(&rec[28..32]);

        Ok(Some(Self {
            raw_name,
            attr,
            first_cluster,
            file_size,
        }))
    }

    /// Build an on-disk 32-byte entry for a short name file (minimal fields).
    pub fn build_short_file(name_83: [u8; 11], first_cluster: u32, file_size: u32) -> [u8; 32] {
        let mut rec = [0u8; 32];
        rec[0..11].copy_from_slice(&name_83);
        rec[11] = 0x20; // archive

        let hi = ((first_cluster >> 16) as u16).to_le_bytes();
        let lo = ((first_cluster & 0xFFFF) as u16).to_le_bytes();
        rec[20..22].copy_from_slice(&hi);
        rec[26..28].copy_from_slice(&lo);

        rec[28..32].copy_from_slice(&file_size.to_le_bytes());
        rec
    }
}

/// Convert a human name like "HELLO.TXT" to FAT 8.3 (11 bytes).
///
/// This MVP supports only ASCII uppercase letters, digits, '_' and '-'.
pub fn to_short_name_83(s: &str) -> Result<[u8; 11]> {
    let mut out = [b' '; 11];

    let (name, ext) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    };

    if name.is_empty() || name.len() > 8 || ext.len() > 3 {
        return Err(Error::InvalidName);
    }

    fn ok_char(c: u8) -> bool {
        (b'A'..=b'Z').contains(&c)
            || (b'0'..=b'9').contains(&c)
            || c == b'_'
            || c == b'-'
    }

    for (i, ch) in name.bytes().enumerate() {
        let up = ch.to_ascii_uppercase();
        if !ok_char(up) {
            return Err(Error::InvalidName);
        }
        out[i] = up;
    }
    for (i, ch) in ext.bytes().enumerate() {
        let up = ch.to_ascii_uppercase();
        if !ok_char(up) {
            return Err(Error::InvalidName);
        }
        out[8 + i] = up;
    }

    Ok(out)
}
