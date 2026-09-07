---
type: Getting Started
title: "Getting Started with unified-archive"
description: "This guide will walk you through installing and using unified-archive for the first time."
tags: [reference, ADR-0062]
timestamp: 2026-06-10T00:00:00Z
status: active
---

# Getting Started with unified-archive

This guide will walk you through installing and using unified-archive for the first time.

If you want the full user-facing guide for `v0.4.0`, start with the [User Manual](./USER_MANUAL.md) and come back here for the fastest path to a working program.

## Table of Contents

1. [Installation](#installation)
2. [Your First Program](#your-first-program)
3. [Common Use Cases](#common-use-cases)
4. [Next Steps](#next-steps)

---

## Installation

### Prerequisites

**macOS:**
```bash
brew install libarchive pkg-config
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install libarchive-dev pkg-config g++
```

**Linux (Fedora/RHEL):**
```bash
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++
```

> libarchive is discovered at build time via `pkg-config`. The library is **not** bundled with
> unified-archive on macOS or Linux -- you must install it through your system package manager.
>
> **Additional build prerequisites:** `pkg-config` and a C++ compiler (e.g. `g++` or
> `clang++`) are required: `build.rs` compiles the bundled UnRAR SDK sources through the
> `cc` crate.

These prerequisites apply to the **default** feature set. `libarchive` and `pkg-config` are
needed only with the `libarchive` feature; the C++ compiler only with `rar-support`. A ZIP-only
build — `default-features = false, features = ["read", "zip-read"]`, adding `create` **and**
`zip-write` to create ZIPs (`zip-write` selects the writer backend; `create` compiles the
`Archive::create` API in front of it) — needs neither, and is the crate's minimal supported configuration (AD-0058 `read-minimal`).
An empty feature set is *not* a valid build: `src/lib.rs` refuses it with a `compile_error!`.

**Windows:**
macOS and Linux are tested; Windows support is present but not release-verified.

### Add to Your Project

Add unified-archive to your `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.4.0"
```

> **Not on crates.io.** No version of this crate has been published to a public registry, so
> the line above will not resolve as written. Until it is published, depend on the crate by
> `path` or `git`.

### Verify Installation

Create a simple test program:

```rust
// src/main.rs
use unified_archive::ArchiveFormat;

fn main() {
    // ArchiveFormat::detect() performs magic-byte detection on a file.
    // Here we just confirm the crate links and the enum is accessible.
    println!("Example format: {:?}", ArchiveFormat::Zip);
    println!("unified-archive installed successfully!");
}
```

Run it:
```bash
cargo run
```

If you see the success message, you're ready to go!

> **Note:** This only confirms that the crate links. It does not exercise backend integration
> (libarchive, UnRAR SDK, etc.). To verify that backends are wired up correctly, try opening a
> real archive:
>
> ```rust
> // Excerpt — wrap in fn main() -> Result<(), Box<dyn std::error::Error>> { … }
> // to compile as a standalone program.
> let _archive = Archive::open("some_test.zip").expect("backend integration works");
> ```

---

## Your First Program

Let's create a simple program that lists the contents of an archive.

### Step 1: Create a New Project

```bash
cargo new archive-inspector
cd archive-inspector
```

### Step 2: Add Dependency

Edit `Cargo.toml`:
```toml
[dependencies]
unified-archive = "0.4.0"
```

> **Not on crates.io.** No version of this crate has been published to a public registry, so
> the line above will not resolve as written. Until it is published, depend on the crate by
> `path` or `git`.

### Step 3: Write the Code

Edit `src/main.rs`:

```rust
use unified_archive::{Archive, ArchiveError};
use std::env;

fn main() -> Result<(), ArchiveError> {
    // Get archive path from command line
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <archive_file>", args[0]);
        std::process::exit(1);
    }

    let archive_path = &args[1];

    // Open the archive (format is auto-detected)
    println!("Opening: {}", archive_path);
    let archive = Archive::open(archive_path)?;

    // Show archive format
    println!("Format: {:?}\n", archive.format());

    // List all files
    println!("Contents:");
    let entries = archive.list_files()?;

    for entry in entries {
        let size = entry.size.unwrap_or(0);
        let crc = entry.crc32
            .map(|c| format!("{:08X}", c))
            .unwrap_or_else(|| "N/A".to_string());

        println!("  {} - {} bytes (CRC32: {})",
            entry.path,
            size,
            crc
        );
    }

    println!("\nTotal files: {}", entries.len());

    Ok(())
}
```

### Step 4: Test It

Create a test archive (or use an existing one):

> **Prerequisite:** This example requires the `zip` command-line tool
> (`brew install zip` on macOS, `apt-get install zip` on Debian/Ubuntu).

```bash
# Create a simple ZIP for testing
echo "Hello, World!" > test.txt
zip test.zip test.txt
```

Run your program:

```bash
cargo run -- test.zip
```

You should see output like:
```
Opening: test.zip
Format: Zip

Contents:
  test.txt - 14 bytes (CRC32: B4E89E84)

Total files: 1
```

**Congratulations!** You've just created your first unified-archive program!

---

## Common Use Cases

Every extraction below starts from `ExtractionOptions::new(destination)` and
chains one setter per option. Struct-literal construction — including the
`..Default::default()` form — is no longer available outside the library.

### Use Case 1: Extract All Files

```rust
use unified_archive::{Archive, ExtractionOptions};

fn extract_archive(archive_path: &str, output_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    let options = ExtractionOptions::new(output_dir)
        .preserve_permissions(true)
        .preserve_times(true);

    println!("Extracting to {}...", output_dir);
    let result = archive.extract_all(options)?;
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }
    println!("Done!");

    Ok(())
}

// Usage:
// extract_archive("backup.zip", "./output")?;
```

### Use Case 2: Extract Specific Files

```rust
use unified_archive::{Archive, ExtractionOptions};

fn extract_readme(archive_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    // Extract a single file
    let options = ExtractionOptions::new("./");
    archive.extract_file("README.md", options)?;

    println!("README.md extracted!");
    Ok(())
}
```

### Use Case 3: Extract Files by Pattern

```rust
use unified_archive::{Archive, ExtractionOptions};

fn extract_images(archive_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    let options = ExtractionOptions::new("./images");

    // Extract only image files
    archive.extract_filtered(
        |entry| {
            entry.path.ends_with(".jpg") ||
            entry.path.ends_with(".png") ||
            entry.path.ends_with(".gif")
        },
        options
    )?;

    println!("Images extracted!");
    Ok(())
}
```

### Use Case 4: Read File Without Extraction

```rust
use unified_archive::Archive;

fn read_config(archive_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    // Extract directly to memory
    let data = archive.extract_to_memory("config.json")?;

    // Convert to string
    let config = String::from_utf8(data)?;

    Ok(config)
}

// Usage:
// let config = read_config("app.zip")?;
// println!("Config: {}", config);
```

### Use Case 5: Process Large Files Efficiently

```rust
use unified_archive::Archive;
use std::io::Read;
use std::fs::File;
use std::io::Write;

fn extract_large_file(archive_path: &str, file_path: &str, output: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    let archive = Archive::open(archive_path)?;

    // Read incrementally through the streaming API.
    // Note: bounded-memory streaming applies to libarchive-backed formats only;
    // other backends (ZIP, SevenZ, UnRAR) buffer full entries first.
    //
    // `StreamBound::DeclaredSize` is the safe default for untrusted input:
    // it holds the reader to the entry's declared listing size, reporting
    // over-production as `io::ErrorKind::InvalidData` (AD 0062 A.2 /
    // DCR-006) and an early end-of-stream as `io::ErrorKind::UnexpectedEof`
    // (R0001-0011). `StreamBound::Cap(n)` is a ceiling-only resource
    // budget; `StreamBound::Unbounded` opts out of both and is for trusted
    // input only. All three keep the same progress accessors.
    use unified_archive::StreamBound;
    let mut stream = archive.extract_to_stream(file_path, StreamBound::DeclaredSize)?;
    let mut output_file = File::create(output)?;

    // Process in 1MB chunks
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                output_file.write_all(&buffer[..n])?;

                // Show progress
                if let Some(progress) = stream.progress() {
                    print!("\rProgress: {:.1}%", progress * 100.0);
                }
            },
            Err(e) => return Err(e.into()),
        }
    }

    println!("\nDone!");
    Ok(())
}
```

### Use Case 6: Check Archive Integrity

```rust
use unified_archive::Archive;

fn verify_archive(archive_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    // Note: Validation mode differs by backend and format. Not all backends
    // perform CRC32 verification; some use hash-based or decompression-based checks.
    println!("Validating archive integrity...");
    let report = archive.validate_integrity()?;

    println!("Total entries: {}", report.total_entries);
    println!("Validated: {}", report.validated);

    if report.failed.is_empty() {
        println!("✅ Archive is intact!");
    } else {
        println!("❌ Failed files:");
        for file in &report.failed {
            println!("  - {}", file);
        }
    }

    Ok(())
}
```

### Use Case 7: Password-Protected Archives

```rust
use unified_archive::{Archive, ArchiveError, ExtractionOptions};

fn extract_encrypted(archive_path: &str, password: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    // Try opening with password
    match Archive::open_encrypted(archive_path, password) {
        Ok(archive) => {
            println!("Archive opened successfully!");

            // List files
            let entries = archive.list_files()?;
            println!("Files: {}", entries.len());

            // Extract to an explicit destination
            let options = ExtractionOptions::new("./output");
            let result = archive.extract_all(options)?;
            for warning in &result.warnings {
                eprintln!("warning: {warning}");
            }
            println!("Extraction complete!");

            Ok(())
        },
        Err(ArchiveError::Password { message }) => {
            eprintln!("Password error: {}", message);
            Err(Box::new(ArchiveError::Password { message }))
        },
        Err(e) => Err(Box::new(e)),
    }
}
```

### Use Case 8: Progress Tracking

```rust
use unified_archive::{Archive, ExtractionOptions, ProgressCallback};
use std::ops::ControlFlow;

struct MyProgress {
    last_percent: u64,
}

impl ProgressCallback for MyProgress {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()> {
        if let Some(total) = total {
            let percent = (processed * 100) / total;

            if percent != self.last_percent {
                println!("Progress: {}% ({} / {} bytes)", percent, processed, total);
                self.last_percent = percent;
            }
        } else {
            println!("Processed: {} bytes", processed);
        }

        // Return Continue to keep going, or Break to cancel
        ControlFlow::Continue(())
    }
}

fn extract_with_progress(archive_path: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    let archive = Archive::open(archive_path)?;

    // `.progress` takes any `impl ProgressCallback` and boxes it for you,
    // so `MyProgress` goes in without `Some` or `Box::new`.
    let options = ExtractionOptions::new("./output")
        .progress(MyProgress { last_percent: 0 });

    let result = archive.extract_all(options)?;
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    Ok(())
}
```

---

## Next Steps

### Learn More

- **[User Manual](./USER_MANUAL.md)** - Installation, workflows, supported formats, and caveats in one place
- **[API Reference](./API_REFERENCE.md)** - API reference covering inspection, extraction, creation, modification, SFX, and streaming
- **[README](../README.md)** - Feature overview and release snapshot
- **[Examples](../examples/)** - Complete working examples
- **[Limitations](../Limitations.md)** - Current unsupported and partially supported cases

### Run the Examples

The repository includes several complete examples:

```bash
# Inspect an archive
cargo run --example inspect_archive -- tests/fixtures/test.zip

# Extract an archive
cargo run --example extract_archive -- tests/fixtures/test.rar ./output

# Streaming extraction (TAR.GZ: the libarchive-backed formats are the ones that
# really stream; ZIP/7z/RAR materialize the entry before handing back the reader)
cargo run --example streaming_extract -- tests/fixtures/test.tar.gz test_file.txt
```

> `streaming_extract` writes the entry under its base name in the **current directory**.
> It stages bytes into a sibling `.part` file and renames it into place only after a clean
> EOF, and it refuses to overwrite an existing file unless you pass `--force` (R0001-0077).
> Run it from a scratch directory so it does not collide with a file of the same name.

### Explore Supported Formats

Try your program with different archive formats:

> **Prerequisite:** The `zip` and `7z` (7-Zip) command-line tools must be installed.
> On macOS: `brew install zip p7zip`. On Debian/Ubuntu: `apt-get install zip p7zip-full`.

```bash
# Create test archives
zip test.zip file.txt
tar czf test.tar.gz file.txt
7z a test.7z file.txt

# List contents
cargo run -- test.zip
cargo run -- test.tar.gz
cargo run -- test.7z
```

### Error Handling

Learn how to handle different error types:

```rust
use unified_archive::{Archive, ArchiveError};

match Archive::open("file.rar") {
    Ok(archive) => {
        // Use archive
        println!("Opened successfully!");
    },
    Err(ArchiveError::Io { source, .. }) => {
        eprintln!("I/O error: {}", source);
    },
    Err(ArchiveError::Format { format, .. }) => {
        eprintln!("Unsupported format: {:?}", format);
    },
    Err(ArchiveError::Password { message }) => {
        eprintln!("Password required: {}", message);
    },
    Err(e) => {
        eprintln!("Error: {}", e);
    },
}
```

### Performance Tips

1. **Use Streaming for Large Files**
   ```rust
   // Good for large files (bounded memory for libarchive-backed formats;
   // other backends such as ZIP, SevenZ, and UnRAR still
   // buffer full entries)
   let stream = archive.extract_to_stream("huge.bin", StreamBound::DeclaredSize)?;

   // Avoid for large files (loads entirely into memory)
   let data = archive.extract_to_memory("huge.bin")?;
   ```

2. **Use Selective Extraction**
   ```rust
   // Single-pass selective extraction (sequential).
   archive.extract_filtered(|e| e.path.ends_with(".jpg"), options)?;
   ```

3. **Reuse File Listings**
   ```rust
   // list_files() is internally cached, so repeated calls are cheap.
   // Still, reuse the returned slice if convenient to avoid the call overhead.
   let entries = archive.list_files()?;

   // Use the returned entries (a `&[ArchiveEntry]` slice)
   for entry in entries {
       // ...
   }
   ```

### Additional Resources

- **[User Manual](./USER_MANUAL.md)** - Complete release-facing guide for v0.4.0
- **[API Reference](./API_REFERENCE.md)** - API reference covering inspection, extraction, creation, modification, SFX, and streaming
- **[Limitations](../Limitations.md)** - Current unsupported and partially supported cases
- **[Examples](../examples/)** - Complete working examples

---
