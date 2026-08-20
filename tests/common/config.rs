//! Test configuration management
//!
//! Provides configuration settings for temporary directory paths used during testing.
//! This is test-only infrastructure - the library itself uses system temp directories.

use once_cell::sync::OnceCell;
use std::path::PathBuf;

static TEMP_DIR: OnceCell<PathBuf> = OnceCell::new();

/// Get the temporary directory for testing
///
/// Priority order:
/// 1. Environment variable `UNIFIED_ARCHIVE_TEMP_DIR`
/// 2. Config file `.unified-archive.toml` in project root
/// 3. System default (`/tmp` or platform equivalent)
pub fn temp_dir() -> PathBuf {
    TEMP_DIR
        .get_or_init(|| {
            // Priority 1: Environment variable
            if let Ok(env_dir) = std::env::var("UNIFIED_ARCHIVE_TEMP_DIR") {
                let path = PathBuf::from(env_dir);
                if path.exists() || std::fs::create_dir_all(&path).is_ok() {
                    return path;
                }
            }

            // Priority 2: Config file
            if let Some(config_dir) = read_config_file() {
                if config_dir.exists() || std::fs::create_dir_all(&config_dir).is_ok() {
                    return config_dir;
                }
            }

            // Priority 3: System default
            std::env::temp_dir()
        })
        .clone()
}

/// Read temp directory from config file
fn read_config_file() -> Option<PathBuf> {
    use std::fs;

    // Try to find .unified-archive.toml in current directory or parent directories
    let mut current = std::env::current_dir().ok()?;
    loop {
        let config_path = current.join(".unified-archive.toml");
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                return parse_config_toml(&content);
            }
        }

        // Move to parent directory
        if !current.pop() {
            break;
        }
    }

    None
}

/// Parse the top-level `temp_dir` setting out of `.unified-archive.toml`.
///
/// R0001-0087: the previous scanner accepted any trimmed line that merely
/// *started with* `temp_dir` and took whatever followed the first `=`, so a
/// similarly prefixed key (`temp_dir_backup`), a key inside an unrelated
/// `[table]`, or a value with an inline comment could silently redirect
/// every test's scratch output. This is a strict exact-key reader instead:
///
/// * comments and blank lines are skipped;
/// * only the top-level table is considered (the first `[header]` ends it);
/// * the key must be exactly `temp_dir`, separated by a real `=`;
/// * the value must be a complete quoted string, optionally followed by an
///   inline comment.
///
/// Anything else fails closed with `None`, so [`temp_dir`] falls back to the
/// system temp directory rather than trusting a half-understood line.
fn parse_config_toml(content: &str) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Top-level keys precede the first table header; a `temp_dir`
        // under `[something]` is a different setting entirely.
        if line.starts_with('[') {
            break;
        }

        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "temp_dir" {
            continue;
        }
        // A duplicate key is invalid TOML — refuse rather than silently
        // picking one of the two definitions.
        if found.is_some() {
            return None;
        }
        found = Some(parse_toml_string_value(rest.trim())?);
    }

    found
}

/// Decode a TOML string value — basic (`"…"`) or literal (`'…'`) — followed
/// by optional whitespace and an optional `#` comment (R0001-0087).
///
/// Returns `None` for bare or unterminated values, for trailing junk after
/// the closing quote, for an empty path, and for basic strings containing a
/// backslash: this reader deliberately does not implement TOML escape
/// sequences, and guessing at one would produce a wrong directory. Windows
/// paths therefore belong in a literal string (`'C:\scratch'`), which needs
/// no escaping.
fn parse_toml_string_value(value: &str) -> Option<PathBuf> {
    let quote = value.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let body = &value[quote.len_utf8()..];
    let end = body.find(quote)?;
    let (text, tail) = body.split_at(end);
    if quote == '"' && text.contains('\\') {
        return None;
    }

    let tail = tail[quote.len_utf8()..].trim();
    if !tail.is_empty() && !tail.starts_with('#') {
        return None;
    }
    if text.is_empty() {
        return None;
    }

    Some(PathBuf::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_returns_path() {
        let dir = temp_dir();
        assert!(dir.is_absolute());
    }

    #[test]
    fn test_parse_config_toml() {
        let content = r#"
            # Configuration for unified-archive
            temp_dir = "/example/archive-tmp"
        "#;
        let result = parse_config_toml(content);
        assert_eq!(result, Some(PathBuf::from("/example/archive-tmp")));
    }

    #[test]
    fn test_parse_config_toml_with_quotes() {
        let content = r#"temp_dir = "/tmp/test""#;
        let result = parse_config_toml(content);
        assert_eq!(result, Some(PathBuf::from("/tmp/test")));
    }

    /// R0001-0087: a commented-out setting must not be honoured.
    #[test]
    fn test_parse_config_toml_ignores_comments() {
        let content = "# temp_dir = \"/attacker/scratch\"\n";
        assert_eq!(parse_config_toml(content), None);
    }

    /// R0001-0087: only the exact key counts — a longer key that merely
    /// shares the prefix used to win under the old `starts_with` scan.
    #[test]
    fn test_parse_config_toml_rejects_prefixed_key() {
        let content = "temp_dir_backup = \"/attacker/scratch\"\n";
        assert_eq!(parse_config_toml(content), None);
    }

    /// R0001-0087: `temp_dir` under a table is a different setting.
    #[test]
    fn test_parse_config_toml_ignores_table_sections() {
        let content = "[other]\ntemp_dir = \"/attacker/scratch\"\n";
        assert_eq!(parse_config_toml(content), None);
    }

    /// R0001-0087: an inline comment is part of TOML, a bare (unquoted)
    /// value is not — the first must round-trip, the second must fail
    /// closed instead of yielding a path with junk appended.
    #[test]
    fn test_parse_config_toml_value_syntax() {
        assert_eq!(
            parse_config_toml("temp_dir = \"/example/tmp\"  # scratch root\n"),
            Some(PathBuf::from("/example/tmp"))
        );
        assert_eq!(
            parse_config_toml("temp_dir = '/example/literal'\n"),
            Some(PathBuf::from("/example/literal"))
        );
        assert_eq!(parse_config_toml("temp_dir = /example/bare\n"), None);
        assert_eq!(
            parse_config_toml("temp_dir = \"/example/unterminated\n"),
            None
        );
        assert_eq!(parse_config_toml("temp_dir = \"\"\n"), None);
        assert_eq!(
            parse_config_toml("temp_dir = \"/example\" trailing\n"),
            None
        );
        assert_eq!(parse_config_toml("temp_dir = \"C:\\\\scratch\"\n"), None);
    }

    /// R0001-0087: duplicate keys are invalid TOML; refusing beats
    /// silently picking whichever definition the scan reached first.
    #[test]
    fn test_parse_config_toml_rejects_duplicate_key() {
        let content = "temp_dir = \"/first\"\ntemp_dir = \"/second\"\n";
        assert_eq!(parse_config_toml(content), None);
    }

    /// R0001-0087: the shipped `.unified-archive.toml` shape must keep
    /// parsing — the comment block above the setting included lines that
    /// mention the key by name.
    #[test]
    fn test_parse_config_toml_matches_shipped_layout() {
        let content = concat!(
            "# Configuration for unified-archive testing\n",
            "\n",
            "# For tests, you can override with:\n",
            "# 1. UNIFIED_ARCHIVE_TEMP_DIR environment variable (highest priority)\n",
            "temp_dir = \"/Volumes/Temp/claude\"\n",
        );
        assert_eq!(
            parse_config_toml(content),
            Some(PathBuf::from("/Volumes/Temp/claude"))
        );
    }
}
