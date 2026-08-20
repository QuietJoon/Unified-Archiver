//! Locating and vetting the external `rar` binary.
//!
//! # Why discovery is a safety problem, not just a lookup
//!
//! The previous implementation ran `Command::new("where").arg("rar.exe")`.
//! Two problems with that:
//!
//! 1. `where` was resolved through `PATH` — and on Windows
//!    `CreateProcess` searches the **current directory first**, so a
//!    `where.exe` dropped next to the caller's working directory would
//!    be executed instead of the system one. Discovery is now performed
//!    in-process by walking `PATH` directly: no helper process, no
//!    hijack surface, and it is unit-testable without a Windows host.
//! 2. Whatever `where` printed was used as the program path after only
//!    an `exists()` check. A relative entry (`PATH` legitimately
//!    contains `.` on some machines) reintroduces the current-directory
//!    hijack, and a `rar.bat` shim would be run through `cmd.exe`, whose
//!    argument re-parsing the [`super::argv`] quoting contract cannot
//!    cover. Both are now refused by [`vet_program`].
//!
//! # Platform
//!
//! External RAR creation is a Windows-only capability. On any other host
//! discovery returns [`DiscoveryOutcome::UnsupportedPlatform`] instead of
//! searching, so the failure a caller sees names the platform rather than
//! being an indistinguishable "not found".
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Stock WinRAR install directory on a 64-bit Windows install, used when
/// `%ProgramFiles%` is not readable.
pub const FALLBACK_PROGRAM_FILES: &str = r"C:\Program Files";
/// Stock 32-bit WinRAR install directory, used when
/// `%ProgramFiles(x86)%` is not readable.
pub const FALLBACK_PROGRAM_FILES_X86: &str = r"C:\Program Files (x86)";
/// Directory WinRAR creates under a Program Files root.
pub const WINRAR_DIR_NAME: &str = "WinRAR";

/// Extensions that are re-parsed by a shell rather than by the MSVCRT
/// argument splitter.
///
/// A program with one of these extensions is refused: `cmd.exe` applies
/// its own de-quoting pass to the command line, so the argument vector
/// this crate builds would not arrive intact and a crafted path could
/// escape its argument. Matched case-insensitively.
pub const SHELL_INTERPRETED_EXTENSIONS: [&str; 2] = ["bat", "cmd"];

/// Whether the external RAR creation lane exists on this build's target.
///
/// A runtime function rather than a `cfg!` at each call site so the
/// non-Windows branch is type-checked and unit-tested everywhere.
pub fn external_rar_supported() -> bool {
    cfg!(windows)
}

/// File name of the `rar` console binary on this target.
pub fn rar_binary_name() -> &'static str {
    if cfg!(windows) { "rar.exe" } else { "rar" }
}

/// Why a candidate program path was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramVetError {
    /// The path does not name an existing regular file.
    NotAFile,
    /// The path is relative. Spawning a relative program path resolves
    /// it against the current directory, which the caller does not
    /// control.
    NotAbsolute,
    /// The path names a batch file, which `cmd.exe` would re-parse.
    ShellInterpreted { extension: String },
}

impl core::fmt::Display for ProgramVetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAFile => f.write_str("does not exist or is not a regular file"),
            Self::NotAbsolute => {
                f.write_str("is a relative path; only absolute program paths are accepted")
            }
            Self::ShellInterpreted { extension } => write!(
                f,
                "is a .{extension} shell script; the argument vector cannot be delivered \
                 intact through cmd.exe, so only a real executable is accepted"
            ),
        }
    }
}

impl std::error::Error for ProgramVetError {}

/// Check that a candidate path may be spawned as the rar binary.
///
/// `is_file` is injected so this is pure: production passes
/// `|p| p.is_file()`, tests pass a fixture set.
pub fn vet_program<F>(path: &Path, is_file: F) -> Result<(), ProgramVetError>
where
    F: Fn(&Path) -> bool,
{
    if !path.is_absolute() {
        return Err(ProgramVetError::NotAbsolute);
    }
    if let Some(extension) = path.extension().and_then(OsStr::to_str) {
        let lowered = extension.to_ascii_lowercase();
        if SHELL_INTERPRETED_EXTENSIONS.contains(&lowered.as_str()) {
            return Err(ProgramVetError::ShellInterpreted { extension: lowered });
        }
    }
    if !is_file(path) {
        return Err(ProgramVetError::NotAFile);
    }
    Ok(())
}

/// Result of a discovery sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoveryOutcome {
    /// A vetted binary. Guaranteed to have passed [`vet_program`].
    Found(PathBuf),
    /// Nothing usable was found. `searched` lists every candidate that
    /// was probed, in order, so the error can tell the user where to
    /// install the tool or which `PATH` entry to fix.
    NotFound { searched: Vec<PathBuf> },
    /// This target has no external RAR lane at all.
    UnsupportedPlatform { os: &'static str },
}

/// The stock WinRAR install directories, preferring the Program Files
/// roots the OS reports over the hardcoded `C:` literals so a Windows
/// install on another drive is still found.
pub fn default_install_dirs(
    program_files: Option<&OsStr>,
    program_files_x86: Option<&OsStr>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(2);
    for (from_env, fallback) in [
        (program_files, FALLBACK_PROGRAM_FILES),
        (program_files_x86, FALLBACK_PROGRAM_FILES_X86),
    ] {
        let root = match from_env {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => PathBuf::from(fallback),
        };
        let dir = root.join(WINRAR_DIR_NAME);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Search for the rar binary: every `PATH` directory in order, then the
/// supplied install directories.
///
/// Relative and empty `PATH` entries are skipped rather than probed —
/// see the module note on the current-directory hijack. Every candidate
/// that was actually probed is recorded in
/// [`DiscoveryOutcome::NotFound::searched`].
pub fn find_rar_binary<F>(
    path_var: Option<&OsStr>,
    install_dirs: &[PathBuf],
    is_file: F,
) -> DiscoveryOutcome
where
    F: Fn(&Path) -> bool,
{
    if !external_rar_supported() {
        return DiscoveryOutcome::UnsupportedPlatform {
            os: std::env::consts::OS,
        };
    }
    match search_dirs(path_var, install_dirs, is_file) {
        Ok(found) => DiscoveryOutcome::Found(found),
        Err(searched) => DiscoveryOutcome::NotFound { searched },
    }
}

/// The search itself, without the platform gate.
///
/// Split out from [`find_rar_binary`] so the ordering, skipping and
/// de-duplication rules are exercised by the unit tests on every host,
/// not only on the Windows lane. Returns the vetted path, or the list of
/// candidates that were probed.
pub fn search_dirs<F>(
    path_var: Option<&OsStr>,
    install_dirs: &[PathBuf],
    is_file: F,
) -> Result<PathBuf, Vec<PathBuf>>
where
    F: Fn(&Path) -> bool,
{
    let name = rar_binary_name();
    let mut searched: Vec<PathBuf> = Vec::new();

    let path_dirs = path_var
        .map(|value| std::env::split_paths(value).collect::<Vec<_>>())
        .unwrap_or_default();

    for dir in path_dirs.iter().chain(install_dirs.iter()) {
        // An empty or relative PATH entry would resolve against the
        // current directory; never probe one.
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(name);
        if searched.contains(&candidate) {
            continue;
        }
        let vetted = vet_program(&candidate, &is_file).is_ok();
        searched.push(candidate.clone());
        if vetted {
            return Ok(candidate);
        }
    }

    Err(searched)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake filesystem: only the listed paths are regular files.
    fn only<'a>(files: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |p: &Path| files.iter().any(|f| Path::new(f) == p)
    }

    /// Absolute-path fixtures have to look absolute to the *host*
    /// `Path` implementation, so the tests use POSIX roots on unix and
    /// drive-letter roots on Windows.
    #[cfg(windows)]
    mod fixture {
        pub const WINRAR_DIR: &str = r"C:\Program Files\WinRAR";
        pub const RAR: &str = r"C:\Program Files\WinRAR\rar.exe";
        pub const TOOLS_DIR: &str = r"C:\tools";
        pub const TOOLS_RAR: &str = r"C:\tools\rar.exe";
        pub const BAT: &str = r"C:\tools\rar.bat";
        pub const PATH_SEP: &str = ";";
    }
    #[cfg(not(windows))]
    mod fixture {
        pub const WINRAR_DIR: &str = "/opt/WinRAR";
        pub const RAR: &str = "/opt/WinRAR/rar";
        pub const TOOLS_DIR: &str = "/opt/tools";
        pub const TOOLS_RAR: &str = "/opt/tools/rar";
        pub const BAT: &str = "/opt/tools/rar.bat";
        pub const PATH_SEP: &str = ":";
    }

    #[test]
    fn support_flag_tracks_the_target() {
        assert_eq!(external_rar_supported(), cfg!(windows));
        assert_eq!(
            rar_binary_name(),
            if cfg!(windows) { "rar.exe" } else { "rar" }
        );
    }

    /// On a non-Windows host discovery must refuse by platform, not
    /// report a misleading "not found".
    #[test]
    #[cfg(not(windows))]
    fn non_windows_hosts_report_unsupported_platform() {
        let outcome = find_rar_binary(None, &[], only(&[fixture::RAR]));
        match outcome {
            DiscoveryOutcome::UnsupportedPlatform { os } => {
                assert_eq!(os, std::env::consts::OS)
            }
            other => panic!("expected UnsupportedPlatform, got {other:?}"),
        }
    }

    #[test]
    fn path_entries_are_searched_in_order() {
        let path = format!(
            "{}{}{}",
            fixture::TOOLS_DIR,
            fixture::PATH_SEP,
            fixture::WINRAR_DIR
        );
        let found = search_dirs(
            Some(OsStr::new(&path)),
            &[],
            only(&[fixture::TOOLS_RAR, fixture::RAR]),
        );
        assert_eq!(found, Ok(PathBuf::from(fixture::TOOLS_RAR)));
    }

    #[test]
    fn install_dirs_are_the_fallback_and_every_probe_is_recorded() {
        let install = vec![PathBuf::from(fixture::WINRAR_DIR)];
        let found = search_dirs(
            Some(OsStr::new(fixture::TOOLS_DIR)),
            &install,
            only(&[fixture::RAR]),
        );
        assert_eq!(found, Ok(PathBuf::from(fixture::RAR)));

        let searched = search_dirs(Some(OsStr::new(fixture::TOOLS_DIR)), &install, only(&[]))
            .expect_err("nothing to find");
        assert_eq!(
            searched,
            vec![
                PathBuf::from(fixture::TOOLS_RAR),
                PathBuf::from(fixture::RAR)
            ],
            "every probed candidate must be reported so the error can name it"
        );
    }

    /// A relative `PATH` entry (`.`, `..`, `bin`) must never be probed:
    /// spawning from it is a current-directory hijack.
    #[test]
    fn relative_path_entries_are_never_probed() {
        let sep = fixture::PATH_SEP;
        let path = format!(".{sep}..{sep}bin{sep}{}", fixture::WINRAR_DIR);
        let found = search_dirs(
            Some(OsStr::new(&path)),
            &[],
            // Pretend a rar exists in every relative location too.
            |_: &Path| true,
        );
        assert_eq!(
            found,
            Ok(PathBuf::from(fixture::RAR)),
            "a relative PATH entry must be skipped, not preferred"
        );
    }

    /// An empty `PATH` element (`a;;b`, or a trailing separator) is the
    /// current directory on Windows; it must be skipped too.
    #[test]
    fn empty_path_elements_are_skipped() {
        let sep = fixture::PATH_SEP;
        let path = format!("{sep}{sep}{}{sep}", fixture::WINRAR_DIR);
        let found = search_dirs(Some(OsStr::new(&path)), &[], |_: &Path| true);
        assert_eq!(found, Ok(PathBuf::from(fixture::RAR)));
    }

    /// The same directory listed twice must be probed once.
    #[test]
    fn duplicate_candidates_are_probed_once() {
        let sep = fixture::PATH_SEP;
        let path = format!("{d}{sep}{d}", d = fixture::TOOLS_DIR);
        let searched = search_dirs(
            Some(OsStr::new(&path)),
            &[PathBuf::from(fixture::TOOLS_DIR)],
            only(&[]),
        )
        .expect_err("nothing to find");
        assert_eq!(searched, vec![PathBuf::from(fixture::TOOLS_RAR)]);
    }

    #[test]
    fn no_path_variable_still_searches_the_install_dirs() {
        let found = search_dirs(
            None,
            &[PathBuf::from(fixture::WINRAR_DIR)],
            only(&[fixture::RAR]),
        );
        assert_eq!(found, Ok(PathBuf::from(fixture::RAR)));
    }

    #[test]
    fn vet_rejects_relative_programs() {
        assert_eq!(
            vet_program(Path::new("rar.exe"), |_| true),
            Err(ProgramVetError::NotAbsolute)
        );
        assert_eq!(
            vet_program(Path::new("./rar.exe"), |_| true),
            Err(ProgramVetError::NotAbsolute)
        );
    }

    #[test]
    fn vet_rejects_batch_shims() {
        for name in [
            fixture::BAT.to_string(),
            fixture::BAT.replace(".bat", ".CMD"),
        ] {
            match vet_program(Path::new(&name), |_| true) {
                Err(ProgramVetError::ShellInterpreted { extension }) => {
                    assert!(matches!(extension.as_str(), "bat" | "cmd"), "{extension}")
                }
                other => panic!("{name} must be refused, got {other:?}"),
            }
        }
    }

    #[test]
    fn vet_rejects_missing_files_and_accepts_real_ones() {
        assert_eq!(
            vet_program(Path::new(fixture::RAR), only(&[])),
            Err(ProgramVetError::NotAFile)
        );
        assert_eq!(
            vet_program(Path::new(fixture::RAR), only(&[fixture::RAR])),
            Ok(())
        );
    }

    #[test]
    fn install_dirs_prefer_the_environment_over_the_c_drive_literals() {
        let dirs = default_install_dirs(Some(OsStr::new(r"D:\Apps")), None);
        assert_eq!(dirs[0], PathBuf::from(r"D:\Apps").join(WINRAR_DIR_NAME));
        assert_eq!(
            dirs[1],
            PathBuf::from(FALLBACK_PROGRAM_FILES_X86).join(WINRAR_DIR_NAME)
        );

        let defaults = default_install_dirs(None, None);
        assert_eq!(
            defaults,
            vec![
                PathBuf::from(FALLBACK_PROGRAM_FILES).join(WINRAR_DIR_NAME),
                PathBuf::from(FALLBACK_PROGRAM_FILES_X86).join(WINRAR_DIR_NAME),
            ]
        );
    }

    /// An empty env var must fall back rather than yield `\WinRAR`.
    #[test]
    fn empty_environment_values_fall_back() {
        let dirs = default_install_dirs(Some(OsStr::new("")), Some(OsStr::new("")));
        assert_eq!(
            dirs,
            vec![
                PathBuf::from(FALLBACK_PROGRAM_FILES).join(WINRAR_DIR_NAME),
                PathBuf::from(FALLBACK_PROGRAM_FILES_X86).join(WINRAR_DIR_NAME),
            ]
        );
    }

    /// Identical Program Files roots must not be probed twice.
    #[test]
    fn duplicate_install_roots_collapse() {
        let dirs = default_install_dirs(Some(OsStr::new(r"D:\Apps")), Some(OsStr::new(r"D:\Apps")));
        assert_eq!(dirs.len(), 1);
    }
}
