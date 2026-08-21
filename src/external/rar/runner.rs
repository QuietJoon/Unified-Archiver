//! The process-execution seam for the external `rar` shell-out.
//!
//! # Why a trait
//!
//! Every decision this crate makes about `rar` — the argument vector,
//! the exit-code classification, the version gate, the "binary missing"
//! refusal — is a decision about a *process invocation*. Wiring those
//! decisions directly to [`std::process::Command`] would make them
//! testable only on a Windows host with a licensed WinRAR installed,
//! which is exactly the coverage that never happens. [`CommandRunner`]
//! is the one place the real process is spawned, so the tests substitute
//! a scripted double and cover the whole contract with no proprietary
//! binary anywhere.
//!
//! # The non-interactive guarantee
//!
//! [`SystemRunner`] always attaches a **null stdin**. `rar` prompts on
//! the console for several conditions (a missing `-hp` payload, an
//! overwrite decision, a volume change); with an inherited stdin those
//! prompts turn a library call into an indefinite hang inside a build or
//! a service. With stdin at EOF the child aborts and returns a code this
//! crate can classify. Argument-level guards in [`super::argv`] prevent
//! the prompts we know about; the null stdin bounds the ones we do not.
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

use std::ffi::OsString;
use std::path::Path;

/// What a finished child process reported.
///
/// `#[non_exhaustive]`: a caller implementing [`CommandRunner`] builds these
/// through [`Self::new`], which takes every field, so nothing is lost by
/// closing the struct literal — and a future field (a timing, a signal
/// number) then costs nobody a compile error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandOutcome {
    /// Raw exit code, or `None` when the process was killed by a signal.
    /// Classified by [`super::exit::RarExit::from_code`].
    pub code: Option<i32>,
    /// Captured standard output.
    pub stdout: Vec<u8>,
    /// Captured standard error.
    pub stderr: Vec<u8>,
}

impl CommandOutcome {
    /// Build an outcome from an exit code and text streams. Convenience
    /// for tests and for callers implementing their own runner.
    pub fn new(code: Option<i32>, stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    /// `stdout` followed by `stderr`, lossily decoded.
    ///
    /// The version banner is written to stdout by some `rar` builds and
    /// to stderr by others, so identification scans both. Lossy decoding
    /// is deliberate: on Windows the streams arrive in the console
    /// code page, and every token the parsers care about is ASCII.
    pub fn combined_text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.stdout).into_owned();
        if !self.stderr.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&String::from_utf8_lossy(&self.stderr));
        }
        text
    }

    /// `stderr`, lossily decoded and truncated to `limit` bytes, for
    /// embedding in an error message.
    ///
    /// Truncation matters because `rar` can emit one diagnostic line per
    /// failed entry; an unbounded copy would put megabytes of text into
    /// an error value.
    pub fn stderr_excerpt(&self, limit: usize) -> String {
        excerpt(&String::from_utf8_lossy(&self.stderr), limit)
    }
}

/// Trim `text` and clamp it to `limit` bytes, cutting on a character
/// boundary and marking the cut.
///
/// Used for every piece of child output that ends up inside an error
/// value: `rar` emits one diagnostic line per failed entry, and an
/// unbounded copy would put megabytes of text into an error.
pub fn excerpt(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= limit {
        return trimmed.to_string();
    }
    let mut cut = limit;
    while cut > 0 && !trimmed.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}… (truncated)", &trimmed[..cut])
}

/// Runs an external program and captures its result.
///
/// Implementations must not consult a shell: the `args` slice is the
/// argument vector, one element per argument, per the [`super::argv`]
/// contract.
pub trait CommandRunner: Send + Sync {
    /// Run `program` with `args`, wait for it, and capture its streams.
    ///
    /// The `Err` arm is reserved for failures to *run* the program
    /// (missing file, permission denied, exec format error). A program
    /// that ran and failed is `Ok` with a non-zero
    /// [`CommandOutcome::code`].
    fn run(&self, program: &Path, args: &[OsString]) -> std::io::Result<CommandOutcome>;
}

/// The production runner: [`std::process::Command`] with a null stdin.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &Path, args: &[OsString]) -> std::io::Result<CommandOutcome> {
        let output = std::process::Command::new(program)
            .args(args)
            // See the module note: never let the child block on a prompt.
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()?;
        Ok(CommandOutcome {
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

/// A scripted [`CommandRunner`] for tests.
///
/// Responses are consumed in order; each invocation is recorded so a
/// test can assert on the exact argument vector that would have been
/// handed to the OS. Available under `cfg(test)` only — it is a test
/// seam, not public surface.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct StubRunner {
    script: std::sync::Mutex<std::collections::VecDeque<StubResponse>>,
    calls: std::sync::Mutex<Vec<StubCall>>,
}

/// One scripted answer for [`StubRunner`].
#[cfg(test)]
#[derive(Debug, Clone)]
pub enum StubResponse {
    /// The process ran and reported this.
    Ran(CommandOutcome),
    /// The process could not be started.
    Failed(std::io::ErrorKind),
}

/// One recorded invocation.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubCall {
    /// Program that would have been spawned.
    pub program: std::path::PathBuf,
    /// Argument vector, verbatim.
    pub args: Vec<OsString>,
}

#[cfg(test)]
impl StubRunner {
    /// An empty stub. Any invocation panics until a response is queued.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a successful run with the given code and streams.
    pub fn push_run(&self, code: Option<i32>, stdout: &str, stderr: &str) -> &Self {
        self.push(StubResponse::Ran(CommandOutcome::new(
            code,
            stdout.as_bytes(),
            stderr.as_bytes(),
        )))
    }

    /// Queue a spawn failure.
    pub fn push_spawn_failure(&self, kind: std::io::ErrorKind) -> &Self {
        self.push(StubResponse::Failed(kind))
    }

    fn push(&self, response: StubResponse) -> &Self {
        self.script
            .lock()
            .expect("stub script lock")
            .push_back(response);
        self
    }

    /// Every invocation so far, in order.
    pub fn calls(&self) -> Vec<StubCall> {
        self.calls.lock().expect("stub call lock").clone()
    }

    /// The argument vector of the `nth` invocation.
    pub fn args(&self, nth: usize) -> Vec<OsString> {
        self.calls()
            .get(nth)
            .unwrap_or_else(|| panic!("no invocation #{nth} was recorded"))
            .args
            .clone()
    }
}

#[cfg(test)]
impl CommandRunner for StubRunner {
    fn run(&self, program: &Path, args: &[OsString]) -> std::io::Result<CommandOutcome> {
        self.calls.lock().expect("stub call lock").push(StubCall {
            program: program.to_path_buf(),
            args: args.to_vec(),
        });
        let response = self
            .script
            .lock()
            .expect("stub script lock")
            .pop_front()
            .unwrap_or_else(|| {
                panic!(
                    "StubRunner: unscripted invocation of {} with {:?}",
                    program.display(),
                    super::argv::redact_argv(args)
                )
            });
        match response {
            StubResponse::Ran(outcome) => Ok(outcome),
            StubResponse::Failed(kind) => Err(std::io::Error::new(kind, "scripted spawn failure")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_text_covers_both_streams() {
        let outcome = CommandOutcome::new(Some(0), "on stdout", "on stderr");
        assert_eq!(outcome.combined_text(), "on stdout\non stderr");

        let only_err = CommandOutcome::new(Some(2), "", "banner here");
        assert_eq!(only_err.combined_text(), "banner here");

        let only_out = CommandOutcome::new(Some(0), "banner here\n", "");
        assert_eq!(only_out.combined_text(), "banner here\n");
    }

    #[test]
    fn stderr_excerpt_truncates_on_a_char_boundary() {
        let outcome = CommandOutcome::new(Some(2), "", "  short  ");
        assert_eq!(outcome.stderr_excerpt(64), "short");

        let long = "é".repeat(40); // 80 bytes
        let outcome = CommandOutcome::new(Some(2), "", long.as_str());
        let excerpt = outcome.stderr_excerpt(9);
        assert!(excerpt.ends_with("… (truncated)"), "{excerpt}");
        assert!(excerpt.starts_with("é"), "{excerpt}");
        // 9 is mid-character; the cut must fall back to 8.
        assert_eq!(excerpt.chars().filter(|c| *c == 'é').count(), 4);
    }

    #[test]
    fn stub_records_calls_and_replays_in_order() {
        let stub = StubRunner::new();
        stub.push_run(Some(0), "first", "");
        stub.push_run(Some(3), "", "boom");

        let args = vec![OsString::from("a"), OsString::from("-hpsecret")];
        let first = stub.run(Path::new("/opt/WinRAR/rar"), &args).expect("ran");
        assert_eq!(first.code, Some(0));
        let second = stub.run(Path::new("/opt/WinRAR/rar"), &[]).expect("ran");
        assert_eq!(second.code, Some(3));

        let calls = stub.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].program, Path::new("/opt/WinRAR/rar"));
        assert_eq!(stub.args(0), args);
        assert!(stub.args(1).is_empty());
    }

    #[test]
    fn stub_can_script_a_spawn_failure() {
        let stub = StubRunner::new();
        stub.push_spawn_failure(std::io::ErrorKind::PermissionDenied);
        let err = stub
            .run(Path::new("/opt/WinRAR/rar"), &[])
            .expect_err("scripted failure");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    #[should_panic(expected = "unscripted invocation")]
    fn stub_panics_on_an_unscripted_invocation() {
        let stub = StubRunner::new();
        let _ = stub.run(Path::new("/opt/WinRAR/rar"), &[]);
    }
}
