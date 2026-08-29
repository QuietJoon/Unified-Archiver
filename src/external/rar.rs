//! WinRAR CLI integration for RAR archive creation.
//!
//! Creates RAR archives by driving the external WinRAR console binary
//! (`rar.exe`). This is a *thin, optional adapter*: no RAR compression
//! code lives here, and every decision the adapter makes about the
//! external process is factored into its own child module.
//!
//! # Requirements
//!
//! - Windows.
//! - A licensed WinRAR installation, at least
//!   [`version::MINIMUM_RAR_VERSION`].
//! - `rar.exe` on `PATH` or in a stock install directory — or an
//!   explicit path via [`RarCreator::with_rar_exe_path`].
//!
//! # What is guaranteed
//!
//! - **Arguments are a vector, never a command string.** See the
//!   [`argv`] contract: no shell, no `cmd.exe`, a `--`
//!   end-of-switches sentinel before every path, and leading-dash paths
//!   re-rooted under `.` as a second line of defence. Inputs that cannot
//!   cross the process boundary intact — interior NULs, an empty
//!   password that would open an interactive prompt — are refused with a
//!   typed error before anything is spawned.
//! - **The exit code is always interpreted.** See [`exit`]: `0` is the
//!   only success, each documented non-zero code becomes its own
//!   [`exit::RarExit`] variant, and an unrecognised code or a process
//!   that reported no code at all is a failure. A success code is
//!   additionally checked against the artifact actually existing.
//! - **The binary is identified before it is used.** See [`version`]:
//!   an `unrar`, a pre-5.0 `rar`, or a program that prints no
//!   recognisable banner is refused rather than driven blind.
//! - **Discovery cannot be hijacked.** See [`discovery`]: `PATH` is
//!   walked in-process (no `where.exe` helper), relative `PATH` entries
//!   are skipped, and batch shims are refused because `cmd.exe` would
//!   re-parse the argument vector.
//!
//! # The fallback when the binary is missing or unusable
//!
//! There is no silent fallback and no partial archive: the lane refuses
//! with a [`RarCliError`] that names what is missing, and the caller
//! decides. RAR creation is not offered by the main [`crate::Archive`]
//! facade at all — `encryption_write`, `multipart_write` and
//! modification are `Support::None` for `Rar`/`Rar5` — so there is no
//! in-crate alternative to degrade to, and the honest answer is a typed
//! refusal rather than a guess.
//!
//! The two cases a caller acts on differently stay distinguishable
//! **by variant** all the way out:
//!
//! | Situation | [`RarCliError`] | After `?` into [`crate::Result`] |
//! |---|---|---|
//! | binary absent | [`RarCliError::BinaryNotFound`] | [`ArchiveError::CodecUnavailable`] |
//! | binary too old / wrong program / unidentifiable / not runnable as a program | [`RarCliError::UnsupportedVersion`], [`RarCliError::WrongBinary`], [`RarCliError::VersionProbeFailed`], [`RarCliError::ProgramRejected`] | [`ArchiveError::Unsupported`] |
//! | not a Windows host | [`RarCliError::UnsupportedPlatform`] | [`ArchiveError::Unsupported`] |
//!
//! Call [`RarCreator::check_availability`] to learn all of this *before*
//! committing to a RAR output path, so a caller can plan a different
//! format instead of discovering the refusal mid-pipeline.
//!
//! # License compliance
//!
//! RAR is a proprietary format. Creating RAR archives requires a
//! licensed copy of WinRAR. This module contains no RAR compression
//! code; it is an interface to the external `rar.exe` tool. Users must
//! purchase a licence from <https://www.win-rar.com/>, install WinRAR,
//! and comply with its licence terms.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(all(target_os = "windows", feature = "external-rar-create"))]
//! # {
//! use unified_archive::external::{RarCreator, RarCliError};
//! use unified_archive::CompressionLevel;
//!
//! // Plan ahead: is the lane usable at all?
//! match RarCreator::check_availability() {
//!     Ok((rar_exe, version)) => println!("using {} ({version})", rar_exe.display()),
//!     Err(error) if error.is_binary_unavailable() => {
//!         // Install it, or pick another format.
//!         return Err(error.into());
//!     }
//!     Err(error) => {
//!         // Upgrade it, or point at the right binary.
//!         return Err(error.into());
//!     }
//! }
//!
//! let mut creator = RarCreator::new("output.rar")?;
//! creator.set_compression_level(CompressionLevel::Normal);
//! creator.add_file("document.txt")?;
//! creator.add_directory("my_folder")?;
//! creator.create()?;
//! # }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod argv;
pub mod discovery;
pub mod error;
pub mod exit;
pub mod runner;
pub mod session;
pub mod version;

use std::path::{Path, PathBuf};

use crate::error::ArchiveError;
use crate::format::ArchiveFormat;
use crate::options::CompressionLevel;
use crate::password::Password;

pub use argv::ArgvError;
pub use discovery::ProgramVetError;
pub use error::RarCliError;
pub use exit::RarExit;
pub use runner::{CommandOutcome, CommandRunner, SystemRunner};
pub use version::{MINIMUM_RAR_VERSION, RarFlavor, RarVersion};

/// Codec label used when this lane reports itself unavailable.
const RAR_CLI_CODEC: &str = "WinRAR CLI (rar)";

/// Operation label carried by the [`ArchiveError`] conversions.
const RAR_CLI_OPERATION: &str = "create_rar";

/// Map a library [`CompressionLevel`] to the WinRAR `-mN` CLI flag.
///
/// WinRAR accepts `-m0` (store) through `-m5` (best). Pulled into a
/// dedicated helper so the codec mapping is one named table instead of
/// an inline `match` mid-`create`, and so docs and tests can refer to the
/// policy in one place. [`argv::build`] independently re-validates that
/// the value is one of those six switches, so this slot cannot become a
/// route for an arbitrary switch.
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

/// Convert a lane failure into the crate-wide error type.
///
/// This is the only crate-coupled part of the lane; the mapping keeps
/// the install-vs-upgrade split visible **by variant** after conversion,
/// so a caller that only ever sees [`ArchiveError`] can still tell the
/// two apart without reading message text.
impl From<RarCliError> for ArchiveError {
    fn from(error: RarCliError) -> Self {
        // Rendered before destructuring so every arm can name the
        // failure without re-deriving the text, and so the password can
        // never reach it (`Display` uses the redacted argv).
        let remediation = error.remediation();
        let text = error.to_string();
        let password_problem = error.is_password_failure();

        match error {
            // "install it"
            RarCliError::BinaryNotFound { .. } => Self::CodecUnavailable {
                codec: RAR_CLI_CODEC.to_string(),
                format: ArchiveFormat::Rar,
                install_instructions: format!("{text}. {remediation}"),
            },
            // "upgrade it" / "wrong program" / "no lane on this host"
            RarCliError::UnsupportedPlatform { .. }
            | RarCliError::UnsupportedVersion { .. }
            | RarCliError::WrongBinary { .. }
            | RarCliError::VersionProbeFailed { .. }
            | RarCliError::ProgramRejected { .. } => Self::unsupported(
                RAR_CLI_OPERATION,
                ArchiveFormat::Rar,
                Some(format!("{text}. {remediation}")),
            ),
            RarCliError::SpawnFailed { rar_exe, source } => {
                Self::io(RAR_CLI_OPERATION, rar_exe, source)
            }
            RarCliError::OutputExists { output } => Self::io(
                RAR_CLI_OPERATION,
                output,
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Output file already exists",
                ),
            ),
            // A password problem stays a password error so the caller can
            // re-prompt instead of aborting the pipeline.
            _ if password_problem => Self::password(text),
            _ => Self::format(Some(ArchiveFormat::Rar), text),
        }
    }
}

/// RAR archive creator driving the external WinRAR CLI.
///
/// Construct it, describe the archive, then [`RarCreator::create`]. Every
/// method that touches the external binary returns [`RarCliError`], which
/// converts into [`ArchiveError`] with `?` — see the module-level
/// fallback table.
pub struct RarCreator {
    /// Output archive path.
    output_path: PathBuf,
    /// Compression level.
    compression_level: CompressionLevel,
    /// Optional password.
    password: Option<Password>,
    /// Files and directories to add.
    entries: Vec<PathBuf>,
    /// Path to the vetted `rar` binary.
    rar_exe_path: PathBuf,
    /// Process-execution seam. [`SystemRunner`] in production.
    runner: Box<dyn CommandRunner>,
}

impl RarCreator {
    /// Create a new RAR archive creator, discovering `rar.exe`.
    ///
    /// # Errors
    ///
    /// - [`RarCliError::OutputExists`] if the output path is occupied.
    /// - [`RarCliError::BinaryNotFound`] if no `rar` binary was found;
    ///   the error names every candidate that was probed.
    /// - [`RarCliError::UnsupportedPlatform`] on a non-Windows target.
    ///
    /// Discovery does **not** run the binary. The version gate applies
    /// at [`RarCreator::create`], or on demand via
    /// [`RarCreator::probe_version`] / [`RarCreator::check_availability`].
    pub fn new<P: AsRef<Path>>(output_path: P) -> Result<Self, RarCliError> {
        let rar_exe = Self::find_rar_exe()?;
        Self::assemble(
            output_path.as_ref().to_path_buf(),
            rar_exe,
            Box::new(SystemRunner),
        )
    }

    /// Construct a [`RarCreator`] with an explicit `rar.exe` path.
    ///
    /// Bypasses discovery — use it for portable installs, deterministic
    /// tests, and custom WinRAR locations. It does **not** bypass
    /// vetting: the path must be absolute, must name an existing regular
    /// file, and must not be a `.bat`/`.cmd` shim, because the
    /// [`argv`] quoting contract cannot survive `cmd.exe`'s
    /// re-parsing. A rejection is
    /// [`RarCliError::ProgramRejected`], which carries the specific
    /// [`ProgramVetError`].
    pub fn with_rar_exe_path<P, R>(output_path: P, rar_exe_path: R) -> Result<Self, RarCliError>
    where
        P: AsRef<Path>,
        R: AsRef<Path>,
    {
        Self::with_command_runner(output_path, rar_exe_path, Box::new(SystemRunner))
    }

    /// Construct a [`RarCreator`] with an explicit binary path *and* a
    /// custom [`CommandRunner`].
    ///
    /// The runner is the seam that makes this lane testable without a
    /// licensed WinRAR: substitute an implementation that records the
    /// argument vector and returns a scripted exit code. It is also the
    /// hook for callers that must supervise the child process
    /// themselves (job objects, sandboxes, custom timeouts).
    ///
    /// The supplied path is vetted exactly as in
    /// [`RarCreator::with_rar_exe_path`].
    pub fn with_command_runner<P, R>(
        output_path: P,
        rar_exe_path: R,
        runner: Box<dyn CommandRunner>,
    ) -> Result<Self, RarCliError>
    where
        P: AsRef<Path>,
        R: AsRef<Path>,
    {
        let rar_exe = session::vet_explicit_program(rar_exe_path.as_ref(), |p: &Path| p.is_file())?;
        Self::assemble(output_path.as_ref().to_path_buf(), rar_exe, runner)
    }

    /// Shared post-construction step: rejects occupied output paths and
    /// stamps the defaults.
    fn assemble(
        output_path: PathBuf,
        rar_exe_path: PathBuf,
        runner: Box<dyn CommandRunner>,
    ) -> Result<Self, RarCliError> {
        if output_path.exists() {
            return Err(RarCliError::OutputExists {
                output: output_path,
            });
        }
        Ok(Self {
            output_path,
            compression_level: CompressionLevel::Normal,
            password: None,
            entries: Vec::new(),
            rar_exe_path,
            runner,
        })
    }

    /// Find the `rar` binary on this system.
    ///
    /// `PATH` is walked **in-process**: the previous implementation
    /// shelled out to `where rar.exe`, which resolved `where` itself
    /// through `PATH` — and on Windows `CreateProcess` searches the
    /// current directory first, so a planted `where.exe` would have been
    /// executed. See [`discovery`] for the rest of the rules (relative
    /// `PATH` entries skipped, batch shims refused).
    ///
    /// After `PATH`, the stock WinRAR directories under `%ProgramFiles%`
    /// and `%ProgramFiles(x86)%` are probed, falling back to the `C:`
    /// literals when those variables are unreadable.
    ///
    /// **Limitation:** a portable install that is neither on `PATH` nor
    /// in a stock directory is not auto-detected; pass its path to
    /// [`RarCreator::with_rar_exe_path`]. Registry-based discovery
    /// remains a future improvement.
    fn find_rar_exe() -> Result<PathBuf, RarCliError> {
        let path_var = std::env::var_os("PATH");
        let install_dirs = discovery::default_install_dirs(
            std::env::var_os("ProgramFiles").as_deref(),
            std::env::var_os("ProgramFiles(x86)").as_deref(),
        );
        match discovery::find_rar_binary(path_var.as_deref(), &install_dirs, |p: &Path| p.is_file())
        {
            discovery::DiscoveryOutcome::Found(path) => Ok(path),
            discovery::DiscoveryOutcome::NotFound { searched } => {
                Err(RarCliError::BinaryNotFound { searched })
            }
            discovery::DiscoveryOutcome::UnsupportedPlatform { os } => {
                Err(RarCliError::UnsupportedPlatform { os })
            }
        }
    }

    /// Answer "can this process create RAR archives at all?" without
    /// committing to an output path.
    ///
    /// Runs discovery and the version gate and returns the binary and
    /// its version, or the typed refusal naming what is missing. This is
    /// the call a caller uses to plan: on
    /// [`RarCliError::is_binary_unavailable`] there is nothing to
    /// install-and-retry within the process, and the caller should
    /// choose a format this crate creates natively (ZIP, 7z, TAR).
    pub fn check_availability() -> Result<(PathBuf, RarVersion), RarCliError> {
        let rar_exe = Self::find_rar_exe()?;
        let version = session::probe_binary(&SystemRunner, &rar_exe)?;
        Ok((rar_exe, version))
    }

    /// Identify the configured binary and check it against
    /// [`MINIMUM_RAR_VERSION`].
    ///
    /// Uses this creator's [`CommandRunner`], so a test double answers
    /// here too.
    pub fn probe_version(&self) -> Result<RarVersion, RarCliError> {
        session::probe_binary(self.runner.as_ref(), &self.rar_exe_path)
    }

    /// Set compression level.
    pub fn set_compression_level(&mut self, level: CompressionLevel) {
        self.compression_level = level;
    }

    /// Set password for encryption.
    ///
    /// **Security limitation:** the external WinRAR CLI accepts the
    /// password through the `-hp{password}` switch, which becomes part of
    /// `rar.exe`'s argument vector. While the rar process is alive the
    /// password is therefore visible to anything that can list running
    /// processes (`tasklist`, `Get-Process`, telemetry agents, crash
    /// reporters, parental-control software, …). The local
    /// [`Password`](crate::Password) (backed by `secstr`) only protects
    /// the value inside this process's address space; it cannot mask
    /// argv on Windows. This crate's own diagnostics never carry the
    /// value — see [`argv::redact_argv`] — but the OS still can.
    ///
    /// For workloads where password leakage to the local process listing
    /// is unacceptable, prefer the in-process `rar-support` Cargo feature
    /// (UnRAR for read; native RAR creation is not shipped) or use a
    /// non-CLI archiver. This module exists to make the WinRAR
    /// integration path explicit and convenient on Windows, not to keep
    /// secrets opaque from the OS.
    ///
    /// An empty password is refused at [`RarCreator::create`] rather
    /// than sent: `-hp` with no payload makes `rar` open an interactive
    /// console prompt, which would hang the calling process.
    pub fn set_password(&mut self, password: impl Into<String>) {
        self.password = Some(Password::new(password));
    }

    /// Add a file to the archive.
    ///
    /// Returns [`crate::Result`] rather than [`RarCliError`]: this is the
    /// crate's shared creation-path validation, identical to every other
    /// creator, and nothing about the external binary is involved yet.
    pub fn add_file<P: AsRef<Path>>(&mut self, path: P) -> crate::Result<()> {
        let path = path.as_ref();
        crate::creation::validate_file_path(path, "add_file")?;
        self.entries.push(path.to_path_buf());
        Ok(())
    }

    /// Add a directory (recursively) to the archive.
    ///
    /// Returns [`crate::Result`] for the same reason as
    /// [`RarCreator::add_file`].
    pub fn add_directory<P: AsRef<Path>>(&mut self, path: P) -> crate::Result<()> {
        let path = path.as_ref();
        crate::creation::validate_directory_path(path, "add_directory")?;
        self.entries.push(path.to_path_buf());
        Ok(())
    }

    /// Number of entries queued.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Path of the vetted `rar` binary this creator will run.
    pub fn rar_exe_path(&self) -> &Path {
        &self.rar_exe_path
    }

    /// The exact argument vector this creator would hand to the OS, with
    /// the password redacted.
    ///
    /// Useful for logging and for reproducing a failure by hand. There
    /// is no un-redacted accessor by design.
    pub fn preview_argv(&self) -> Result<Vec<String>, RarCliError> {
        session::preview_argv(&self.request())
    }

    /// Create the RAR archive.
    ///
    /// Runs the full sequence documented on [`session`]: build the
    /// argument vector, re-check the destination, identify the binary,
    /// run it with a null stdin, classify the exit code, verify the
    /// artifact.
    ///
    /// # Errors
    ///
    /// - [`RarCliError::InvalidArguments`] — no entries, an empty
    ///   password, an unrepresentable path. Nothing is spawned.
    /// - [`RarCliError::OutputExists`] — the destination became occupied.
    /// - [`RarCliError::UnsupportedVersion`], [`RarCliError::WrongBinary`],
    ///   [`RarCliError::VersionProbeFailed`] — the binary is not one this
    ///   crate will drive. The archive is not attempted.
    /// - [`RarCliError::SpawnFailed`] — the binary could not be started.
    /// - [`RarCliError::CommandFailed`] — `rar` reported a non-zero (or
    ///   absent) exit code; the classified [`RarExit`] is attached.
    /// - [`RarCliError::OutputMissing`] — `rar` reported success but no
    ///   archive exists.
    ///
    /// # Security
    ///
    /// When a password is set, `-hp{password}` is part of the argument
    /// vector and is therefore observable in OS-level process listings
    /// while the child runs. See [`RarCreator::set_password`].
    pub fn create(self) -> Result<(), RarCliError> {
        session::create_archive(
            self.runner.as_ref(),
            &self.rar_exe_path,
            &self.request(),
            |p: &Path| p.exists(),
        )
    }

    /// Borrow the creator's state as an [`argv::AddArgv`].
    fn request(&self) -> argv::AddArgv<'_> {
        argv::AddArgv {
            compression_flag: rar_compression_flag(self.compression_level),
            password: self.password.as_ref().map(Password::as_str),
            recurse: true,
            output: &self.output_path,
            entries: &self.entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAR6: &str = "RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal   1 Oct 2023";

    /// Every level maps to a switch `argv::build` will accept — the two
    /// tables cannot drift apart without this failing.
    #[test]
    fn every_compression_level_maps_to_an_accepted_switch() {
        let levels = [
            (CompressionLevel::Store, "-m0"),
            (CompressionLevel::Fastest, "-m1"),
            (CompressionLevel::Fast, "-m2"),
            (CompressionLevel::Normal, "-m3"),
            (CompressionLevel::Maximum, "-m4"),
            (CompressionLevel::Ultra, "-m5"),
        ];
        let entries = vec![PathBuf::from("f.txt")];
        for (level, expected) in levels {
            let flag = rar_compression_flag(level);
            assert_eq!(flag, expected, "{level:?}");
            let spec = argv::AddArgv {
                compression_flag: flag,
                password: None,
                recurse: true,
                output: Path::new("out.rar"),
                entries: &entries,
            };
            assert!(
                argv::build(&spec).is_ok(),
                "{flag} must be accepted by the argv builder"
            );
        }
    }

    /// "Install it" and "upgrade it" must stay separable after the
    /// conversion into the crate-wide error type.
    #[test]
    fn error_conversion_keeps_install_and_upgrade_apart() {
        let absent = RarCliError::BinaryNotFound {
            searched: vec![PathBuf::from(r"C:\Program Files\WinRAR\rar.exe")],
        };
        match ArchiveError::from(absent) {
            ArchiveError::CodecUnavailable {
                codec,
                format,
                install_instructions,
            } => {
                assert_eq!(codec, RAR_CLI_CODEC);
                assert_eq!(format, ArchiveFormat::Rar);
                assert!(install_instructions.contains("win-rar.com"));
                assert!(install_instructions.contains("WinRAR"));
            }
            other => panic!("absence must map to CodecUnavailable, got {other:?}"),
        }

        let old = RarCliError::UnsupportedVersion {
            rar_exe: PathBuf::from(r"C:\Program Files\WinRAR\rar.exe"),
            found: RarVersion::new(4, 20),
            minimum: MINIMUM_RAR_VERSION,
        };
        match ArchiveError::from(old) {
            ArchiveError::Unsupported {
                operation,
                format,
                details,
            } => {
                assert_eq!(operation, RAR_CLI_OPERATION);
                assert_eq!(format, ArchiveFormat::Rar);
                let details = details.expect("details present");
                assert!(details.contains("4.20"), "{details}");
                assert!(details.contains("Upgrade WinRAR"), "{details}");
            }
            other => panic!("an old binary must map to Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn error_conversion_maps_the_remaining_classes() {
        assert!(matches!(
            ArchiveError::from(RarCliError::UnsupportedPlatform { os: "macos" }),
            ArchiveError::Unsupported { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::ProgramRejected {
                rar_exe: PathBuf::from("/opt/rar.bat"),
                reason: ProgramVetError::ShellInterpreted {
                    extension: "bat".to_string()
                },
            }),
            ArchiveError::Unsupported { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::SpawnFailed {
                rar_exe: PathBuf::from("/opt/rar"),
                source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            }),
            ArchiveError::Io { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::OutputExists {
                output: PathBuf::from("out.rar"),
            }),
            ArchiveError::Io { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::OutputMissing {
                output: PathBuf::from("out.rar"),
            }),
            ArchiveError::Format { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::CommandFailed {
                exit: RarExit::from_code(Some(5)),
                argv: vec!["a".to_string()],
                stderr: String::new(),
            }),
            ArchiveError::Format { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::CommandFailed {
                exit: RarExit::from_code(Some(11)),
                argv: vec!["a".to_string()],
                stderr: String::new(),
            }),
            ArchiveError::Password { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::InvalidArguments(ArgvError::EmptyPassword)),
            ArchiveError::Password { .. }
        ));
        assert!(matches!(
            ArchiveError::from(RarCliError::InvalidArguments(ArgvError::NoEntries)),
            ArchiveError::Format { .. }
        ));
    }

    /// A converted error must never carry the password, whichever arm of
    /// the conversion it took.
    #[test]
    fn converted_errors_never_carry_the_password() {
        let failed = RarCliError::CommandFailed {
            exit: RarExit::from_code(Some(11)),
            argv: argv::redact_argv(&[
                std::ffi::OsString::from("a"),
                std::ffi::OsString::from("-hphunter2"),
            ]),
            stderr: String::new(),
        };
        let text = ArchiveError::from(failed).to_string();
        assert!(!text.contains("hunter2"), "leaked password: {text}");
    }

    /// End-to-end through the public type with an injected runner: no
    /// `rar` binary, no Windows host, no proprietary anything.
    #[test]
    fn creator_drives_the_injected_runner() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let fake_rar = temp.path().join("rar_stub_binary");
        std::fs::write(&fake_rar, b"not really a binary")?;
        let input = temp.path().join("doc.txt");
        std::fs::write(&input, b"payload")?;
        let output = temp.path().join("out.rar");

        // Scripted: identify as RAR 6.24, then succeed.
        let stub = std::sync::Arc::new(RecordingRunner::default());
        stub.push(Some(0), RAR6);
        stub.push(Some(0), "");

        let mut creator = RarCreator::with_command_runner(
            &output,
            &fake_rar,
            Box::new(std::sync::Arc::clone(&stub)),
        )?;
        creator.set_compression_level(CompressionLevel::Ultra);
        creator.add_file(&input)?;
        assert_eq!(creator.entry_count(), 1);
        assert_eq!(creator.rar_exe_path(), fake_rar.as_path());

        let preview = creator.preview_argv()?;
        assert_eq!(preview[0], "a");
        assert_eq!(preview[1], "-m5");
        assert_eq!(preview[2], "-r");
        assert_eq!(preview[3], "--");

        // `rar` reports success but writes nothing, so the artifact
        // check must catch it.
        let error = creator.create().expect_err("no archive was written");
        assert!(
            matches!(error, RarCliError::OutputMissing { .. }),
            "{error}"
        );

        let calls = stub.calls();
        assert_eq!(calls.len(), 2, "one probe, one add");
        assert!(calls[0].is_empty(), "the probe takes no arguments");
        assert!(
            calls[1].iter().any(|a| a.as_str() == "--"),
            "{:?}",
            calls[1]
        );
        Ok(())
    }

    /// A batch shim must be refused at construction: `cmd.exe` would
    /// re-parse the argument vector.
    #[test]
    fn a_batch_shim_is_refused_at_construction() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let shim = temp.path().join("rar.bat");
        std::fs::write(&shim, b"@echo off\n")?;
        let error = RarCreator::with_rar_exe_path(temp.path().join("out.rar"), &shim)
            .expect_err("a batch shim must be refused");
        assert!(matches!(
            error,
            RarCliError::ProgramRejected {
                reason: ProgramVetError::ShellInterpreted { .. },
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn a_missing_binary_is_refused_at_construction() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let error = RarCreator::with_rar_exe_path(
            temp.path().join("out.rar"),
            temp.path().join("absent_rar"),
        )
        .expect_err("a missing binary must be refused");
        assert!(matches!(
            error,
            RarCliError::ProgramRejected {
                reason: ProgramVetError::NotAFile,
                ..
            }
        ));
        assert!(error.is_binary_unavailable());
        Ok(())
    }

    #[test]
    fn an_occupied_output_is_refused_at_construction() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let fake_rar = temp.path().join("rar_stub_binary");
        std::fs::write(&fake_rar, b"stub")?;
        let output = temp.path().join("out.rar");
        std::fs::write(&output, b"existing archive")?;
        let error = RarCreator::with_rar_exe_path(&output, &fake_rar)
            .expect_err("an occupied output must be refused");
        assert!(matches!(error, RarCliError::OutputExists { .. }));
        Ok(())
    }

    /// A recording [`CommandRunner`] built on the public trait — proof
    /// that the seam is usable from outside this module.
    #[derive(Default)]
    struct RecordingRunner {
        script: std::sync::Mutex<std::collections::VecDeque<(Option<i32>, String)>>,
        calls: std::sync::Mutex<Vec<Vec<String>>>,
    }

    impl RecordingRunner {
        fn push(&self, code: Option<i32>, stdout: &str) {
            self.script
                .lock()
                .expect("script lock")
                .push_back((code, stdout.to_string()));
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl CommandRunner for RecordingRunner {
        fn run(
            &self,
            _program: &Path,
            args: &[std::ffi::OsString],
        ) -> std::io::Result<CommandOutcome> {
            self.calls
                .lock()
                .expect("calls lock")
                .push(argv::redact_argv(args));
            let (code, stdout) = self
                .script
                .lock()
                .expect("script lock")
                .pop_front()
                .expect("scripted response");
            Ok(CommandOutcome::new(code, stdout.as_bytes(), ""))
        }
    }

    impl CommandRunner for std::sync::Arc<RecordingRunner> {
        fn run(
            &self,
            program: &Path,
            args: &[std::ffi::OsString],
        ) -> std::io::Result<CommandOutcome> {
            self.as_ref().run(program, args)
        }
    }
}
