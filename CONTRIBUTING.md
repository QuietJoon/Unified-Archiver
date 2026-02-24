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
git clone https://github.com/yourusername/7zip-rbinding
cd 7zip-rbinding
cargo build
```

The build.rs script automatically:
- Links libarchive via pkg-config (or Homebrew on macOS)
- Compiles bundled UnRAR SDK for RAR/RAR5 support
- Generates FFI bindings

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test integration

# Run with output
cargo test -- --nocapture

# Run benchmarks
cargo bench
```

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
├── archive.rs          # Archive handle and operations
├── entry.rs            # ArchiveEntry metadata
├── error.rs            # Error types
├── format.rs           # Format detection
├── options.rs          # Configuration structs
├── ffi/                # FFI bindings
│   ├── unrar.rs       # UnRAR SDK bindings
│   ├── libarchive.rs  # libarchive bindings
│   └── wrapper.rs     # Safe wrappers
└── sfx/                # SFX detection module

tests/
├── integration/        # Integration test modules
├── fixtures/          # Test archives
└── common/            # Shared test utilities

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

- Use present tense ("Add feature" not "Added feature")
- First line: brief summary (50 chars or less)
- Blank line, then detailed description if needed
- Reference issues and PRs where appropriate

Examples:
- `Add multi-part archive creation support`
- `Fix CRC32 validation for RAR5 archives`
- `Update documentation for Archive::modify()`

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
/// let options = ExtractionOptions {
///     destination: "output/".into(),
///     ..Default::default()
/// };
/// archive.extract_all(options)?;
/// # Ok::<(), unified_archive::ArchiveError>(())
/// ```
pub fn extract_all(&self, options: ExtractionOptions) -> Result<()> {
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

1. **Ensure CI passes**
   - All tests pass
   - No clippy warnings
   - Code is formatted
   - Documentation builds

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

By contributing, you agree that your contributions will be licensed under:
- Apache License 2.0 OR
- MIT License

(Dual license, contributor's choice)

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
