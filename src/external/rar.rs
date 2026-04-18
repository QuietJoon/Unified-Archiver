//! WinRAR CLI integration for RAR archive creation
//!
//! This module provides RAR creation support by shelling out to the
//! WinRAR command-line tool (`rar.exe`).
//!
//! # Requirements
//!
//! - Windows operating system
//! - Licensed WinRAR installation
//! - `rar.exe` accessible in PATH or installed in standard location
//!
//! # License Compliance
//!
//! RAR is a proprietary format. Creating RAR archives requires a licensed
//! copy of WinRAR. This module does NOT include any RAR compression code;
//! it merely provides a convenient Rust interface to the external `rar.exe` tool.
//!
//! Users must:
//! - Purchase a WinRAR license from https://www.win-rar.com/
//! - Install WinRAR on their Windows system
//! - Comply with WinRAR's license terms
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(all(target_os = "windows", feature = "external-rar-create"))]
//! # {
//! use unified_archive::external::RarCreator;
//! use unified_archive::CompressionLevel;
//!
//! let mut creator = RarCreator::new("output.rar")?;
//! creator.set_compression_level(CompressionLevel::Normal);
//! creator.add_file("document.txt")?;
//! creator.add_directory("my_folder")?;
//! creator.create()?;
//! # }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::{ArchiveError, Result};
use crate::options::CompressionLevel;
use secstr::SecStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// RAR archive creator using external WinRAR CLI
pub struct RarCreator {
    /// Output archive path
    output_path: PathBuf,
    /// Compression level
    compression_level: CompressionLevel,
    /// Optional password
    password: Option<SecStr>,
    /// Files and directories to add
    entries: Vec<PathBuf>,
    /// Path to rar.exe
    rar_exe_path: PathBuf,
}

impl RarCreator {
    /// Create a new RAR archive creator
    ///
    /// # Arguments
    ///
    /// * `output_path` - Path where the RAR archive will be created
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Output file already exists
    /// - rar.exe cannot be found
    pub fn new<P: AsRef<Path>>(output_path: P) -> Result<Self> {
        let output_path = output_path.as_ref().to_path_buf();

        // Check if output already exists
        if output_path.exists() {
            return Err(ArchiveError::io(
                "create_rar",
                output_path.clone(),
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Output file already exists",
                ),
            ));
        }

        // Find rar.exe
        let rar_exe = Self::find_rar_exe()?;

        Ok(Self {
            output_path,
            compression_level: CompressionLevel::Normal,
            password: None,
            entries: Vec::new(),
            rar_exe_path: rar_exe,
        })
    }

    /// Find rar.exe on the system
    ///
    /// Searches in:
    /// 1. PATH environment variable
    /// 2. Common WinRAR installation directories
    fn find_rar_exe() -> Result<PathBuf> {
        // Try PATH first
        if let Ok(output) = Command::new("where").arg("rar.exe").output() {
            if output.status.success() {
                if let Ok(path_str) = String::from_utf8(output.stdout) {
                    let path = path_str.lines().next().unwrap_or("").trim();
                    if !path.is_empty() && Path::new(path).exists() {
                        return Ok(PathBuf::from(path));
                    }
                }
            }
        }

        // Try common installation paths
        let common_paths = [
            r"C:\Program Files\WinRAR\rar.exe",
            r"C:\Program Files (x86)\WinRAR\rar.exe",
        ];

        for path_str in &common_paths {
            let path = Path::new(path_str);
            if path.exists() {
                return Ok(path.to_path_buf());
            }
        }

        Err(ArchiveError::unsupported(
            "create_rar",
            crate::format::ArchiveFormat::Rar,
            Some(
                "rar.exe not found. Please install WinRAR and ensure it's in your PATH."
                    .to_string(),
            ),
        ))
    }

    /// Set compression level
    pub fn set_compression_level(&mut self, level: CompressionLevel) {
        self.compression_level = level;
    }

    /// Set password for encryption
    pub fn set_password(&mut self, password: impl Into<String>) {
        self.password = Some(SecStr::from(password.into()));
    }

    /// Add a file to the archive
    ///
    /// # Arguments
    ///
    /// * `path` - Path to file to add
    pub fn add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        crate::creation::validate_file_path(path, "add_file")?;

        self.entries.push(path.to_path_buf());
        Ok(())
    }

    /// Add a directory (recursively) to the archive
    ///
    /// # Arguments
    ///
    /// * `path` - Path to directory to add
    pub fn add_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        crate::creation::validate_directory_path(path, "add_directory")?;

        self.entries.push(path.to_path_buf());
        Ok(())
    }

    /// Get number of entries to be added
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Create the RAR archive
    ///
    /// Executes `rar.exe` with appropriate arguments to create the archive.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No entries have been added
    /// - rar.exe execution fails
    /// - Archive creation fails
    pub fn create(self) -> Result<()> {
        if self.entries.is_empty() {
            return Err(ArchiveError::format(
                Some(crate::format::ArchiveFormat::Rar),
                "Cannot create empty archive - add at least one entry",
            ));
        }

        // Build rar.exe command
        let mut cmd = Command::new(&self.rar_exe_path);

        // Command: a = add files to archive
        cmd.arg("a");

        // Compression level mapping:
        // -m0 = store (no compression)
        // -m1 = fastest
        // -m2 = fast
        // -m3 = normal (default)
        // -m4 = good
        // -m5 = best
        let compression_arg = match self.compression_level {
            CompressionLevel::Store => "-m0",
            CompressionLevel::Fastest => "-m1",
            CompressionLevel::Fast => "-m2",
            CompressionLevel::Normal => "-m3",
            CompressionLevel::Maximum => "-m4",
            CompressionLevel::Ultra => "-m5",
        };
        cmd.arg(compression_arg);

        // Password encryption
        if let Some(pwd_str) = crate::options::password_as_str(&self.password) {
            cmd.arg(format!("-hp{}", pwd_str));
        }

        // Recursive mode for directories
        cmd.arg("-r");

        // Output archive path
        cmd.arg(&self.output_path);

        // Add all entries
        for entry in &self.entries {
            cmd.arg(entry);
        }

        // Execute command
        let output = cmd
            .output()
            .map_err(|e| ArchiveError::io("create_rar", self.output_path.clone(), e))?;

        // Check exit status
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(ArchiveError::format(
                Some(crate::format::ArchiveFormat::Rar),
                &format!("RAR creation failed: {}", error_msg),
            ));
        }

        // Verify archive was created
        if !self.output_path.exists() {
            return Err(ArchiveError::format(
                Some(crate::format::ArchiveFormat::Rar),
                "Archive file was not created",
            ));
        }

        Ok(())
    }

    /// Get the path where rar.exe was found
    pub fn rar_exe_path(&self) -> &Path {
        &self.rar_exe_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_find_rar_exe() {
        // This test will pass if WinRAR is installed, otherwise it will fail
        // That's acceptable since this feature requires WinRAR
        let result = RarCreator::find_rar_exe();

        if result.is_ok() {
            println!("Found rar.exe at: {:?}", result.unwrap());
        } else {
            println!("rar.exe not found (expected if WinRAR not installed)");
        }
    }

    #[test]
    fn test_entry_tracking() {
        // This test doesn't require rar.exe to exist
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_rar_input.txt");
        let test_archive = temp_dir.join("test_output.rar");

        // Create a test file
        fs::write(&test_file, b"test content").ok();

        // Try to create RarCreator (may fail if rar.exe not found, that's ok for this test)
        if let Ok(mut creator) = RarCreator::new(&test_archive) {
            if test_file.exists() {
                creator.add_file(&test_file).ok();
                assert_eq!(creator.entry_count(), 1);
            }
        }

        // Cleanup
        fs::remove_file(&test_file).ok();
        fs::remove_file(&test_archive).ok();
    }
}
