# API Contract: Error Types

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Purpose**: Unified error handling across all archive formats

## ArchiveError Definition

```rust
#[derive(Debug)]
pub enum ArchiveError {
    Io {
        operation: String,
        path: PathBuf,
        source: std::io::Error,
    },
    Format {
        format: Option<ArchiveFormat>,
        message: String,
    },
    Corruption {
        path: String,
        details: String,
    },
    Password {
        message: String,
    },
    Unsupported {
        operation: String,
        format: ArchiveFormat,
        details: Option<String>, // e.g., "Missing codec: PPMd. Install libarchive with PPMd support."
    },
    InvalidPath {
        path: String,
        reason: String,
    },
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ArchiveError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ArchiveError::Io { operation, path, source } => {
                write!(f, "I/O error during {}: {} ({})",
                    operation, path.display(), source)
            }
            ArchiveError::Format { format, message } => {
                if let Some(fmt) = format {
                    write!(f, "{:?} format error: {}", fmt, message)
                } else {
                    write!(f, "Archive format error: {}", message)
                }
            }
            ArchiveError::Corruption { path, details } => {
                write!(f, "Corrupted entry '{}': {}", path, details)
            }
            ArchiveError::Password { message } => {
                write!(f, "Password error: {}", message)
            }
            ArchiveError::Unsupported { operation, format, details } => {
                if let Some(d) = details {
                    write!(f, "Operation '{}' not supported for {:?} format: {}",
                        operation, format, d)
                } else {
                    write!(f, "Operation '{}' not supported for {:?} format",
                        operation, format)
                }
            }
            ArchiveError::InvalidPath { path, reason } => {
                write!(f, "Invalid path '{}': {}", path, reason)
            }
        }
    }
}
```

## Error Categories

### Recoverable Errors
- `Password`: User can retry with correct password
- `Io` (file not found): User can provide correct path

### Fatal Errors
- `Corruption`: Archive is damaged, extraction unsafe
- `Format`: Unsupported or invalid format
- `Unsupported`: Operation impossible for format, or required codec unavailable (FR-024)

## Format-Agnostic Guarantee

**Critical (FR-010)**: Same error types across all formats.

```rust
// All formats return same error variants
match Archive::open("corrupted.zip") {
    Err(ArchiveError::Corruption { .. }) => { /* Handle */ }
    _ => {}
}

match Archive::open("corrupted.rar") {
    Err(ArchiveError::Corruption { .. }) => { /* Same handling */ }
    _ => {}
}
```

## Error Construction Helpers

```rust
impl ArchiveError {
    pub fn io(operation: impl Into<String>, path: impl AsRef<Path>,
              source: std::io::Error) -> Self {
        Self::Io {
            operation: operation.into(),
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn format(format: Option<ArchiveFormat>,
                  message: impl Into<String>) -> Self {
        Self::Format {
            format,
            message: message.into(),
        }
    }

    // ... other constructors
}
```

## Error Message Requirements

Per constitution Principle V (Clear Contracts):

1. **Actionable**: Tell user what to do
2. **Specific**: Include paths, operation names
3. **Contextual**: Preserve underlying error sources
4. **Consistent**: Same wording across formats

**Good Example**:
```
I/O error during extraction: /path/to/output/file.txt (Permission denied)
```

**Bad Example**:
```
Error occurred
```

## Contract Tests

1. Verify all error variants are returned appropriately
2. Test error message clarity and actionability
3. Verify error source preservation
4. Test Display implementation formatting
