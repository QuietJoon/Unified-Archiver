//! The argument-vector contract for the external `rar` shell-out.
//!
//! # The contract
//!
//! 1. **No command string is ever built.** Arguments are assembled as a
//!    `Vec<OsString>` and handed to the OS process API one element at a
//!    time ([`std::process::Command::args`]). There is no shell, no
//!    `cmd.exe /C`, no `format!`-ed command line, and therefore no
//!    metacharacter to escape: a path containing a space, `&`, `|`, `^`,
//!    `>` or `"` is one argument because the vector says it is one
//!    argument, not because it was quoted correctly.
//! 2. **`--` terminates switch scanning.** Every path — the archive and
//!    each entry — is emitted after the `--` sentinel, so a path
//!    beginning with `-` cannot be read as a switch.
//! 3. **Leading-dash paths are re-rooted anyway.** `./-x.txt` is
//!    emitted instead of `-x.txt`, so a `rar` build that ignores `--`
//!    still cannot mistake the path for a switch. Belt and suspenders,
//!    because guard 2 depends on the installed binary's cooperation and
//!    guard 3 does not.
//! 4. **Arguments that cannot survive the process boundary are refused
//!    up front** with a typed error rather than being truncated,
//!    mangled, or silently passed to the OS. Concretely: interior NUL
//!    bytes (a Windows/Unix argument terminator), an empty password
//!    (which makes `rar` open an interactive prompt and block forever),
//!    a password carrying CR/LF, and an empty entry list.
//! 5. **Passwords never appear in diagnostics.** [`redact_argv`]
//!    produces the only form of the argument vector this crate is
//!    allowed to log or embed in an error message.
//!
//! # What this contract cannot fix
//!
//! On Windows the vector is serialised by `std` into a single command
//! line using the MSVCRT quoting rules and re-split by the child. That
//! round-trip is exact for `rar.exe`, which is an MSVCRT-parsed
//! executable — but it is *not* exact for a `.bat`/`.cmd` file, which
//! `cmd.exe` re-parses with different rules that no escaping in `std`
//! fully covers. The discovery layer therefore refuses to treat a batch
//! file as the rar binary; see [`super::discovery::vet_program`].
//!
//! Password secrecy against *local process listing* is likewise out of
//! reach here: `-hp<password>` is an argument, so it is visible to
//! anything that can enumerate command lines while the child runs. That
//! limitation is documented on the public setter.
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The `rar` sentinel that stops switch scanning: everything after it is
/// a path.
pub const END_OF_SWITCHES: &str = "--";

/// Placeholder substituted for the `-hp…` element by [`redact_argv`].
pub const REDACTED_PASSWORD_ARG: &str = "-hp<redacted>";

/// Which slot of the argument vector a rejected value came from.
///
/// Carried by [`ArgvError`] so a caller can tell "your output path is
/// unusable" from "your password is unusable" without parsing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgRole {
    /// The archive being written.
    Output,
    /// One of the files/directories being added.
    Entry,
    /// The `-hp` password payload.
    Password,
    /// The `-mN` compression switch.
    CompressionFlag,
}

impl ArgRole {
    /// Stable label for messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Output => "output archive path",
            Self::Entry => "entry path",
            Self::Password => "password",
            Self::CompressionFlag => "compression flag",
        }
    }
}

impl core::fmt::Display for ArgRole {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

/// An input that cannot be expressed as a `rar` argument.
///
/// Every variant is a refusal *before* any process is spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgvError {
    /// Nothing to archive. `rar a` with no input would either prompt or
    /// produce an empty/absent archive depending on the build.
    NoEntries,
    /// An argument carried an interior NUL byte, which terminates an
    /// argument at the OS boundary and would silently truncate it.
    ///
    /// `shown` is a lossy rendering of the offending value, and is
    /// suppressed entirely for [`ArgRole::Password`].
    InteriorNul { role: ArgRole, shown: String },
    /// A path slot was empty.
    EmptyPath { role: ArgRole },
    /// A password was set to the empty string. `-hp` with no payload
    /// makes `rar` prompt on the console and wait forever, so it is
    /// refused instead of being sent.
    EmptyPassword,
    /// A password contained CR or LF. Those cannot be delivered through
    /// the `-hp` switch and would be interpreted as input-line
    /// boundaries by anything that echoes the argument vector.
    PasswordControlCharacter,
    /// The compression switch was not one of `-m0` … `-m5`. Guards the
    /// slot against being used to smuggle an arbitrary switch.
    InvalidCompressionFlag { shown: String },
}

impl core::fmt::Display for ArgvError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoEntries => {
                f.write_str("cannot create an empty RAR archive - add at least one entry")
            }
            Self::InteriorNul { role, .. } if *role == ArgRole::Password => {
                write!(f, "{role} contains an interior NUL byte")
            }
            Self::InteriorNul { role, shown } => {
                write!(f, "{role} '{shown}' contains an interior NUL byte")
            }
            Self::EmptyPath { role } => write!(f, "{role} is empty"),
            Self::EmptyPassword => f.write_str(
                "password is empty; '-hp' without a payload makes rar block on an interactive prompt",
            ),
            Self::PasswordControlCharacter => {
                f.write_str("password contains a carriage return or line feed")
            }
            Self::InvalidCompressionFlag { shown } => {
                write!(f, "compression flag '{shown}' is not one of -m0..-m5")
            }
        }
    }
}

impl std::error::Error for ArgvError {}

/// Everything the `rar a` argument vector needs.
///
/// A struct rather than five positional parameters so a future switch
/// cannot be added in the wrong slot at a call site.
#[derive(Debug, Clone, Copy)]
pub struct AddArgv<'a> {
    /// The `-mN` compression switch, from the crate-internal
    /// compression-level mapping (`rar_compression_flag`).
    pub compression_flag: &'a str,
    /// Password payload for `-hp`, if the archive is encrypted.
    pub password: Option<&'a str>,
    /// Emit `-r` so named directories are archived recursively.
    pub recurse: bool,
    /// The archive to write.
    pub output: &'a Path,
    /// Files and directories to add. Must be non-empty.
    pub entries: &'a [PathBuf],
}

/// Build the argument vector (everything after the program name) for a
/// `rar a` invocation, or refuse with a typed error.
///
/// The emitted order is: `a`, `-mN`, `-hp…` (optional), `-r`
/// (optional), `--`, output, entries. Guards 2–4 of the module contract
/// are enforced here.
pub fn build(spec: &AddArgv<'_>) -> Result<Vec<OsString>, ArgvError> {
    if spec.entries.is_empty() {
        return Err(ArgvError::NoEntries);
    }
    if !is_known_compression_flag(spec.compression_flag) {
        return Err(ArgvError::InvalidCompressionFlag {
            shown: spec.compression_flag.to_string(),
        });
    }

    let mut args: Vec<OsString> = Vec::with_capacity(spec.entries.len() + 5);

    // `a` — add files to archive.
    args.push("a".into());
    args.push(spec.compression_flag.into());

    if let Some(password) = spec.password {
        if password.is_empty() {
            return Err(ArgvError::EmptyPassword);
        }
        if password.contains('\r') || password.contains('\n') {
            return Err(ArgvError::PasswordControlCharacter);
        }
        if password.as_bytes().contains(&0) {
            return Err(ArgvError::InteriorNul {
                role: ArgRole::Password,
                shown: String::new(),
            });
        }
        args.push(format!("-hp{password}").into());
    }

    if spec.recurse {
        args.push("-r".into());
    }

    // Guard 2: nothing after this point can be scanned as a switch.
    args.push(END_OF_SWITCHES.into());

    args.push(vet_path(spec.output, ArgRole::Output)?);
    for entry in spec.entries {
        args.push(vet_path(entry, ArgRole::Entry)?);
    }

    Ok(args)
}

/// Validate one path slot and apply guard 3 (leading-dash re-rooting).
fn vet_path(path: &Path, role: ArgRole) -> Result<OsString, ArgvError> {
    let raw = path.as_os_str();
    if raw.is_empty() {
        return Err(ArgvError::EmptyPath { role });
    }
    if raw.as_encoded_bytes().contains(&0) {
        return Err(ArgvError::InteriorNul {
            role,
            shown: raw.to_string_lossy().replace('\0', "\\0"),
        });
    }
    Ok(reroot_leading_dash(raw))
}

/// Prefix `./` to a path that begins with `-`.
///
/// Applied to the archive path as well as to entries: a build that
/// ignores `--` would misread either one.
fn reroot_leading_dash(raw: &OsStr) -> OsString {
    if raw.as_encoded_bytes().first() == Some(&b'-') {
        Path::new(".").join(raw).into_os_string()
    } else {
        raw.to_os_string()
    }
}

/// The six switches the level mapping is allowed to produce.
fn is_known_compression_flag(flag: &str) -> bool {
    matches!(flag, "-m0" | "-m1" | "-m2" | "-m3" | "-m4" | "-m5")
}

/// Render an argument vector for logging or for embedding in an error,
/// with any `-hp…` element replaced by [`REDACTED_PASSWORD_ARG`].
///
/// This is the **only** sanctioned way to turn the vector into text.
/// Formatting the raw `Vec<OsString>` with `{:?}` would write the
/// password into logs and error strings.
pub fn redact_argv(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|arg| {
            let text = arg.to_string_lossy();
            if text.starts_with("-hp") {
                REDACTED_PASSWORD_ARG.to_string()
            } else {
                text.into_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>(output: &'a Path, entries: &'a [PathBuf]) -> AddArgv<'a> {
        AddArgv {
            compression_flag: "-m3",
            password: None,
            recurse: true,
            output,
            entries,
        }
    }

    fn as_text(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// Guard 1: the builder returns a vector. There is no code path that
    /// produces a command *string*, so this test pins the shape callers
    /// depend on: one logical argument per element, never joined.
    #[test]
    fn shell_metacharacters_stay_inside_one_argument() {
        let entries = vec![PathBuf::from("a b & c | d > e \"quoted\" ^caret.txt")];
        let output = PathBuf::from("out dir/ar&chive.rar");
        let args = build(&spec(&output, &entries)).expect("argv builds");
        let text = as_text(&args);

        assert!(
            text.contains(&"a b & c | d > e \"quoted\" ^caret.txt".to_string()),
            "the entry must survive verbatim as a single element: {text:?}"
        );
        assert!(
            text.contains(&"out dir/ar&chive.rar".to_string()),
            "the output must survive verbatim as a single element: {text:?}"
        );
        // Nothing was concatenated: the element count is exactly
        // a, -m3, -r, --, output, entry.
        assert_eq!(args.len(), 6, "unexpected argv shape: {text:?}");
    }

    /// Guard 2 + guard 3 together.
    #[test]
    fn sentinel_precedes_paths_and_dash_paths_are_rerooted() {
        let entries = vec![
            PathBuf::from("-sw.txt"),
            PathBuf::from("normal.txt"),
            PathBuf::from("--recursive"),
        ];
        let output = PathBuf::from("-out.rar");
        let args = build(&spec(&output, &entries)).expect("argv builds");

        let sentinel = args
            .iter()
            .position(|a| a.as_os_str() == OsStr::new(END_OF_SWITCHES))
            .expect("sentinel present");

        // Guard 2: every path is after the sentinel.
        assert_eq!(sentinel, 3, "sentinel must sit right before the paths");
        // Guard 3: no post-sentinel element may begin with '-'.
        for arg in args.iter().skip(sentinel + 1) {
            let text = arg.to_string_lossy();
            assert!(
                !text.starts_with('-'),
                "post-sentinel argument {text:?} could be read as a switch"
            );
        }
        for original in ["-out.rar", "-sw.txt", "--recursive"] {
            let expected: OsString = Path::new(".").join(original).into();
            assert!(
                args.contains(&expected),
                "{original} must be re-rooted, got {:?}",
                as_text(&args)
            );
        }
        assert!(args.contains(&OsString::from("normal.txt")));
    }

    #[test]
    fn switch_order_is_command_level_password_recurse_sentinel() {
        let entries = vec![PathBuf::from("f.txt")];
        let mut s = spec(Path::new("o.rar"), &entries);
        s.password = Some("hunter2");
        s.compression_flag = "-m5";
        let args = as_text(&build(&s).expect("argv builds"));
        assert_eq!(
            args,
            vec!["a", "-m5", "-hphunter2", "-r", "--", "o.rar", "f.txt"]
        );
    }

    #[test]
    fn recurse_flag_is_optional() {
        let entries = vec![PathBuf::from("f.txt")];
        let mut s = spec(Path::new("o.rar"), &entries);
        s.recurse = false;
        let args = as_text(&build(&s).expect("argv builds"));
        assert_eq!(args, vec!["a", "-m3", "--", "o.rar", "f.txt"]);
    }

    /// A password whose text looks like a switch must still land in the
    /// `-hp` payload and never become an argument of its own.
    #[test]
    fn switch_shaped_password_stays_a_payload() {
        let entries = vec![PathBuf::from("f.txt")];
        let mut s = spec(Path::new("o.rar"), &entries);
        s.password = Some("-r -x*.txt");
        let args = as_text(&build(&s).expect("argv builds"));
        assert!(args.contains(&"-hp-r -x*.txt".to_string()), "{args:?}");
        assert_eq!(
            args.iter().filter(|a| a.as_str() == "-r").count(),
            1,
            "the password must not add a second -r: {args:?}"
        );
    }

    #[test]
    fn empty_entry_list_is_refused() {
        let empty: Vec<PathBuf> = Vec::new();
        assert_eq!(
            build(&spec(Path::new("o.rar"), &empty)),
            Err(ArgvError::NoEntries)
        );
    }

    /// `-hp` with no payload makes rar block on a console prompt.
    #[test]
    fn empty_password_is_refused() {
        let entries = vec![PathBuf::from("f.txt")];
        let mut s = spec(Path::new("o.rar"), &entries);
        s.password = Some("");
        assert_eq!(build(&s), Err(ArgvError::EmptyPassword));
    }

    #[test]
    fn newline_in_password_is_refused() {
        let entries = vec![PathBuf::from("f.txt")];
        for bad in ["pw\n", "pw\r", "a\r\nb"] {
            let mut s = spec(Path::new("o.rar"), &entries);
            s.password = Some(bad);
            assert_eq!(build(&s), Err(ArgvError::PasswordControlCharacter));
        }
    }

    #[test]
    fn nul_bytes_are_refused_per_role() {
        let entries = vec![PathBuf::from("f.txt")];

        let bad_entry = vec![PathBuf::from("ok.txt"), PathBuf::from("ba\0d.txt")];
        match build(&spec(Path::new("o.rar"), &bad_entry)) {
            Err(ArgvError::InteriorNul { role, .. }) => assert_eq!(role, ArgRole::Entry),
            other => panic!("expected an entry NUL rejection, got {other:?}"),
        }

        match build(&spec(Path::new("o\0.rar"), &entries)) {
            Err(ArgvError::InteriorNul { role, .. }) => assert_eq!(role, ArgRole::Output),
            other => panic!("expected an output NUL rejection, got {other:?}"),
        }

        let mut s = spec(Path::new("o.rar"), &entries);
        s.password = Some("pw\0");
        match build(&s) {
            Err(ArgvError::InteriorNul { role, shown }) => {
                assert_eq!(role, ArgRole::Password);
                assert!(shown.is_empty(), "the password must not be echoed back");
            }
            other => panic!("expected a password NUL rejection, got {other:?}"),
        }
    }

    #[test]
    fn empty_paths_are_refused() {
        let entries = vec![PathBuf::from("")];
        assert_eq!(
            build(&spec(Path::new("o.rar"), &entries)),
            Err(ArgvError::EmptyPath {
                role: ArgRole::Entry
            })
        );
        let ok = vec![PathBuf::from("f.txt")];
        assert_eq!(
            build(&spec(Path::new(""), &ok)),
            Err(ArgvError::EmptyPath {
                role: ArgRole::Output
            })
        );
    }

    /// The compression slot is validated so it cannot be used to inject
    /// an arbitrary switch ahead of the sentinel.
    #[test]
    fn compression_slot_only_accepts_the_known_table() {
        let entries = vec![PathBuf::from("f.txt")];
        for good in ["-m0", "-m1", "-m2", "-m3", "-m4", "-m5"] {
            let mut s = spec(Path::new("o.rar"), &entries);
            s.compression_flag = good;
            assert!(build(&s).is_ok(), "{good} must be accepted");
        }
        for bad in ["-m6", "-m", "m3", "", "-r", "-hpsecret", "-m3 -r"] {
            let mut s = spec(Path::new("o.rar"), &entries);
            s.compression_flag = bad;
            assert_eq!(
                build(&s),
                Err(ArgvError::InvalidCompressionFlag {
                    shown: bad.to_string()
                }),
                "{bad} must be refused"
            );
        }
    }

    /// Guard 5: diagnostics must never carry the password.
    #[test]
    fn redaction_hides_the_password_and_keeps_everything_else() {
        let entries = vec![PathBuf::from("f.txt")];
        let mut s = spec(Path::new("o.rar"), &entries);
        s.password = Some("hunter2");
        let args = build(&s).expect("argv builds");

        let redacted = redact_argv(&args);
        assert_eq!(
            redacted,
            vec![
                "a",
                "-m3",
                REDACTED_PASSWORD_ARG,
                "-r",
                "--",
                "o.rar",
                "f.txt"
            ]
        );
        assert!(
            !redacted.join(" ").contains("hunter2"),
            "redacted argv leaked the password: {redacted:?}"
        );
    }

    #[test]
    fn redaction_is_a_no_op_without_a_password() {
        let entries = vec![PathBuf::from("f.txt")];
        let args = build(&spec(Path::new("o.rar"), &entries)).expect("argv builds");
        assert_eq!(redact_argv(&args), as_text(&args));
    }

    /// Error text for a rejected password must not quote the value.
    #[test]
    fn error_display_never_echoes_a_password() {
        let err = ArgvError::InteriorNul {
            role: ArgRole::Password,
            shown: "hunter2".to_string(),
        };
        let text = err.to_string();
        assert!(!text.contains("hunter2"), "leaked password in {text:?}");
        assert!(text.contains("password"));
    }
}
