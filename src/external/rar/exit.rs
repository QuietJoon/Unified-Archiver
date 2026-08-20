//! Exit-code classification for the external `rar` console binary.
//!
//! # Why this is its own module
//!
//! An unchecked exit code on a shell-out is a silent-corruption bug: a
//! `rar.exe` run that produced no archive, a truncated archive, or an
//! archive missing entries can still leave a file on disk, so "the
//! process ran" is not "the archive is correct". WinRAR documents a
//! distinct code per failure class, and this module is the single place
//! that turns one into a typed value.
//!
//! # Policy
//!
//! - **`0` is the only success.** Every other code — including the
//!   documented `1` (non-fatal warning) — is a failure.  `rar` returns
//!   `1` when it finished but could not process some of the requested
//!   files (locked, sharing-violation, vanished mid-run). The resulting
//!   archive exists but is *incomplete*, and silently handing a caller
//!   an archive with missing entries is precisely the failure mode this
//!   module exists to prevent. A caller that wants the lenient reading
//!   can match [`RarExit::Warning`] explicitly.
//! - **An unrecognised code is a failure**, never a success. It is
//!   preserved as [`RarExit::Unrecognised`] with the raw value so a
//!   future WinRAR release that adds a code is reported accurately
//!   instead of being coerced into the nearest known variant.
//! - **No exit code at all** (the process was killed by a signal, which
//!   is reachable on the non-Windows lanes and via job-object kills on
//!   Windows) is [`RarExit::Terminated`], also a failure.
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

/// A classified exit status from the external `rar` binary.
///
/// Values follow the exit-code table published in WinRAR's `rar.txt`
/// ("Exit values"). [`RarExit::from_code`] is total: every possible
/// `Option<i32>` maps to a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RarExit {
    /// `0` — successful operation. The only success value.
    Success,
    /// `1` — non-fatal warning; some files were not processed.
    ///
    /// Treated as a failure by [`RarExit::is_success`]: the archive on
    /// disk is incomplete relative to the requested entry list.
    Warning,
    /// `2` — a fatal error occurred.
    FatalError,
    /// `3` — invalid checksum; data is damaged.
    CrcError,
    /// `4` — attempt to modify a locked archive.
    LockedArchive,
    /// `5` — write error (out of space, read-only destination, …).
    WriteError,
    /// `6` — file open error.
    OpenError,
    /// `7` — wrong command-line option.
    ///
    /// On the creation path this almost always means the argument
    /// vector this crate built was rejected by the installed `rar`
    /// build — i.e. a bug here, not caller error.
    UserError,
    /// `8` — not enough memory.
    MemoryError,
    /// `9` — file create error.
    CreateError,
    /// `10` — no files matching the specified mask and options.
    NoFilesMatched,
    /// `11` — wrong password.
    WrongPassword,
    /// `255` — user break (Ctrl-C / console interrupt).
    UserBreak,
    /// A code the published table does not define. Always a failure.
    Unrecognised(i32),
    /// The process reported no exit code (killed by a signal).
    Terminated,
}

impl RarExit {
    /// Classify a raw exit code as returned by
    /// [`std::process::ExitStatus::code`].
    ///
    /// Total by construction: `None` becomes [`RarExit::Terminated`] and
    /// an unknown `Some(n)` becomes [`RarExit::Unrecognised`], so no
    /// input can be mistaken for success.
    pub const fn from_code(code: Option<i32>) -> Self {
        match code {
            None => Self::Terminated,
            Some(0) => Self::Success,
            Some(1) => Self::Warning,
            Some(2) => Self::FatalError,
            Some(3) => Self::CrcError,
            Some(4) => Self::LockedArchive,
            Some(5) => Self::WriteError,
            Some(6) => Self::OpenError,
            Some(7) => Self::UserError,
            Some(8) => Self::MemoryError,
            Some(9) => Self::CreateError,
            Some(10) => Self::NoFilesMatched,
            Some(11) => Self::WrongPassword,
            Some(255) => Self::UserBreak,
            Some(other) => Self::Unrecognised(other),
        }
    }

    /// The raw code this status came from, if the process reported one.
    pub const fn code(self) -> Option<i32> {
        match self {
            Self::Success => Some(0),
            Self::Warning => Some(1),
            Self::FatalError => Some(2),
            Self::CrcError => Some(3),
            Self::LockedArchive => Some(4),
            Self::WriteError => Some(5),
            Self::OpenError => Some(6),
            Self::UserError => Some(7),
            Self::MemoryError => Some(8),
            Self::CreateError => Some(9),
            Self::NoFilesMatched => Some(10),
            Self::WrongPassword => Some(11),
            Self::UserBreak => Some(255),
            Self::Unrecognised(code) => Some(code),
            Self::Terminated => None,
        }
    }

    /// `true` only for [`RarExit::Success`].
    ///
    /// Deliberately *not* `code() == Some(0) || code() == Some(1)`:
    /// see the module-level policy note on `1`.
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }

    /// `true` when the failure is a wrong/missing password rather than a
    /// data or environment problem, so a caller can re-prompt instead of
    /// aborting.
    pub const fn is_password_failure(self) -> bool {
        matches!(self, Self::WrongPassword)
    }

    /// Stable human-readable label. Used by error `Display` impls; keeps
    /// message text out of the error module.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Warning => "non-fatal warning: some files were not processed",
            Self::FatalError => "fatal error",
            Self::CrcError => "invalid checksum; data is damaged",
            Self::LockedArchive => "attempt to modify a locked archive",
            Self::WriteError => "write error",
            Self::OpenError => "file open error",
            Self::UserError => "wrong command-line option",
            Self::MemoryError => "not enough memory",
            Self::CreateError => "file create error",
            Self::NoFilesMatched => "no files matched the requested mask and options",
            Self::WrongPassword => "wrong password",
            Self::UserBreak => "user break",
            Self::Unrecognised(_) => "unrecognised exit code",
            Self::Terminated => "process terminated without an exit code",
        }
    }
}

impl core::fmt::Display for RarExit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.code() {
            Some(code) => write!(f, "rar exit {code} ({})", self.label()),
            None => write!(f, "rar exit <none> ({})", self.label()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The documented table, verbatim. A regression here means a
    /// failure class silently changed meaning.
    #[test]
    fn documented_codes_map_to_their_variants() {
        let table = [
            (0, RarExit::Success),
            (1, RarExit::Warning),
            (2, RarExit::FatalError),
            (3, RarExit::CrcError),
            (4, RarExit::LockedArchive),
            (5, RarExit::WriteError),
            (6, RarExit::OpenError),
            (7, RarExit::UserError),
            (8, RarExit::MemoryError),
            (9, RarExit::CreateError),
            (10, RarExit::NoFilesMatched),
            (11, RarExit::WrongPassword),
            (255, RarExit::UserBreak),
        ];
        for (code, expected) in table {
            assert_eq!(
                RarExit::from_code(Some(code)),
                expected,
                "exit code {code} must classify as {expected:?}"
            );
            assert_eq!(expected.code(), Some(code), "code() must round-trip");
        }
    }

    #[test]
    fn only_zero_is_success() {
        assert!(RarExit::from_code(Some(0)).is_success());
        for code in [
            1i32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 100, 254, 255, -1,
        ] {
            assert!(
                !RarExit::from_code(Some(code)).is_success(),
                "exit code {code} must not be reported as success"
            );
        }
    }

    /// The stated policy: a warning is a failure because the archive is
    /// incomplete. Locked in a test so it cannot be "fixed" by accident.
    #[test]
    fn warning_is_a_failure_not_a_success() {
        let warn = RarExit::from_code(Some(1));
        assert_eq!(warn, RarExit::Warning);
        assert!(!warn.is_success());
    }

    #[test]
    fn unrecognised_codes_are_preserved_and_fail() {
        for code in [12i32, 13, 42, 128, 254, -5, i32::MAX, i32::MIN] {
            let exit = RarExit::from_code(Some(code));
            assert_eq!(exit, RarExit::Unrecognised(code));
            assert_eq!(exit.code(), Some(code));
            assert!(!exit.is_success());
        }
    }

    #[test]
    fn missing_code_is_terminated_and_fails() {
        let exit = RarExit::from_code(None);
        assert_eq!(exit, RarExit::Terminated);
        assert_eq!(exit.code(), None);
        assert!(!exit.is_success());
    }

    #[test]
    fn password_failure_is_isolated_to_code_eleven() {
        assert!(RarExit::from_code(Some(11)).is_password_failure());
        for code in [0i32, 1, 2, 3, 10, 12, 255] {
            assert!(!RarExit::from_code(Some(code)).is_password_failure());
        }
    }

    #[test]
    fn display_names_the_code() {
        assert_eq!(
            RarExit::from_code(Some(3)).to_string(),
            "rar exit 3 (invalid checksum; data is damaged)"
        );
        assert_eq!(
            RarExit::from_code(Some(42)).to_string(),
            "rar exit 42 (unrecognised exit code)"
        );
        assert_eq!(
            RarExit::from_code(None).to_string(),
            "rar exit <none> (process terminated without an exit code)"
        );
    }
}
