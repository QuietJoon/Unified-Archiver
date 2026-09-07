//! FFI bindings and native Rust archive backends
//!
//! This module provides both FFI wrappers around native C libraries
//! and native Rust implementations for optimal performance.

pub(crate) mod common; // Shared utilities (AtomicOutputFile, write_entry_atomically, normalize_path)
#[cfg(feature = "libarchive")]
pub mod libarchive; // libarchive manual bindings
#[cfg(feature = "libarchive")]
pub mod libarchive_wrapper; // libarchive safe wrapper
#[cfg(feature = "sevenzip")]
pub mod sevenz_wrapper; // Native Rust 7z backend with CRC32 metadata
#[cfg(feature = "rar-support")]
pub mod unrar; // UnRAR manual bindings
#[cfg(feature = "rar-support")]
pub mod wrapper; // UnRAR safe wrapper
#[cfg(feature = "zip-read")]
pub mod zip_wrapper; // Native Rust ZIP backend — sole reader/extractor for encrypted and unencrypted ZIP
#[cfg(feature = "zip-write")]
pub mod zip_writer; // Native Rust ZIP creation backend
