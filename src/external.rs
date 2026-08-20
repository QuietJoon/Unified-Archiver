//! External tool integrations.
//!
//! Wrappers for external command-line tools that extend
//! `unified-archive` beyond what the linked libraries provide. Today
//! there is exactly one: [`RarCreator`], which creates RAR archives by
//! driving a licensed WinRAR console binary on Windows, behind the
//! optional `external-rar-create` Cargo feature.
//!
//! # Shell-out policy
//!
//! Everything in this module obeys the same rules, because a shell-out
//! is a correctness *and* a safety boundary:
//!
//! - **Arguments are passed as a vector to the OS process API.** No
//!   command string is ever built and no shell is ever involved, so
//!   there is no quoting to get wrong. Every path is emitted after an
//!   end-of-switches sentinel, and a value that cannot cross the process
//!   boundary intact is refused with a typed error instead of being
//!   truncated or mangled. See [`rar::argv`].
//! - **Exit codes are always interpreted.** A tool that ran is not a
//!   tool that succeeded: each documented failure code becomes its own
//!   typed variant, an unrecognised code is a failure rather than a
//!   success, and the expected artifact is checked for afterwards. See
//!   [`rar::exit`].
//! - **The tool is identified before it is driven.** The wrong program
//!   or an unsupported version is refused up front rather than handed a
//!   command it will mis-parse. See [`rar::version`].
//! - **Diagnostics never carry secrets.** Argument vectors are redacted
//!   before they reach an error message or a log line.
//!
//! # Fallback when an external tool is missing or unusable
//!
//! An optional external tool is, by definition, sometimes absent. This
//! module's contract is that such a case is a **typed refusal that names
//! what is missing** — never a raw OS error, never a partial artifact,
//! and never a silent substitution of a different format.
//!
//! For RAR specifically there is nothing to fall back *to*: RAR creation
//! is not offered through the main [`crate::Archive`] facade at all
//! (`encryption_write`, `multipart_write` and modification are all
//! `Support::None` for `Rar`/`Rar5`), so a caller that cannot use this
//! lane must choose a different format. The refusal therefore has to be
//! actionable, and it is:
//!
//! - [`RarCliError::is_binary_unavailable`] — nothing to drive here
//!   (absent, non-Windows host, unrunnable program). Install WinRAR, or
//!   create a ZIP/7z/TAR archive, which this crate does natively on
//!   every platform.
//! - [`RarCliError::is_binary_unusable`] — a binary exists but this
//!   crate refuses to drive it (too old, `unrar` rather than `rar`,
//!   unidentifiable). Upgrade or repoint it.
//! - [`RarCliError::remediation`] — the human-facing sentence for either
//!   case.
//!
//! Callers that want to know *before* committing to a RAR output path
//! should call [`RarCreator::check_availability`], which answers the
//! question without creating anything.
//!
//! Each [`RarCliError`] variant also converts into [`crate::ArchiveError`]
//! with `?`, and the conversion preserves the install-vs-upgrade split by
//! variant: absence becomes `ArchiveError::CodecUnavailable`, an
//! unusable-or-too-old binary becomes `ArchiveError::Unsupported`.

#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub mod rar;

#[cfg(all(target_os = "windows", feature = "external-rar-create"))]
pub use rar::{
    ArgvError, CommandOutcome, CommandRunner, MINIMUM_RAR_VERSION, ProgramVetError, RarCliError,
    RarCreator, RarExit, RarFlavor, RarVersion, SystemRunner,
};
