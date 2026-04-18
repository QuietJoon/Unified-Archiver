//! FFI bindings and native Rust archive backends
//!
//! This module provides both FFI wrappers around native C libraries
//! and native Rust implementations for optimal performance.

pub(crate) mod common; // Shared utilities (TempDirGuard, normalize_path)
pub mod libarchive; // libarchive manual bindings
pub mod libarchive_wrapper; // libarchive safe wrapper
pub mod piz_wrapper; // Native Rust ZIP backend with parallel extraction and CRC32 metadata
pub mod sevenz_wrapper; // Native Rust 7z backend with CRC32 metadata
#[cfg(feature = "rar-support")]
pub mod unrar; // UnRAR manual bindings
#[cfg(feature = "rar-support")]
pub mod wrapper; // UnRAR safe wrapper
pub mod zip_wrapper; // Native Rust ZIP reading backend (encrypted ZIP support)
pub mod zip_writer; // Native Rust ZIP creation backend
