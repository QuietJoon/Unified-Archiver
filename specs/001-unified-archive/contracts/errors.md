# API Contract: Error Types

**Feature**: 001-unified-archive
**Date**: 2025-10-30
**Purpose**: Unified error handling across all archive formats
**Status**: Implemented (retrospective documentation)

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
        details: Option<String>, // e.g., "RAR format does not support modification"
    },
    InvalidPath {
        path: String,
        reason: String,
    },
    CodecUnavailable {
        codec: String,
        format: ArchiveFormat,
        install_instructions: String,
    },
    UnsupportedOperation {
        operation: String,
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
            ArchiveError::CodecUnavailable { codec, format, install_instructions } => {
                write!(f, "Codec '{}' unavailable for format '{}': {}",
                    codec, format, install_instructions)
            }
            ArchiveError::UnsupportedOperation { operation, reason } => {
                write!(f, "Unsupported operation '{}': {}", operation, reason)
            }
        }
    }
}
```

## Error Categories

### Recoverable Errors (illustrative, not exhaustive)
- `Password`: User can retry with correct password
- `Io` (file not found): User can provide correct path
- `Io` (`ErrorKind::AlreadyExists`): Destination file already exists; user can choose to overwrite, rename, or skip

### Fatal Errors (illustrative, not exhaustive)
- `Format`: Open-time failures -- bad magic bytes, truncated header, unsupported or invalid format
- `Corruption`: Data-integrity failures -- CRC mismatch, bad data during extraction
- `Unsupported`: Operation impossible for format
- `CodecUnavailable`: Required codec not available (e.g., missing PPMd support)
- `UnsupportedOperation`: Operation not applicable in the current context
- `InvalidPath`: Path traversal or invalid entry path; security-critical terminal failure

> **Note**: The recoverable/fatal split above is a useful mental model but does not
> cover every variant cleanly. Overwrite conflicts may be recoverable via user
> policy, codec failures may be resolvable by installing an optional dependency,
> and unsupported-operation errors are neither "recoverable" nor "fatal" in the
> traditional sense -- they indicate a capability gap. Treat the categories as
> guidance for common cases, not as an exhaustive classification.

## Format-Agnostic Guarantee

**Critical (FR-010)**: All formats share the same `ArchiveError` enum. The guarantee
is at the *error family* level -- callers can match on variant discriminants
(`Format { .. }`, `Corruption { .. }`, `Password { .. }`, etc.) uniformly across
backends. However, the specific variants each backend can produce differ: not every
backend surfaces every variant, and the granularity of the inner detail fields
(e.g., `message`, `details`) is backend-dependent. Callers should not depend on
exact variant-to-scenario mappings being identical across backends.

**Known exceptions**:
- `CodecUnavailable` is primarily relevant to 7z and RAR; ZIP backends resolve
  codecs at compile time and surface `Format` errors instead.
- `Corruption` detail strings vary in specificity: ZIP backends report CRC values,
  while 7z backends may only report a generic integrity failure.
- `Password` semantics differ: some backends distinguish "wrong password" from
  "encryption not supported"; others conflate both.

Callers should match on the variant (e.g., `ArchiveError::Format { .. }`) for
control flow but must not parse or depend on the inner string fields for
programmatic decisions.

```rust
// All formats return ArchiveError variants -- match on variant, not message
// Format errors at open time (bad magic, truncated header)
match Archive::open("bad_header.zip") {
    Err(ArchiveError::Format { .. }) => { /* Handle open-time failure */ }
    _ => {}
}

// Corruption errors during extraction (CRC mismatch)
match archive.extract_all(options) {
    Err(ArchiveError::Corruption { .. }) => { /* Handle data-integrity failure */ }
    _ => {}
}
```

## Selected Error Construction Helpers

The following are representative examples of the convenience constructors on
`ArchiveError`. See `src/error.rs` for the full set (including constructors for
`Corruption`, `Password`, `Unsupported`, `InvalidPath`, `CodecUnavailable`, and
`UnsupportedOperation`).

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

    // ... additional constructors in src/error.rs
}
```

## Error Message Requirements

Per constitution Principle V (Clear Contracts):

1. **Actionable**: Tell user what to do
2. **Specific**: Include paths, operation names
3. **Contextual**: Preserve underlying error sources
4. **Consistent** *(documentation goal, not hard contract)*: Aim for similar
   wording across formats, but backend-specific detail strings may differ.
   This is a best-effort aspiration; callers must not rely on exact message
   text for programmatic decisions

**Good Example**:
```
I/O error during extraction: /path/to/output/file.txt (Permission denied)
```

**Bad Example**:
```
Error occurred
```

## Contract Tests

The following verification goals apply to the error contract. Relevant test files
include `tests/integration/` (format-specific extraction tests that exercise error
paths) and `src/error.rs` (unit tests for Display formatting and source chaining).

1. Verify all error variants are returned appropriately
2. Test error message clarity and actionability
3. Verify error source preservation (especially `Io { source, .. }`)
4. Test Display implementation formatting
