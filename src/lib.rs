#![no_std]

extern crate alloc;

#[cfg(not(test))]
mod allocator;

pub mod bpb;
pub mod device;
pub mod dir;
pub mod error;
pub mod fat;
pub mod fs;

pub use crate::error::{Error, Result};
pub use crate::fs::Fat32;
