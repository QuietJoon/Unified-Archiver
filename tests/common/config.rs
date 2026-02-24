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

/// Parse TOML config file
fn parse_config_toml(content: &str) -> Option<PathBuf> {
    // Simple TOML parser for temp_dir setting
    // Format: temp_dir = "/path/to/temp"
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("temp_dir") {
            if let Some(value) = line.split('=').nth(1) {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                return Some(PathBuf::from(value));
            }
        }
    }
    None
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
            temp_dir = "/Volumes/Temp/claude"
        "#;
        let result = parse_config_toml(content);
        assert_eq!(result, Some(PathBuf::from("/Volumes/Temp/claude")));
    }

    #[test]
    fn test_parse_config_toml_with_quotes() {
        let content = r#"temp_dir = "/tmp/test""#;
        let result = parse_config_toml(content);
        assert_eq!(result, Some(PathBuf::from("/tmp/test")));
    }
}
