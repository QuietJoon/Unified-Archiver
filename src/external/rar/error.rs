//! Typed failures for the external RAR creation lane.
//!
//! # Why a dedicated error type
//!
//! The three failures a caller of an optional external tool actually
//! reacts to are *install it*, *upgrade it*, and *you pointed me at the
//! wrong program* — and each demands a different remediation. Collapsing
//! them into one generic "RAR creation failed" string forces the caller
//! to grep message text, which breaks the moment the wording changes.
//! [`RarCliError`] therefore gives every one of them its own variant, as
//! it does for each documented `rar` exit class.
//!
//! # The fallback contract
//!
//! When the external binary is absent, unusable, or too old, this lane
//! **refuses in a typed way and does nothing else**. It does not fall
//! back to another format, does not write a partial archive, and does
//! not surface a raw OS error such as `NotFound: program not found`. The
//! refusal names what is missing:
//!
//! | Situation | Variant | Caller's move |
//! |---|---|---|
//! | No `rar` binary anywhere | [`RarCliError::BinaryNotFound`] (carries every path probed) | install WinRAR, or pass an explicit path |
//! | Host is not Windows | [`RarCliError::UnsupportedPlatform`] | no external RAR lane exists here |
//! | Older than [`super::version::MINIMUM_RAR_VERSION`] | [`RarCliError::UnsupportedVersion`] | upgrade WinRAR |
//! | Binary is `unrar`, which cannot create | [`RarCliError::WrongBinary`] | point at `rar`, not `unrar` |
//! | Batch shim / relative path / not a file | [`RarCliError::ProgramRejected`] | pass a real executable's absolute path |
//! | Version banner unparseable | [`RarCliError::VersionProbeFailed`] | unknown build; treated as unusable |
//!
//! There is **no silent degradation**, because there is nothing to
//! degrade to: RAR creation is not offered by the main archive facade at
//! all (`encryption_write`, `multipart_write` and modification are all
//! `Support::None` for `Rar`/`Rar5`), so a caller that cannot use this
//! lane has no in-crate alternative and must be told precisely why.
//!
//! The two predicates [`RarCliError::is_binary_unavailable`] and
//! [`RarCliError::is_binary_unusable`] exist so the install-vs-upgrade
//! split can be taken without matching every variant, and
//! [`RarCliError::remediation`] carries the human-facing hint.
//!
//! Conversion into the crate-wide error type lives in the parent module
//! (`super`), which is the only crate-coupled file in this lane; this
//! module is deliberately free of any `crate::` reference so it can be
//! compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

use std::path::PathBuf;

use super::argv::ArgvError;
use super::discovery::ProgramVetError;
use super::exit::RarExit;
use super::version::{RarFlavor, RarVersion};

/// A failure of the external RAR creation lane.
#[derive(Debug)]
#[non_exhaustive]
pub enum RarCliError {
    /// No usable `rar` binary was found.
    ///
    /// `searched` lists every candidate that was probed, in order, so
    /// the message can tell the user which `PATH` entry or install
    /// directory to fix. Distinct from [`Self::UnsupportedVersion`]:
    /// this one means *install it*.
    BinaryNotFound {
        /// Candidates that were probed and rejected.
        searched: Vec<PathBuf>,
    },

    /// This target has no external RAR lane. Not a missing install.
    UnsupportedPlatform {
        /// `std::env::consts::OS` of the running build.
        os: &'static str,
    },

    /// A caller-supplied program path cannot be spawned as `rar`.
    ProgramRejected {
        /// The rejected path.
        rar_exe: PathBuf,
        /// Why it was rejected.
        reason: ProgramVetError,
    },

    /// The binary ran but did not print a banner this crate can parse,
    /// so its identity and version are unknown.
    ///
    /// Treated as unusable: driving an unidentified archiver with a
    /// version-sensitive argument vector is exactly the guess this lane
    /// refuses to make.
    VersionProbeFailed {
        /// The probed binary.
        rar_exe: PathBuf,
        /// Lossily decoded, truncated probe output.
        output: String,
    },

    /// The binary is a genuine `rar`, but older than
    /// [`super::version::MINIMUM_RAR_VERSION`]. Means *upgrade it*.
    UnsupportedVersion {
        /// The probed binary.
        rar_exe: PathBuf,
        /// Version it reported.
        found: RarVersion,
        /// Oldest version this crate will drive.
        minimum: RarVersion,
    },

    /// The binary is a RAR-family tool that cannot create archives — in
    /// practice `unrar`. Means *you pointed me at the wrong program*.
    WrongBinary {
        /// The probed binary.
        rar_exe: PathBuf,
        /// What it identified itself as.
        flavor: RarFlavor,
        /// Version it reported.
        version: RarVersion,
    },

    /// The process could not be started at all.
    SpawnFailed {
        /// The binary that failed to start.
        rar_exe: PathBuf,
        /// Underlying OS error.
        source: std::io::Error,
    },

    /// The process ran and reported a failure.
    ///
    /// `exit` is the classified code — never a bare integer a caller has
    /// to look up — and `argv` is the redacted argument vector, so a
    /// diagnosis never has to reconstruct the invocation and never
    /// contains the password.
    CommandFailed {
        /// Classified exit status. Never [`RarExit::Success`].
        exit: RarExit,
        /// Redacted argument vector, per [`super::argv::redact_argv`].
        argv: Vec<String>,
        /// Truncated `rar` stderr.
        stderr: String,
    },

    /// The argument vector could not be built from the given inputs.
    InvalidArguments(ArgvError),

    /// The requested output path is already occupied.
    OutputExists {
        /// The occupied path.
        output: PathBuf,
    },

    /// `rar` reported success but the archive is not on disk.
    ///
    /// The post-condition check exists because a success code is not
    /// proof of an artifact: a filter switch, an antivirus quarantine or
    /// a redirected working directory can all leave nothing behind.
    OutputMissing {
        /// Where the archive was expected.
        output: PathBuf,
    },
}

impl RarCliError {
    /// `true` when the external binary is missing or cannot be run as a
    /// *program* — install / point-elsewhere territory.
    ///
    /// Covers [`Self::BinaryNotFound`], [`Self::UnsupportedPlatform`],
    /// [`Self::ProgramRejected`] and [`Self::SpawnFailed`]. Explicitly
    /// **not** [`Self::UnsupportedVersion`], which calls for an upgrade.
    pub fn is_binary_unavailable(&self) -> bool {
        matches!(
            self,
            Self::BinaryNotFound { .. }
                | Self::UnsupportedPlatform { .. }
                | Self::ProgramRejected { .. }
                | Self::SpawnFailed { .. }
        )
    }

    /// `true` when a binary exists but this crate refuses to drive it —
    /// upgrade / replace territory.
    pub fn is_binary_unusable(&self) -> bool {
        matches!(
            self,
            Self::UnsupportedVersion { .. }
                | Self::WrongBinary { .. }
                | Self::VersionProbeFailed { .. }
        )
    }

    /// `true` when the failure was a wrong or unusable password, so a
    /// caller can re-prompt rather than abort.
    pub fn is_password_failure(&self) -> bool {
        match self {
            Self::CommandFailed { exit, .. } => exit.is_password_failure(),
            Self::InvalidArguments(
                ArgvError::EmptyPassword | ArgvError::PasswordControlCharacter,
            ) => true,
            _ => false,
        }
    }

    /// Remediation hint for the human at the other end.
    pub fn remediation(&self) -> String {
        match self {
            Self::BinaryNotFound { .. } => {
                "Install WinRAR from https://www.win-rar.com/ and make sure the directory \
                 containing rar.exe is on PATH, or construct the creator with an explicit \
                 path via RarCreator::with_rar_exe_path."
                    .to_string()
            }
            Self::UnsupportedPlatform { os } => format!(
                "External RAR creation is a Windows-only capability; this build targets {os}. \
                 Use ZIP or 7z, which this crate creates natively on every platform."
            ),
            Self::UnsupportedVersion { minimum, .. } => {
                format!("Upgrade WinRAR to {minimum} or newer (https://www.win-rar.com/).")
            }
            Self::WrongBinary { .. } => {
                "Point the creator at 'rar', the full archiver, not 'unrar' - the freeware \
                 unrar binary can only extract."
                    .to_string()
            }
            Self::ProgramRejected { .. } => {
                "Pass the absolute path of the rar executable itself, not a wrapper script."
                    .to_string()
            }
            Self::VersionProbeFailed { .. } => {
                "The binary did not identify itself as a supported rar build; verify the \
                 installation."
                    .to_string()
            }
            _ => "See the reported error for the failing step.".to_string(),
        }
    }
}

impl core::fmt::Display for RarCliError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BinaryNotFound { searched } => {
                write!(f, "no usable rar binary found")?;
                if searched.is_empty() {
                    return Ok(());
                }
                f.write_str("; probed ")?;
                for (index, path) in searched.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "'{}'", path.display())?;
                }
                Ok(())
            }
            Self::UnsupportedPlatform { os } => write!(
                f,
                "external RAR creation is not available on this platform ({os}); \
                 it requires Windows"
            ),
            Self::ProgramRejected { rar_exe, reason } => {
                write!(f, "rar path '{}' {reason}", rar_exe.display())
            }
            Self::VersionProbeFailed { rar_exe, output } => write!(
                f,
                "could not identify the rar binary at '{}' from its output: {output:?}",
                rar_exe.display()
            ),
            Self::UnsupportedVersion {
                rar_exe,
                found,
                minimum,
            } => write!(
                f,
                "rar binary at '{}' reports version {found}, but {minimum} or newer is required",
                rar_exe.display()
            ),
            Self::WrongBinary {
                rar_exe,
                flavor,
                version,
            } => write!(
                f,
                "the binary at '{}' is {flavor} {version}, which cannot create archives",
                rar_exe.display()
            ),
            Self::SpawnFailed { rar_exe, source } => {
                write!(f, "failed to run '{}': {source}", rar_exe.display())
            }
            Self::CommandFailed { exit, argv, stderr } => {
                write!(f, "rar reported failure: {exit}; argv {argv:?}")?;
                if !stderr.is_empty() {
                    write!(f, "; stderr: {stderr}")?;
                }
                Ok(())
            }
            Self::InvalidArguments(error) => write!(f, "invalid rar invocation: {error}"),
            Self::OutputExists { output } => {
                write!(f, "output file '{}' already exists", output.display())
            }
            Self::OutputMissing { output } => write!(
                f,
                "rar reported success but no archive exists at '{}'",
                output.display()
            ),
        }
    }
}

impl std::error::Error for RarCliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SpawnFailed { source, .. } => Some(source),
            Self::InvalidArguments(error) => Some(error),
            Self::ProgramRejected { reason, .. } => Some(reason),
            _ => None,
        }
    }
}

impl From<ArgvError> for RarCliError {
    fn from(error: ArgvError) -> Self {
        Self::InvalidArguments(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn not_found() -> RarCliError {
        RarCliError::BinaryNotFound {
            searched: vec![
                PathBuf::from("/opt/tools/rar"),
                PathBuf::from("/opt/WinRAR/rar"),
            ],
        }
    }

    fn too_old() -> RarCliError {
        RarCliError::UnsupportedVersion {
            rar_exe: PathBuf::from("/opt/WinRAR/rar"),
            found: RarVersion::new(4, 20),
            minimum: super::super::version::MINIMUM_RAR_VERSION,
        }
    }

    /// The ticket's third requirement: "binary not found" and
    /// "unsupported version" must be separable by variant, never by
    /// string, because the remediations differ.
    #[test]
    fn absence_and_version_are_distinct_variants() {
        assert!(not_found().is_binary_unavailable());
        assert!(!not_found().is_binary_unusable());
        assert!(too_old().is_binary_unusable());
        assert!(!too_old().is_binary_unavailable());

        assert!(matches!(not_found(), RarCliError::BinaryNotFound { .. }));
        assert!(matches!(too_old(), RarCliError::UnsupportedVersion { .. }));

        assert!(
            not_found().remediation().contains("Install WinRAR"),
            "absence must advise installing: {}",
            not_found().remediation()
        );
        assert!(
            too_old().remediation().contains("Upgrade WinRAR"),
            "an old build must advise upgrading: {}",
            too_old().remediation()
        );
        assert_ne!(not_found().remediation(), too_old().remediation());
    }

    #[test]
    fn binary_not_found_names_every_probed_path() {
        let text = not_found().to_string();
        assert!(text.contains("/opt/tools/rar"), "{text}");
        assert!(text.contains("/opt/WinRAR/rar"), "{text}");

        let nothing = RarCliError::BinaryNotFound { searched: vec![] };
        assert_eq!(nothing.to_string(), "no usable rar binary found");
    }

    #[test]
    fn unsupported_version_names_found_and_minimum() {
        let text = too_old().to_string();
        assert!(text.contains("4.20"), "{text}");
        assert!(text.contains("5.0"), "{text}");
    }

    #[test]
    fn wrong_program_is_its_own_variant() {
        let wrong = RarCliError::WrongBinary {
            rar_exe: PathBuf::from("/usr/bin/unrar"),
            flavor: RarFlavor::UnRar,
            version: RarVersion::new(6, 24),
        };
        assert!(wrong.is_binary_unusable());
        assert!(!wrong.is_binary_unavailable());
        assert!(
            wrong.to_string().contains("cannot create archives"),
            "{wrong}"
        );
        assert!(
            wrong.remediation().contains("not 'unrar'"),
            "{}",
            wrong.remediation()
        );
    }

    #[test]
    fn platform_refusal_names_the_platform_and_an_alternative() {
        let refusal = RarCliError::UnsupportedPlatform { os: "macos" };
        assert!(refusal.is_binary_unavailable());
        assert!(refusal.to_string().contains("macos"), "{refusal}");
        let hint = refusal.remediation();
        assert!(hint.contains("Windows-only"), "{hint}");
        assert!(hint.contains("ZIP or 7z"), "{hint}");
    }

    #[test]
    fn program_rejection_carries_the_vet_reason_as_a_source() {
        let rejected = RarCliError::ProgramRejected {
            rar_exe: PathBuf::from("/opt/tools/rar.bat"),
            reason: ProgramVetError::ShellInterpreted {
                extension: "bat".to_string(),
            },
        };
        assert!(rejected.is_binary_unavailable());
        assert!(rejected.to_string().contains("cmd.exe"), "{rejected}");
        assert!(std::error::Error::source(&rejected).is_some());
    }

    #[test]
    fn version_probe_failure_is_unusable_not_missing() {
        let failed = RarCliError::VersionProbeFailed {
            rar_exe: PathBuf::from("/opt/WinRAR/rar"),
            output: "7-Zip 23.01".to_string(),
        };
        assert!(failed.is_binary_unusable());
        assert!(!failed.is_binary_unavailable());
    }

    /// The classified exit code must reach the message, and the password
    /// must not.
    #[test]
    fn command_failure_reports_the_exit_class_without_the_password() {
        let failed = RarCliError::CommandFailed {
            exit: RarExit::from_code(Some(5)),
            argv: super::super::argv::redact_argv(&[
                std::ffi::OsString::from("a"),
                std::ffi::OsString::from("-hphunter2"),
            ]),
            stderr: "Cannot create out.rar".to_string(),
        };
        let text = failed.to_string();
        assert!(text.contains("rar exit 5"), "{text}");
        assert!(text.contains("write error"), "{text}");
        assert!(text.contains("Cannot create out.rar"), "{text}");
        assert!(!text.contains("hunter2"), "leaked password: {text}");
    }

    #[test]
    fn password_failures_are_flagged() {
        let wrong_pw = RarCliError::CommandFailed {
            exit: RarExit::from_code(Some(11)),
            argv: vec!["a".to_string()],
            stderr: String::new(),
        };
        assert!(wrong_pw.is_password_failure());
        assert!(RarCliError::InvalidArguments(ArgvError::EmptyPassword).is_password_failure());
        assert!(
            RarCliError::InvalidArguments(ArgvError::PasswordControlCharacter)
                .is_password_failure()
        );

        let write_error = RarCliError::CommandFailed {
            exit: RarExit::from_code(Some(5)),
            argv: vec!["a".to_string()],
            stderr: String::new(),
        };
        assert!(!write_error.is_password_failure());
        assert!(!not_found().is_password_failure());
    }

    #[test]
    fn spawn_failure_is_unavailable_and_keeps_the_os_error() {
        let failed = RarCliError::SpawnFailed {
            rar_exe: PathBuf::from("/opt/WinRAR/rar"),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        };
        assert!(failed.is_binary_unavailable());
        assert!(std::error::Error::source(&failed).is_some());
        assert!(failed.to_string().contains("/opt/WinRAR/rar"), "{failed}");
    }

    #[test]
    fn output_state_errors_name_the_path() {
        let exists = RarCliError::OutputExists {
            output: PathBuf::from("/data/out.rar"),
        };
        assert!(exists.to_string().contains("already exists"), "{exists}");

        let missing = RarCliError::OutputMissing {
            output: PathBuf::from("/data/out.rar"),
        };
        assert!(
            missing
                .to_string()
                .contains("reported success but no archive"),
            "{missing}"
        );
    }

    #[test]
    fn argv_errors_convert_in() {
        let error: RarCliError = ArgvError::NoEntries.into();
        assert!(matches!(
            error,
            RarCliError::InvalidArguments(ArgvError::NoEntries)
        ));
        assert!(
            error.to_string().contains("invalid rar invocation"),
            "{error}"
        );
    }
}
