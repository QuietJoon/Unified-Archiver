//! Contract tests for the external RAR creation lane (OI-0076-006).
//!
//! # Why this file includes source modules by path
//!
//! `src/external.rs` is gated in `src/lib.rs` on
//! `all(target_os = "windows", feature = "external-rar-create")`, so on a
//! non-Windows development host the whole lane is compiled out and none
//! of its unit tests execute. The lane's decisions, however, are not
//! platform-specific: argument-vector construction, exit-code
//! classification, binary identification, discovery rules and the
//! invocation sequence are ordinary logic that a macOS or Linux host can
//! check perfectly well.
//!
//! Those five modules are therefore written free of any `crate::`
//! reference and are compiled directly into this test binary with
//! `#[path]`. Their own `#[cfg(test)]` unit tests come along with them
//! and run here, on every host, in addition to running inside the crate
//! on the Windows lane. The only crate-coupled files —
//! `src/external/rar.rs` (the `RarCreator` facade and the
//! `RarCliError -> ArchiveError` bridge) — keep their tests in-crate.
//!
//! # No proprietary binary is required
//!
//! Nothing here needs WinRAR. Two lanes cover the ground:
//!
//! - the scripted [`runner::CommandRunner`] double, which covers every
//!   decision on every platform; and
//! - on unix, a generated `/bin/sh` stub standing in for `rar`, which
//!   additionally proves the *real* process boundary behaves as
//!   documented — the argument vector arrives element for element, the
//!   exit code is captured, and stdin is at EOF so the child can never
//!   block on a prompt.
//!
//! A third lane, against a real licensed `rar`, exists as an
//! `#[ignore]`d test with its exact command line in the ignore reason.

#[allow(dead_code)]
#[path = "../src/external/rar/argv.rs"]
mod argv;

#[allow(dead_code)]
#[path = "../src/external/rar/discovery.rs"]
mod discovery;

#[allow(dead_code)]
#[path = "../src/external/rar/error.rs"]
mod error;

#[allow(dead_code)]
#[path = "../src/external/rar/exit.rs"]
mod exit;

#[allow(dead_code)]
#[path = "../src/external/rar/runner.rs"]
mod runner;

#[allow(dead_code)]
#[path = "../src/external/rar/session.rs"]
mod session;

#[allow(dead_code)]
#[path = "../src/external/rar/version.rs"]
mod version;

use std::path::{Path, PathBuf};

use error::RarCliError;
use exit::RarExit;
use runner::{CommandRunner, SystemRunner};
use version::{MINIMUM_RAR_VERSION, RarVersion};

const RAR_BANNER: &str = "RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal   1 Oct 2023";

/// Environment variable naming a real `rar` binary for the ignored lane.
const REAL_RAR_ENV: &str = "UA_RAR_EXE";

/// A `/bin/sh` stand-in for `rar`, generated per test.
///
/// Behaviour is baked into the script text rather than read from the
/// environment, so parallel tests cannot interfere with each other.
#[cfg(unix)]
struct StubBinary {
    path: PathBuf,
    argv_log: PathBuf,
    stdin_marker: PathBuf,
}

#[cfg(unix)]
impl StubBinary {
    /// Write an executable stub that prints the RAR banner when called
    /// with no arguments, and otherwise logs its argument vector,
    /// records whether stdin was readable, optionally creates `touch`,
    /// and exits with `exit_code`.
    fn new(dir: &Path, exit_code: i32, touch: Option<&Path>) -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("rar_stub");
        let argv_log = dir.join("argv.log");
        let stdin_marker = dir.join("stdin.marker");
        let touch_line = match touch {
            Some(target) => format!(": > '{}'\n", target.display()),
            None => String::new(),
        };
        let script = format!(
            "#!/bin/sh\n\
             if [ \"$#\" -eq 0 ]; then\n\
             \x20 echo '{banner}'\n\
             \x20 exit 7\n\
             fi\n\
             if IFS= read -r _line; then printf 'stdin-open' > '{marker}';\n\
             else printf 'stdin-eof' > '{marker}'; fi\n\
             : > '{log}'\n\
             for arg in \"$@\"; do printf '%s\\n' \"$arg\" >> '{log}'; done\n\
             {touch_line}exit {exit_code}\n",
            banner = RAR_BANNER,
            marker = stdin_marker.display(),
            log = argv_log.display(),
            touch_line = touch_line,
            exit_code = exit_code,
        );
        std::fs::write(&path, script)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        Ok(Self {
            path,
            argv_log,
            stdin_marker,
        })
    }

    /// The argument vector the stub actually received, one element per
    /// logged line.
    fn received_argv(&self) -> Vec<String> {
        std::fs::read_to_string(&self.argv_log)
            .expect("the stub logged its argv")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn stdin_state(&self) -> String {
        std::fs::read_to_string(&self.stdin_marker).expect("the stub recorded its stdin state")
    }
}

/// The scripted-runner lane: every decision, no process at all.
///
/// The bulk of this lane lives in the included modules' own `#[cfg(test)]
/// mod tests`, which run in this binary. The cases below are the
/// cross-module assertions that belong to no single module.
mod scripted {
    use super::*;

    fn rar_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\WinRAR\rar.exe")
        } else {
            PathBuf::from("/opt/WinRAR/rar")
        }
    }

    /// A stub that answers the identification probe, then the add.
    fn scripted_runner(add_exit: Option<i32>) -> runner::StubRunner {
        let stub = runner::StubRunner::new();
        stub.push_run(Some(0), RAR_BANNER, "");
        stub.push_run(add_exit, "", "stub diagnostic");
        stub
    }

    /// The full sequence: identify, run, classify, verify.
    #[test]
    fn a_clean_run_produces_the_documented_argument_vector() {
        let stub = scripted_runner(Some(0));
        let entries = vec![PathBuf::from("a file & more.txt")];
        let request = session::CreateRequest {
            compression_flag: "-m5",
            password: Some("hunter2"),
            recurse: true,
            output: Path::new("out dir/archive.rar"),
            entries: &entries,
        };
        // Absent for the pre-check, present for the post-check.
        let probed = std::cell::Cell::new(0u32);
        let exists = |_: &Path| {
            let nth = probed.get();
            probed.set(nth + 1);
            nth > 0
        };

        session::create_archive(&stub, &rar_path(), &request, exists).expect("archive created");

        let sent: Vec<String> = stub
            .args(1)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            sent,
            vec![
                "a",
                "-m5",
                "-hphunter2",
                "-r",
                "--",
                "out dir/archive.rar",
                "a file & more.txt",
            ],
            "the argument vector must arrive element for element"
        );
    }

    /// The single most important property: a non-zero exit must never be
    /// reported as a created archive, even when the artifact exists.
    #[test]
    fn a_failing_exit_code_is_never_success() {
        for code in [1i32, 2, 3, 5, 7, 10, 11, 42, 255] {
            let stub = scripted_runner(Some(code));
            let entries = vec![PathBuf::from("doc.txt")];
            let request = session::CreateRequest {
                compression_flag: "-m3",
                password: None,
                recurse: true,
                output: Path::new("out.rar"),
                entries: &entries,
            };
            // The artifact exists after the run, so only the exit code
            // can fail this: a lenient implementation returns Ok.
            let probed = std::cell::Cell::new(0u32);
            let result = session::create_archive(&stub, &rar_path(), &request, |_: &Path| {
                let nth = probed.get();
                probed.set(nth + 1);
                nth > 0
            });
            match result {
                Err(RarCliError::CommandFailed { exit, .. }) => {
                    assert_eq!(exit, RarExit::from_code(Some(code)));
                    assert!(!exit.is_success());
                }
                other => panic!("exit {code} must fail the run, got {other:?}"),
            }
        }
    }

    /// The two remediations the ticket separates: install vs upgrade.
    #[test]
    fn absence_and_wrong_version_are_separate_variants() {
        // Absence: discovery finds nothing (or refuses by platform off
        // Windows). Either way the caller learns the binary is not
        // usable here, by variant.
        let stub = runner::StubRunner::new();
        let absent = session::check_availability(
            &stub,
            None,
            &[rar_path().parent().expect("parent").to_path_buf()],
            |_: &Path| false,
        )
        .expect_err("nothing installed");
        assert!(absent.is_binary_unavailable(), "{absent:?}");
        assert!(!absent.is_binary_unusable(), "{absent:?}");
        assert!(
            matches!(
                absent,
                RarCliError::BinaryNotFound { .. } | RarCliError::UnsupportedPlatform { .. }
            ),
            "{absent:?}"
        );

        // Wrong version: a binary exists and identifies itself as 4.20.
        let stub = runner::StubRunner::new();
        stub.push_run(
            Some(0),
            "RAR 4.20   Copyright (c) 1993-2012 Alexander Roshal",
            "",
        );
        let old = session::probe_binary(&stub, &rar_path()).expect_err("too old");
        match old {
            RarCliError::UnsupportedVersion { found, minimum, .. } => {
                assert_eq!(found, RarVersion::new(4, 20));
                assert_eq!(minimum, MINIMUM_RAR_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
        assert!(!old.is_binary_unavailable());
        assert!(old.is_binary_unusable());
        assert_ne!(absent.remediation(), old.remediation());
    }
}

/// The stub-script lane: the same sequence across a real process
/// boundary, with a generated `/bin/sh` script standing in for `rar`.
#[cfg(unix)]
mod stub_script {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        // Honours TMPDIR, which the project's test command points at the
        // sanctioned scratch volume; no path is hardcoded here.
        tempfile::tempdir().expect("temp dir")
    }

    /// The argument vector survives a real `execve`: a path with spaces,
    /// ampersands, quotes and a leading dash arrives as the exact
    /// elements the contract promises.
    #[test]
    fn the_real_process_receives_the_exact_vector() {
        let dir = temp_dir();
        let output = dir.path().join("out.rar");
        let stub = StubBinary::new(dir.path(), 0, Some(&output)).expect("stub written");

        let entries = vec![
            PathBuf::from("a b & c | d > e \"quoted\".txt"),
            PathBuf::from("-sw.txt"),
        ];
        let request = session::CreateRequest {
            compression_flag: "-m0",
            password: Some("p a s s\"word"),
            recurse: true,
            output: &output,
            entries: &entries,
        };

        session::create_archive(&SystemRunner, &stub.path, &request, |p: &Path| p.exists())
            .expect("the stub reports success and creates the output");

        let received = stub.received_argv();
        assert_eq!(
            received,
            vec![
                "a".to_string(),
                "-m0".to_string(),
                "-hpp a s s\"word".to_string(),
                "-r".to_string(),
                "--".to_string(),
                output.display().to_string(),
                "a b & c | d > e \"quoted\".txt".to_string(),
                "./-sw.txt".to_string(),
            ],
            "shell metacharacters must not be re-interpreted, and a \
             leading-dash entry must arrive re-rooted"
        );
    }

    /// The non-interactive guarantee, observed by the child: stdin is at
    /// EOF, so a prompting `rar` aborts instead of hanging the caller.
    #[test]
    fn the_child_sees_a_closed_stdin() {
        let dir = temp_dir();
        let output = dir.path().join("out.rar");
        let stub = StubBinary::new(dir.path(), 0, Some(&output)).expect("stub written");
        let entries = vec![PathBuf::from("doc.txt")];
        let request = session::CreateRequest {
            compression_flag: "-m3",
            password: None,
            recurse: true,
            output: &output,
            entries: &entries,
        };
        session::create_archive(&SystemRunner, &stub.path, &request, |p: &Path| p.exists())
            .expect("created");
        assert_eq!(stub.stdin_state(), "stdin-eof");
    }

    /// A real non-zero exit is captured and classified.
    #[test]
    fn a_real_non_zero_exit_is_classified() {
        let dir = temp_dir();
        let output = dir.path().join("out.rar");
        // Exits 3 (CRC) *and* creates the output, so only the exit code
        // can fail the run.
        let stub = StubBinary::new(dir.path(), 3, Some(&output)).expect("stub written");
        let entries = vec![PathBuf::from("doc.txt")];
        let request = session::CreateRequest {
            compression_flag: "-m3",
            password: None,
            recurse: true,
            output: &output,
            entries: &entries,
        };
        match session::create_archive(&SystemRunner, &stub.path, &request, |p: &Path| p.exists()) {
            Err(RarCliError::CommandFailed { exit, .. }) => {
                assert_eq!(exit, RarExit::CrcError)
            }
            other => panic!("expected a classified failure, got {other:?}"),
        }
        assert!(output.exists(), "the stub did create a file");
    }

    /// A success code with no artifact is still a failure.
    #[test]
    fn a_real_success_without_an_artifact_fails() {
        let dir = temp_dir();
        let output = dir.path().join("out.rar");
        let stub = StubBinary::new(dir.path(), 0, None).expect("stub written");
        let entries = vec![PathBuf::from("doc.txt")];
        let request = session::CreateRequest {
            compression_flag: "-m3",
            password: None,
            recurse: true,
            output: &output,
            entries: &entries,
        };
        match session::create_archive(&SystemRunner, &stub.path, &request, |p: &Path| p.exists()) {
            Err(RarCliError::OutputMissing { .. }) => {}
            other => panic!("expected OutputMissing, got {other:?}"),
        }
    }

    /// A real spawn failure (a non-executable file) is a typed
    /// "unavailable", not a leaked OS string.
    #[test]
    fn a_real_spawn_failure_is_typed() {
        let dir = temp_dir();
        let not_executable = dir.path().join("rar_not_exec");
        std::fs::write(&not_executable, b"#!/nonexistent\n").expect("written");
        match session::probe_binary(&SystemRunner, &not_executable) {
            Err(error @ RarCliError::SpawnFailed { .. }) => {
                assert!(error.is_binary_unavailable());
                assert!(
                    error.to_string().contains("rar_not_exec"),
                    "the refusal must name the binary: {error}"
                );
            }
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }

    /// A stub that identifies itself as `unrar` is refused as the wrong
    /// program even though it runs perfectly well.
    #[test]
    fn a_real_unrar_is_refused_as_the_wrong_program() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir();
        let path = dir.path().join("unrar_stub");
        std::fs::write(
            &path,
            "#!/bin/sh\necho 'UNRAR 6.24 freeware  Copyright (c) 1993-2023 Alexander Roshal'\n",
        )
        .expect("written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        match session::probe_binary(&SystemRunner, &path) {
            Err(error @ RarCliError::WrongBinary { .. }) => {
                assert!(error.is_binary_unusable());
                assert!(
                    error.to_string().contains("cannot create archives"),
                    "{error}"
                );
            }
            other => panic!("expected WrongBinary, got {other:?}"),
        }
    }
}

/// The real-binary lane. Requires a licensed WinRAR, so it never runs by
/// default.
#[test]
#[ignore = "requires a licensed rar binary; run with: \
            UA_RAR_EXE=/absolute/path/to/rar TMPDIR=/Volumes/Temp/claude \
            cargo test --all-features --test external_rar_cli_contract -- \
            --ignored --test-threads=4"]
fn a_real_rar_binary_identifies_itself_and_creates_an_archive() {
    let rar_exe = PathBuf::from(
        std::env::var_os(REAL_RAR_ENV)
            .unwrap_or_else(|| panic!("set {REAL_RAR_ENV} to an absolute path to a rar binary")),
    );
    let version = session::probe_binary(&SystemRunner, &rar_exe).expect("a supported rar");
    assert!(version >= MINIMUM_RAR_VERSION, "{version}");

    let dir = tempfile::tempdir().expect("temp dir");
    let input = dir.path().join("doc.txt");
    std::fs::write(&input, b"payload").expect("input written");
    let output = dir.path().join("out.rar");
    let entries = vec![input];
    let request = session::CreateRequest {
        compression_flag: "-m3",
        password: None,
        recurse: true,
        output: &output,
        entries: &entries,
    };
    session::create_archive(&SystemRunner, &rar_exe, &request, |p: &Path| p.exists())
        .expect("archive created");
    assert!(output.exists());
}

/// The included modules are the real source files, not copies: this
/// asserts the values the crate itself depends on, so a drift between
/// this binary and the lib is impossible rather than merely unlikely.
#[test]
fn the_included_modules_are_the_shipped_ones() {
    assert_eq!(MINIMUM_RAR_VERSION, RarVersion::new(5, 0));
    assert_eq!(argv::END_OF_SWITCHES, "--");
    assert!(RarExit::from_code(Some(0)).is_success());
    assert!(!RarExit::from_code(Some(1)).is_success());
    assert_eq!(discovery::external_rar_supported(), cfg!(windows));
    // The trait object is usable from outside the crate's own modules.
    let stub = runner::StubRunner::new();
    stub.push_run(Some(0), RAR_BANNER, "");
    let runner: &dyn CommandRunner = &stub;
    assert_eq!(
        session::probe_binary(runner, Path::new("/opt/WinRAR/rar")).expect("identified"),
        RarVersion::new(6, 24)
    );
}
