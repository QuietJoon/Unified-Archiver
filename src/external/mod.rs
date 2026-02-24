//! External tool integrations
//!
//! This module provides wrappers for external command-line tools that
//! extend the functionality of unified-archive beyond what's available
//! through libraries.

#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub mod rar;

#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub use rar::RarCreator;
