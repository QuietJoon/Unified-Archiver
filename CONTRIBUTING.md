# Contributing to unified-archive

Thank you for your interest in contributing to unified-archive! This document provides guidelines for contributing to the project.

## Code of Conduct

This project adheres to professional standards of behavior. Please be respectful and constructive in all interactions.

## Development Setup

### Prerequisites

- **Rust 1.85+** (stable channel, 2024 edition)
- **libarchive** (system library)
  - macOS: `brew install libarchive`
  - Ubuntu/Debian: `sudo apt-get install libarchive-dev`
  - Fedora/RHEL: `sudo dnf install libarchive-devel`
- **pkg-config** for FFI linking
- **UnRAR SDK** (bundled, compiled automatically via build.rs)

### Building

```bash
git clone https://github.com/QuietJoon/unified-archive
cd unified-archive
cargo build
```

The build.rs script automatically:
- Links libarchive via pkg-config (or Homebrew on macOS)
- Compiles the bundled UnRAR SDK for RAR/RAR5 support with the `cc` crate

The FFI declarations are **not** generated: `src/ffi/libarchive.rs` and
`src/ffi/unrar.rs` are hand-written `unsafe extern "C"` blocks, so adding a
native call means adding its declaration by hand.

### Running Tests

```bash
# Default profile (rar-support + v2-api)
cargo test -- --test-threads=4

# The two profiles the release gate requires
cargo test --all-features -- --test-threads=4
cargo test --no-default-features -- --test-threads=4

# Run one test binary
cargo test --test integration_tests
cargo test --test contract_tests

# Filter down to a single module inside a binary
cargo test --test integration_tests modification

# Run with output
cargo test -- --nocapture

# Run benchmarks
cargo bench
```

`tests/integration/` and `tests/contract/` are module trees, not cargo test
targets: each is reached through a single binary (`tests/integration_tests.rs`,
`tests/contract_tests.rs`) that declares them via `mod`. Select an individual
module with a filter argument, as above, rather than with `--test`.

### Testing Philosophy

The project follows comprehensive testing principles:
- **Unit tests**: >90% coverage target for individual modules
- **Integration tests**: Real archive samples from official tools
- **Property-based tests**: Invariant validation with proptest
- **Contract tests**: Public API stability verification
- **Performance tests**: Regression detection for critical paths

## Project Structure

```
src/
├── lib.rs              # Public API exports
├── archive.rs          # Archive handle, format detection, backend routing
├── archive/mode_split.rs # v2-api typed handles (ReadArchive/WriteArchive/ModifyArchive)
├── backend.rs          # ReadBackend trait — read-side backend dispatch
│                       #   (WriteBackend is an enum in creation.rs;
│                       #    a ModifyBackend trait is only planned)
├── entry.rs            # ArchiveEntry metadata + builder
├── error.rs            # Error types and Operation enum
├── format.rs           # Format detection + capability matrix
├── options.rs          # Configuration structs (ExtractionOptions, CompressionOptions, typed builders)
├── inspection.rs       # Listing, find_entry/find_entries, integrity, multipart_layout
├── extraction.rs       # extract_all, extract_file, selective extraction, progress
├── creation.rs         # Archive::create + create_zip/create_seven_zip/create_libarchive
├── modification.rs     # Modify-mode tracker and commit_changes
├── security.rs         # ExtractionLimits + crate-internal sanitize/verify helpers
├── streaming.rs        # StreamingExtractor (Read trait wrapper)
├── stream_crc.rs       # GZIP/BZIP2/XZ checksum parsing
├── password.rs         # Password handling
├── fs_identity.rs      # Read-handle file-identity binding (DCR-014)
├── payload_window.rs   # In-place SFX payload window (DCR-015)
├── ffi/                # FFI bindings (unrar, libarchive, zip, sevenz)
│   ├── unrar.rs       # UnRAR SDK bindings
│   ├── libarchive.rs  # libarchive bindings
│   └── wrapper.rs     # Safe wrappers
├── sfx/                # SFX detection module
└── external/rar.rs     # Optional WinRAR CLI bridge (external-rar-create)

tests/
├── integration_tests.rs  # Binary for the tests/integration/ module tree
├── integration/          # Integration test modules
├── contract_tests.rs     # Binary for the tests/contract/ module tree
├── contract/             # Contract test modules
├── *_test.rs             # Standalone integration binaries (run with --test <name>)
├── fixtures/             # Test archives (committed, incl. sparse.tar)
└── common/               # Shared test utilities

examples/              # Usage demonstrations
benches/              # Performance benchmarks
```

## Making Changes

### Development Workflow

1. **Create a feature branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**
   - Follow Rust 2024 edition requirements
   - Add tests for new functionality
   - Update documentation
   - Ensure code compiles without warnings

3. **Run checks before committing**
   ```bash
   cargo fmt          # Format code
   cargo clippy       # Lint code
   cargo test         # Run tests
   cargo doc --open   # Verify documentation
   ```

4. **Commit your changes**
   ```bash
   git add .
   git commit -m "Brief description of changes"
   ```

5. **Push and create pull request**
   ```bash
   git push origin feature/your-feature-name
   ```

### Commit Message Guidelines

- Use Conventional Commits: `type(scope): summary`. Types in use here are `feat`, `fix`, `docs`, `refactor`,
  `test`, `chore`, `build`, `security`.
- Append `!` after the type/scope for a breaking change: `feat(api)!: widen recovery_percentage to u16`.
- Use present tense. Subjects here are descriptive rather than terse — 60-95 characters is normal.
- Blank line, then a body carrying the reasoning, the evidence, and the gate result.
- Reference records (AD / MADR / DCR under `docs/records/`), open issues, and ticgit ids where relevant.

Examples:
- `feat(create): add multi-part archive creation support`
- `fix(unrar): validate CRC32 for RAR5 archives`
- `docs(api): rule that the id route answers non-UTF-8 entry access`

## Coding Standards

### Rust Style

- Follow standard Rust conventions (rustfmt configuration)
- Use descriptive variable names
- Keep functions focused and small
- Prefer explicit types over inference in public APIs

### Rust 2024 Edition Requirements

- All `extern` blocks MUST be marked `unsafe`
- Unsafe operations inside `unsafe fn` MUST use explicit `unsafe` blocks
- Follow enhanced FFI safety requirements

### Error Handling

- Use `Result<T, ArchiveError>` for fallible operations
- Provide context in error messages
- Include operation name, file path, and underlying cause
- Example:
  ```rust
  ArchiveError::io("extract", path.clone(), source_error)
  ```

### Documentation

- Public items MUST have doc comments (`///`)
- Include examples in doc comments where helpful
- Document safety requirements for unsafe code
- Keep docs concise but complete

Example:
```rust
/// Extracts all files from the archive to the specified destination.
///
/// # Examples
///
/// ```no_run
/// use unified_archive::{Archive, ExtractionOptions};
///
/// let archive = Archive::open("test.zip")?;
/// let options = ExtractionOptions::new("output/");
/// let result = archive.extract_all(options)?;
/// for warning in &result.warnings { eprintln!("{warning}"); }
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub fn extract_all(&self, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> {
    // ...
}
```

### Testing

- Add tests for all new functionality
- Use descriptive test names: `test_extract_password_protected_zip()`
- Include positive and negative test cases
- Test edge cases and error conditions
- Use fixtures from `tests/fixtures/` directory

## Constitutional Principles

The project follows five core principles (see `.specify/memory/constitution.md`):

1. **Robustness & Stability**: Handle all errors explicitly, validate inputs, maintain invariants
2. **Pragmatic Performance**: Optimize where it counts, measure before and after
3. **Unified Interface**: Platform-agnostic public API with optimized backends
4. **Comprehensive Testing**: >90% coverage, integration tests, property tests
5. **Clear Contracts**: Explicit specifications for all public APIs

All contributions must align with these principles.

## Pull Request Process

1. **Run the gate and record it**
   - This project has **no hosted CI** (AD-0070). "CI-covered" means a committed command was run on a host that can
     observe the property, and its native exit status was recorded against a commit.
   - Run `scripts/release-gate.sh` in the foreground of a terminal you own — not detached, not from a background
     task. Its lanes are `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
     `cargo test --all-features -- --test-threads=4`, `cargo test --no-default-features -- --test-threads=4`, and
     `cargo check --all-targets`.
   - Commit the resulting record under `docs/verification/`, naming the platform you ran on. A platform with no
     record is unverified, not assumed-green.

2. **Update documentation**
   - Update README.md if adding user-facing features
   - Update CHANGELOG.md under "Unreleased"
   - Add/update examples if applicable

3. **Get review**
   - PRs require approval before merging
   - Address review feedback promptly
   - Keep PRs focused and reasonably sized

4. **Merge**
   - Use squash merge for feature branches
   - Maintainers will handle the merge

## Feature Requests and Bug Reports

### Reporting Bugs

Include:
- Rust version: `rustc --version`
- Operating system and version
- Minimal reproduction case
- Expected vs actual behavior
- Error messages and stack traces

### Requesting Features

Include:
- Use case description
- Proposed API if applicable
- Alternative approaches considered
- Impact assessment (breaking change? performance?)

## Architecture Decisions

For significant changes:
1. Open an issue first to discuss the approach
2. Consider impact on:
   - Public API stability
   - Performance characteristics
   - Cross-platform compatibility
   - Memory usage
   - Dependencies

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

## Questions?

- Open an issue for questions
- Check existing issues and documentation first
- Be specific and provide context

## Recognition

Contributors are recognized in:
- CHANGELOG.md for their contributions
- Git commit history
- Release notes

Thank you for contributing to unified-archive!
