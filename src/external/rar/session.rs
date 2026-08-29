//! The external `rar` invocation sequence: discover, identify, run,
//! classify, verify.
//!
//! # Why the sequence lives here
//!
//! Each individual decision has its own module — [`super::discovery`]
//! finds the binary, [`super::version`] identifies it, [`super::argv`]
//! builds the vector, [`super::exit`] reads the code — and this module
//! is the *order* in which they happen. Keeping the order in one place,
//! parameterised over the [`CommandRunner`] seam and over the filesystem
//! predicates, means the whole lane can be driven end to end by tests
//! with no `rar` binary present, on any host.
//!
//! # The sequence, and why each step exists
//!
//! 1. **Build the argument vector first.** Bad inputs (no entries, an
//!    empty password, a NUL in a path) are refused before any process is
//!    started, so an invalid request never costs a spawn and never
//!    reaches a binary that might partially act on it.
//! 2. **Identify the binary before using it.** The probe converts
//!    "something is installed" into "a `rar` at least
//!    [`super::version::MINIMUM_RAR_VERSION`]", which is what the
//!    argument vector's `--` guard depends on. An `unrar`, a 4.x `rar`,
//!    or an unidentifiable program is refused here rather than being
//!    handed a vector it will mis-parse.
//! 3. **Run it with a null stdin** (see [`CommandRunner`]).
//! 4. **Classify the exit code.** Only `0` continues; every other code,
//!    including unknown ones and "no code at all", becomes a typed
//!    failure. This is the step whose absence lets a failed archive look
//!    like a success.
//! 5. **Verify the artifact exists.** A success code is not proof that a
//!    file was written.
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::argv::{self, AddArgv};
use super::discovery::{self, DiscoveryOutcome};
use super::error::RarCliError;
use super::exit::RarExit;
use super::runner::{self, CommandRunner};
use super::version::{MINIMUM_RAR_VERSION, RarVersion, parse_banner};

/// Maximum number of child-output bytes copied into an error value.
pub const OUTPUT_EXCERPT_LIMIT: usize = 4096;

/// Everything needed to invoke `rar a`, in already-validated form.
///
/// Mirrors [`AddArgv`] rather than re-deriving it, so the sequence and
/// the vector cannot drift apart.
#[derive(Debug, Clone, Copy)]
/// **Not `#[non_exhaustive]`, deliberately (ticgit a5b31f).** Same reason as
/// [`super::argv::AddArgv`], to which this is field-for-field identical:
/// callers construct it directly to reach [`preview_argv`], and there is no
/// builder to construct it through. Adding a field is a breaking change
/// until that is resolved, and collapsing the two types is the candidate
/// repair.
pub struct CreateRequest<'a> {
    /// `-mN` switch from the compression-level mapping.
    pub compression_flag: &'a str,
    /// Password payload for `-hp`, if any.
    pub password: Option<&'a str>,
    /// Whether to pass `-r`.
    pub recurse: bool,
    /// Archive to write.
    pub output: &'a Path,
    /// Files and directories to add.
    pub entries: &'a [PathBuf],
}

impl<'a> CreateRequest<'a> {
    fn as_argv(&self) -> AddArgv<'a> {
        AddArgv {
            compression_flag: self.compression_flag,
            password: self.password,
            recurse: self.recurse,
            output: self.output,
            entries: self.entries,
        }
    }
}

/// Locate a `rar` binary, then identify it.
///
/// This is the "can I use this lane at all?" question, answered with a
/// version on success and a typed refusal naming the missing piece on
/// failure. `is_file` and the environment values are injected so the
/// whole check is testable.
pub fn check_availability<F>(
    runner: &dyn CommandRunner,
    path_var: Option<&OsStr>,
    install_dirs: &[PathBuf],
    is_file: F,
) -> Result<(PathBuf, RarVersion), RarCliError>
where
    F: Fn(&Path) -> bool,
{
    let rar_exe = match discovery::find_rar_binary(path_var, install_dirs, is_file) {
        DiscoveryOutcome::Found(path) => path,
        DiscoveryOutcome::NotFound { searched } => {
            return Err(RarCliError::BinaryNotFound { searched });
        }
        DiscoveryOutcome::UnsupportedPlatform { os } => {
            return Err(RarCliError::UnsupportedPlatform { os });
        }
    };
    let version = probe_binary(runner, &rar_exe)?;
    Ok((rar_exe, version))
}

/// Accept a caller-supplied program path, applying the same vetting the
/// discovery sweep applies.
///
/// Bypasses discovery but not the safety checks: a relative path or a
/// batch shim is refused with [`RarCliError::ProgramRejected`], because
/// the [`super::argv`] quoting contract cannot hold for either.
pub fn vet_explicit_program<F>(rar_exe: &Path, is_file: F) -> Result<PathBuf, RarCliError>
where
    F: Fn(&Path) -> bool,
{
    match discovery::vet_program(rar_exe, is_file) {
        Ok(()) => Ok(rar_exe.to_path_buf()),
        Err(reason) => Err(RarCliError::ProgramRejected {
            rar_exe: rar_exe.to_path_buf(),
            reason,
        }),
    }
}

/// Identify a binary and check it against the minimum supported version.
///
/// Runs the binary with an **empty** argument vector, which makes every
/// RAR-family build print its identifying banner; see
/// [`super::version`] for why the exit code of this probe is ignored and
/// `-iver` is not used.
pub fn probe_binary(runner: &dyn CommandRunner, rar_exe: &Path) -> Result<RarVersion, RarCliError> {
    let outcome = runner
        .run(rar_exe, &[])
        .map_err(|source| RarCliError::SpawnFailed {
            rar_exe: rar_exe.to_path_buf(),
            source,
        })?;
    let text = outcome.combined_text();
    let banner = parse_banner(&text).ok_or_else(|| RarCliError::VersionProbeFailed {
        rar_exe: rar_exe.to_path_buf(),
        output: runner::excerpt(&text, OUTPUT_EXCERPT_LIMIT),
    })?;
    if !banner.flavor.can_create() {
        return Err(RarCliError::WrongBinary {
            rar_exe: rar_exe.to_path_buf(),
            flavor: banner.flavor,
            version: banner.version,
        });
    }
    if banner.version < MINIMUM_RAR_VERSION {
        return Err(RarCliError::UnsupportedVersion {
            rar_exe: rar_exe.to_path_buf(),
            found: banner.version,
            minimum: MINIMUM_RAR_VERSION,
        });
    }
    Ok(banner.version)
}

/// Run one `rar a` invocation through the full sequence.
///
/// `output_exists` is injected (production passes `|p| p.exists()`) so
/// the pre-condition and the post-condition can both be exercised
/// without touching a filesystem.
pub fn create_archive<F>(
    runner: &dyn CommandRunner,
    rar_exe: &Path,
    request: &CreateRequest<'_>,
    output_exists: F,
) -> Result<(), RarCliError>
where
    F: Fn(&Path) -> bool,
{
    // Step 1: refuse bad input before spending a spawn on it.
    let args = argv::build(&request.as_argv())?;

    // Step 2: refuse an occupied destination before spending a spawn on
    // it. Cheap, and it keeps the common "the file is already there"
    // case free of any child process at all.
    if output_exists(request.output) {
        return Err(RarCliError::OutputExists {
            output: request.output.to_path_buf(),
        });
    }

    // Step 3: identify the binary before handing it a vector.
    probe_binary(runner, rar_exe)?;

    // Step 4: check *again*, now that nothing else will happen before
    // the run.
    //
    // ticgit 642488: step 2's check used to be the only one, and its
    // comment claimed it ran "immediately before the run" — but
    // `probe_binary` spawns `rar` and waits for its banner, so a whole
    // child process lived in the gap. That is the widest possible
    // version of the window the check exists to narrow. Moving the probe
    // earlier would have closed the gap too, but at the cost of spawning
    // on every occupied destination, which step 2 deliberately avoids;
    // a second `stat` buys the same narrowing for no process at all.
    //
    // It narrows the window; it does not close it. Closing it means
    // creating under an exclusive temporary name and installing the
    // result with `rename_noclobber` — which already exists in
    // `ffi::common`, with unix, Windows and fallback arms — and is
    // tracked as the remaining half of OI-0076-006 / ticgit 642488.
    if output_exists(request.output) {
        return Err(RarCliError::OutputExists {
            output: request.output.to_path_buf(),
        });
    }

    // Step 5: run it.
    let outcome = runner
        .run(rar_exe, &args)
        .map_err(|source| RarCliError::SpawnFailed {
            rar_exe: rar_exe.to_path_buf(),
            source,
        })?;

    // Step 6: an unchecked exit code is how a failed archive passes for
    // a good one.
    let exit = RarExit::from_code(outcome.code);
    if !exit.is_success() {
        return Err(RarCliError::CommandFailed {
            exit,
            argv: argv::redact_argv(&args),
            stderr: outcome.stderr_excerpt(OUTPUT_EXCERPT_LIMIT),
        });
    }

    // Step 7: a success code is not an artifact.
    if !output_exists(request.output) {
        return Err(RarCliError::OutputMissing {
            output: request.output.to_path_buf(),
        });
    }

    Ok(())
}

/// The argument vector a given request would produce, redacted.
///
/// Exposed so a caller can log or display the exact invocation without
/// running it — and without ever seeing the password.
pub fn preview_argv(request: &CreateRequest<'_>) -> Result<Vec<String>, RarCliError> {
    let args: Vec<OsString> = argv::build(&request.as_argv())?;
    Ok(argv::redact_argv(&args))
}

#[cfg(test)]
mod tests {
    use super::super::runner::StubRunner;
    use super::super::version::RarFlavor;
    use super::*;

    const RAR6: &str = "RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal   1 Oct 2023";
    const RAR4: &str = "RAR 4.20   Copyright (c) 1993-2012 Alexander Roshal   9 Jun 2012";
    const UNRAR6: &str = "UNRAR 6.24 freeware      Copyright (c) 1993-2023 Alexander Roshal";

    fn rar_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\WinRAR\rar.exe")
        } else {
            PathBuf::from("/opt/WinRAR/rar")
        }
    }

    fn entries() -> Vec<PathBuf> {
        vec![PathBuf::from("doc.txt")]
    }

    fn request<'a>(output: &'a Path, entries: &'a [PathBuf]) -> CreateRequest<'a> {
        CreateRequest {
            compression_flag: "-m3",
            password: None,
            recurse: true,
            output,
            entries,
        }
    }

    /// The archive is created only when the binary identifies itself,
    /// the exit code is 0, and the artifact exists. The argument vector
    /// handed to the OS is asserted element by element: this is the
    /// quoting contract observed at the process boundary.
    #[test]
    fn happy_path_probes_then_runs_with_a_vector() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), RAR6, "");
        stub.push_run(Some(0), "", "");

        let entries = entries();
        let created = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &entries),
            // Absent before the run, present after. Keyed off the
            // runner's call count rather than a call counter of its own,
            // so adding or removing an existence check does not silently
            // change what this closure means: two runs (probe, add) is
            // exactly "the archive has been written".
            |_: &Path| stub.calls().len() >= 2,
        );
        assert!(created.is_ok(), "{created:?}");

        let calls = stub.calls();
        assert_eq!(calls.len(), 2, "one probe, one add");
        assert!(calls[0].args.is_empty(), "the probe takes no arguments");
        assert_eq!(calls[0].program, rar_path());
        assert_eq!(
            stub.args(1)
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["a", "-m3", "-r", "--", "out.rar", "doc.txt"]
        );
    }

    /// Every documented non-zero code must abort with the classified
    /// status attached — the ticket's exit-code requirement, end to end.
    #[test]
    fn every_failing_exit_code_aborts_with_its_class() {
        let codes = [
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
            (42, RarExit::Unrecognised(42)),
        ];
        for (code, expected) in codes {
            let stub = StubRunner::new();
            stub.push_run(Some(0), RAR6, "");
            stub.push_run(Some(code), "", "diagnostic text");

            let entries = entries();
            // The artifact is present, so only the exit code can fail
            // the run: a lenient implementation would return Ok here.
            let result = create_archive(
                &stub,
                &rar_path(),
                &request(Path::new("out.rar"), &entries),
                |_: &Path| false,
            );
            match result {
                Err(RarCliError::CommandFailed { exit, stderr, .. }) => {
                    assert_eq!(exit, expected, "exit {code} classified wrongly");
                    assert_eq!(stderr, "diagnostic text");
                }
                other => panic!("exit code {code} must fail the run, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_terminated_process_is_a_failure() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), RAR6, "");
        stub.push_run(None, "", "");
        let entries = entries();
        let result = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &entries),
            |_: &Path| false,
        );
        assert!(matches!(
            result,
            Err(RarCliError::CommandFailed {
                exit: RarExit::Terminated,
                ..
            })
        ));
    }

    /// A zero exit with no artifact must not be reported as success.
    #[test]
    fn success_without_an_artifact_is_a_failure() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), RAR6, "");
        stub.push_run(Some(0), "Done", "");
        let entries = entries();
        let result = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &entries),
            |_: &Path| false,
        );
        match result {
            Err(RarCliError::OutputMissing { output }) => {
                assert_eq!(output, Path::new("out.rar"))
            }
            other => panic!("expected OutputMissing, got {other:?}"),
        }
    }

    /// An occupied destination is refused before the binary is probed,
    /// so nothing is overwritten and no process is started.
    #[test]
    fn an_occupied_output_is_refused_without_running_anything() {
        let stub = StubRunner::new();
        let entries = entries();
        let result = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &entries),
            |_: &Path| true,
        );
        assert!(matches!(result, Err(RarCliError::OutputExists { .. })));
        assert!(
            stub.calls().is_empty(),
            "nothing may be spawned for an occupied destination"
        );
    }

    /// Bad input must be refused before any process is spawned.
    #[test]
    fn invalid_input_never_spawns() {
        let empty: Vec<PathBuf> = Vec::new();
        let stub = StubRunner::new();
        let result = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &empty),
            |_: &Path| false,
        );
        assert!(matches!(
            result,
            Err(RarCliError::InvalidArguments(
                super::super::argv::ArgvError::NoEntries
            ))
        ));
        assert!(stub.calls().is_empty(), "no process may be started");

        let entries = entries();
        let stub = StubRunner::new();
        let mut bad = request(Path::new("out.rar"), &entries);
        bad.password = Some("");
        let result = create_archive(&stub, &rar_path(), &bad, |_: &Path| false);
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(RarCliError::is_password_failure)
        );
        assert!(stub.calls().is_empty(), "no process may be started");
    }

    /// An old `rar` is refused with the upgrade variant, and the add is
    /// never attempted.
    #[test]
    fn an_old_binary_is_refused_before_the_add() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), RAR4, "");
        let entries = entries();
        let result = create_archive(
            &stub,
            &rar_path(),
            &request(Path::new("out.rar"), &entries),
            |_: &Path| false,
        );
        match result {
            Err(RarCliError::UnsupportedVersion { found, minimum, .. }) => {
                assert_eq!(found, RarVersion::new(4, 20));
                assert_eq!(minimum, MINIMUM_RAR_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
        assert_eq!(stub.calls().len(), 1, "the add must not be attempted");
    }

    #[test]
    fn unrar_is_refused_as_the_wrong_program() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), UNRAR6, "");
        match probe_binary(&stub, &rar_path()) {
            Err(RarCliError::WrongBinary {
                flavor, version, ..
            }) => {
                assert_eq!(flavor, RarFlavor::UnRar);
                assert_eq!(version, RarVersion::new(6, 24));
            }
            other => panic!("expected WrongBinary, got {other:?}"),
        }
    }

    /// The probe's own exit code is irrelevant: `rar` with no command
    /// exits non-zero on some builds and zero on others.
    #[test]
    fn the_probe_ignores_its_own_exit_code() {
        for code in [Some(0), Some(7), Some(255), None] {
            let stub = StubRunner::new();
            stub.push_run(code, RAR6, "");
            assert_eq!(
                probe_binary(&stub, &rar_path()).expect("banner decides, not the code"),
                RarVersion::new(6, 24),
                "probe exit code {code:?} must not change the verdict"
            );
        }
    }

    #[test]
    fn a_banner_on_stderr_is_still_read() {
        let stub = StubRunner::new();
        stub.push_run(Some(7), "", RAR6);
        assert_eq!(
            probe_binary(&stub, &rar_path()).expect("stderr banner"),
            RarVersion::new(6, 24)
        );
    }

    #[test]
    fn an_unidentifiable_binary_is_refused_with_its_output() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), "7-Zip 23.01 : Copyright (c) Igor Pavlov", "");
        match probe_binary(&stub, &rar_path()) {
            Err(RarCliError::VersionProbeFailed { output, .. }) => {
                assert!(output.contains("7-Zip"), "{output}")
            }
            other => panic!("expected VersionProbeFailed, got {other:?}"),
        }
    }

    /// Probe output is bounded so an error value cannot grow without
    /// limit.
    #[test]
    fn probe_output_is_truncated() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), &"x".repeat(OUTPUT_EXCERPT_LIMIT * 2), "");
        match probe_binary(&stub, &rar_path()) {
            Err(RarCliError::VersionProbeFailed { output, .. }) => {
                assert!(
                    output.len() <= OUTPUT_EXCERPT_LIMIT + 32,
                    "{}",
                    output.len()
                );
                assert!(output.ends_with("… (truncated)"));
            }
            other => panic!("expected VersionProbeFailed, got {other:?}"),
        }
    }

    #[test]
    fn a_spawn_failure_names_the_binary() {
        let stub = StubRunner::new();
        stub.push_spawn_failure(std::io::ErrorKind::NotFound);
        match probe_binary(&stub, &rar_path()) {
            Err(error @ RarCliError::SpawnFailed { .. }) => {
                assert!(error.is_binary_unavailable());
                assert!(error.to_string().contains("rar"), "{error}");
            }
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn availability_reports_the_missing_binary_with_every_probed_path() {
        let stub = StubRunner::new();
        let install = vec![rar_path().parent().expect("has a parent").to_path_buf()];
        let result = check_availability(&stub, None, &install, |_: &Path| false);
        match result {
            Err(RarCliError::BinaryNotFound { searched }) => {
                assert_eq!(searched, vec![rar_path()])
            }
            #[cfg(not(windows))]
            Err(error @ RarCliError::UnsupportedPlatform { .. }) => {
                // Non-Windows hosts refuse by platform before searching, so
                // this arm exists only off Windows. Expressing that with
                // `#[cfg]` beats the `assert!(!cfg!(windows))` that stood
                // here: that assertion was `assert!(true)` off Windows and
                // unreachable on it, so it could never fail.
                assert!(error.is_binary_unavailable());
            }
            other => panic!("expected a typed refusal, got {other:?}"),
        }
        assert!(stub.calls().is_empty(), "a missing binary is never spawned");
    }

    #[test]
    fn availability_returns_the_version_when_everything_is_in_place() {
        if !cfg!(windows) {
            // Discovery refuses by platform off Windows; the probe half
            // of this path is covered by `the_probe_ignores_...`.
            return;
        }
        let stub = StubRunner::new();
        stub.push_run(Some(0), RAR6, "");
        let install = vec![rar_path().parent().expect("has a parent").to_path_buf()];
        let expected = rar_path();
        let (found, version) =
            check_availability(&stub, None, &install, |p: &Path| p == expected).expect("available");
        assert_eq!(found, rar_path());
        assert_eq!(version, RarVersion::new(6, 24));
    }

    #[test]
    fn explicit_programs_are_vetted_like_discovered_ones() {
        let expected = rar_path();
        assert_eq!(
            vet_explicit_program(&expected, |p: &Path| p == expected).expect("accepted"),
            expected
        );

        let relative = Path::new("rar.exe");
        match vet_explicit_program(relative, |_: &Path| true) {
            Err(RarCliError::ProgramRejected { reason, .. }) => assert_eq!(
                reason,
                super::super::discovery::ProgramVetError::NotAbsolute
            ),
            other => panic!("a relative program path must be refused, got {other:?}"),
        }

        let batch = rar_path().with_extension("bat");
        match vet_explicit_program(&batch, |_: &Path| true) {
            Err(RarCliError::ProgramRejected { reason, .. }) => assert!(matches!(
                reason,
                super::super::discovery::ProgramVetError::ShellInterpreted { .. }
            )),
            other => panic!("a batch shim must be refused, got {other:?}"),
        }
    }

    /// The preview is the only sanctioned rendering of the invocation,
    /// and it never carries the password.
    #[test]
    fn preview_redacts_the_password() {
        let entries = entries();
        let mut spec = request(Path::new("out.rar"), &entries);
        spec.password = Some("hunter2");
        let preview = preview_argv(&spec).expect("argv builds");
        assert_eq!(
            preview,
            vec![
                "a",
                "-m3",
                super::super::argv::REDACTED_PASSWORD_ARG,
                "-r",
                "--",
                "out.rar",
                "doc.txt"
            ]
        );
        assert!(!preview.join(" ").contains("hunter2"));
    }
}
