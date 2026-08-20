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
use crate::password::Password;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Map a library [`CompressionLevel`] to the WinRAR `-mN` CLI flag.
///
/// WinRAR accepts `-m0` (store) through `-m5` (best). Pulled into a
/// dedicated helper so the codec mapping is one named table instead of
/// an inline `match` mid-`create_archive`, and so docs/tests can refer
/// to the policy in one place.
fn rar_compression_flag(level: CompressionLevel) -> &'static str {
    match level {
        CompressionLevel::Store => "-m0",
        CompressionLevel::Fastest => "-m1",
        CompressionLevel::Fast => "-m2",
        CompressionLevel::Normal => "-m3",
        CompressionLevel::Maximum => "-m4",
        CompressionLevel::Ultra => "-m5",
    }
}

/// RAR archive creator using external WinRAR CLI
pub struct RarCreator {
    /// Output archive path
    output_path: PathBuf,
    /// Compression level
    compression_level: CompressionLevel,
    /// Optional password
    password: Option<Password>,
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
        Self::assemble(output_path.as_ref().to_path_buf(), Self::find_rar_exe()?)
    }

    /// Construct a [`RarCreator`] with an explicit `rar.exe` path.
    ///
    /// Bypasses the `PATH` / Program-Files discovery in
    /// [`Self::new`]. Use this for portable installs, deterministic
    /// tests, and custom WinRAR locations the default discovery
    /// cannot find. Verifies the path exists and points at a regular
    /// file before returning.
    pub fn with_rar_exe_path<P, R>(output_path: P, rar_exe_path: R) -> Result<Self>
    where
        P: AsRef<Path>,
        R: AsRef<Path>,
    {
        let rar_exe = rar_exe_path.as_ref().to_path_buf();
        if !rar_exe.is_file() {
            return Err(ArchiveError::unsupported(
                "create_rar",
                crate::format::ArchiveFormat::Rar,
                Some(format!(
                    "rar.exe path '{}' does not exist or is not a regular file",
                    rar_exe.display()
                )),
            ));
        }
        Self::assemble(output_path.as_ref().to_path_buf(), rar_exe)
    }

    /// Shared post-construction step for the public constructors:
    /// rejects existing output paths and stamps the defaults.
    fn assemble(output_path: PathBuf, rar_exe_path: PathBuf) -> Result<Self> {
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
        Ok(Self {
            output_path,
            compression_level: CompressionLevel::Normal,
            password: None,
            entries: Vec::new(),
            rar_exe_path,
        })
    }

    /// Find `rar.exe` on the system.
    ///
    /// Discovery is intentionally narrow:
    /// 1. `where rar.exe` against the current `PATH`.
    /// 2. The two stock WinRAR install paths
    ///    (`C:\Program Files\WinRAR\rar.exe`,
    ///    `C:\Program Files (x86)\WinRAR\rar.exe`).
    ///
    /// **Limitation:** custom or portable WinRAR installs (e.g. on a
    /// non-system drive, or extracted into a per-user directory) are not
    /// auto-detected by `find_rar_exe`. If your install lives elsewhere
    /// either add its directory to `PATH`, or construct [`RarCreator`] via
    /// [`RarCreator::with_rar_exe_path`] which bypasses discovery
    /// entirely. Registry-based auto-discovery is tracked as a future
    /// improvement.
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

    /// Set password for encryption.
    ///
    /// **Security limitation:** the external WinRAR CLI accepts the
    /// password through the `-hp{password}` switch, which becomes part of
    /// `rar.exe`'s command-line argument vector. While the rar process is
    /// alive the password is therefore visible to anything that can list
    /// running processes (`tasklist`, `Get-Process`, telemetry agents,
    /// crash reporters, parental-control software, …). The local
    /// [`Password`](crate::Password) (backed by `secstr`) only protects
    /// the value inside this process's address space; it cannot mask
    /// argv on Windows.
    ///
    /// For workloads where password leakage to the local process listing
    /// is unacceptable, prefer the in-process `rar-support` Cargo feature
    /// (UnRAR for read; native RAR creation is not currently shipped) or
    /// use a non-CLI archiver. This module exists to make the WinRAR
    /// integration path explicit and convenient on Windows, not to keep
    /// secrets opaque from the OS.
    pub fn set_password(&mut self, password: impl Into<String>) {
        self.password = Some(Password::new(password));
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
    ///
    /// # Security
    ///
    /// When a password is set, `-hp{password}` is appended to `rar.exe`'s
    /// argument vector. That makes the password observable in OS-level
    /// process listings while the worker runs. See
    /// [`RarCreator::set_password`] for the full caveat and recommended
    /// alternatives.
    pub fn create(self) -> Result<()> {
        if self.entries.is_empty() {
            return Err(ArchiveError::format(
                Some(crate::format::ArchiveFormat::Rar),
                "Cannot create empty archive - add at least one entry",
            ));
        }

        // Build rar.exe command
        let mut cmd = Command::new(&self.rar_exe_path);
        cmd.args(self.build_create_args()?);

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

    /// Build the argument vector (everything after the program name)
    /// for the `rar a` invocation. Factored out of [`Self::create`] so
    /// the switch/path ordering — the `--` end-of-switches sentinel and
    /// the `./` re-rooting of leading-dash entry paths (R0079-0039) —
    /// is unit-testable without spawning `rar.exe`.
    fn build_create_args(&self) -> Result<Vec<std::ffi::OsString>> {
        let mut args: Vec<std::ffi::OsString> = Vec::new();

        // Command: a = add files to archive
        args.push("a".into());

        // Codec mapping lives in `rar_compression_flag`
        // so the CLI policy is one named helper instead of an inline
        // table tangled with process-spawning.
        args.push(rar_compression_flag(self.compression_level).into());

        // Password encryption
        if let Some(pw) = self.password.as_ref() {
            args.push(format!("-hp{}", pw.as_str()).into());
        }

        // Recursive mode for directories
        args.push("-r".into());

        // End-of-switches sentinel: everything after "--" is treated as
        // a path, so an output or entry path beginning with '-' cannot
        // be parsed as a WinRAR switch (R0079-0039).
        args.push("--".into());

        // Output archive path
        args.push(self.output_path.clone().into());

        // Add all entries. Belt and suspenders for rar builds that do
        // not honor "--": re-root a leading-dash entry under "." so it
        // can never scan as a switch.
        for entry in &self.entries {
            if entry.to_string_lossy().starts_with('-') {
                args.push(Path::new(".").join(entry).into());
            } else {
                args.push(entry.clone().into());
            }
        }

        Ok(args)
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

    /// R0079-0039: the argv must carry a `--` end-of-switches sentinel
    /// before any path, and leading-dash entry paths must be re-rooted
    /// under `.` so they cannot be parsed as switches by rar builds
    /// that ignore `--`.
    #[test]
    fn create_args_protect_leading_dash_entries() {
        use std::ffi::{OsStr, OsString};

        let creator = RarCreator {
            output_path: PathBuf::from("out.rar"),
            compression_level: CompressionLevel::Normal,
            password: None,
            entries: vec![PathBuf::from("-sw.txt"), PathBuf::from("normal.txt")],
            rar_exe_path: PathBuf::from("rar.exe"),
        };
        let args = creator.build_create_args().expect("argv builds");

        let sep = args
            .iter()
            .position(|a| a.as_os_str() == OsStr::new("--"))
            .expect("'--' sentinel present");
        let out = args
            .iter()
            .position(|a| a.as_os_str() == OsStr::new("out.rar"))
            .expect("output path present");
        assert!(sep < out, "'--' must precede the output path: {:?}", args);

        let rerooted: OsString = Path::new(".").join("-sw.txt").into();
        assert!(
            args.contains(&rerooted),
            "leading-dash entry must be re-rooted: {:?}",
            args
        );
        assert!(args.contains(&OsString::from("normal.txt")));
        assert!(
            !args
                .iter()
                .skip(sep + 1)
                .any(|a| a.to_string_lossy().starts_with('-')),
            "no post-sentinel argument may begin with '-': {:?}",
            args
        );
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
