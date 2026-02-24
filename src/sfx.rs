//! Self-Extracting Archive (SFX) Detection and Handling
//!
//! This module provides functionality to detect and handle self-extracting archives
//! that combine executable stubs with embedded archive data.
//!
//! ## Detection Pipeline
//!
//! 3-stage detection process with early exit optimization:
//! 1. **Executable Validation** (~1ms): Check if file is PE/ELF/Mach-O/Script
//! 2. **Signature Scanning** (~10-50ms): Search first 1MB for archive signatures
//! 3. **Archive Validation** (~10-20ms): Verify archive structure at detected offset
//!
//! ## Platform Support
//!
//! - **Windows**: PE executables (.exe) with embedded archives
//! - **Linux/BSD**: ELF executables with embedded archives
//! - **macOS**: Mach-O executables with embedded archives
//! - **Unix**: Shell scripts with appended archives (makeself-style)
//!
//! ## Example
//!
//! ```no_run
//! use unified_archive::Archive;
//!
//! // Detect if file is SFX
//! let result = Archive::detect_sfx("installer.exe")?;
//! if result.is_sfx {
//!     println!("SFX detected: {}", result.summary());
//!     println!("Archive offset: {:?}", result.data_offset);
//!     println!("Stub type: {:?}", result.stub_type);
//!     println!("Archive format: {:?}", result.archive_format);
//! }
//!
//! // Open SFX archive directly
//! let archive = Archive::open_sfx("installer.exe")?;
//! let entries = archive.list_files()?;
//! # Ok::<(), unified_archive::ArchiveError>(())
//! ```

pub mod detection;
pub mod result;
pub mod signatures;
pub mod stub_types;

// Re-export public types
pub use detection::{detect_sfx, validate_archive_at_offset};
pub use result::SfxDetectionResult;
pub use signatures::{Signature, find_first_signature, scan_for_signatures};
pub use stub_types::StubType;
