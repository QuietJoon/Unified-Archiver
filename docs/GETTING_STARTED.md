# Getting Started with unified-archive

This guide will walk you through installing and using unified-archive for the first time.

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
brew install libarchive
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install libarchive-dev
```

**Linux (Fedora/RHEL):**
```bash
sudo dnf install libarchive-devel
```

**Windows:**
libarchive is bundled automatically (no action needed)

### Add to Your Project

Add unified-archive to your `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.1.0"
```

### Verify Installation

Create a simple test program:

```rust
// src/main.rs
use unified_archive::Archive;

fn main() {
    println!("unified-archive installed successfully!");
}
```

Run it:
```bash
cargo run
```

If you see the success message, you're ready to go!

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
unified-archive = "0.1.0"
```

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

    for entry in &entries {
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

```bash
# Create a simple ZIP for testing
echo "Hello, World!" > test.txt
zip test.zip test.txt
```

Run your program:

```bash
cargo run test.zip
```

You should see output like:
```
Opening: test.zip
Format: Zip

Contents:
  test.txt - 14 bytes (CRC32: 0BF71E59)

Total files: 1
```

**Congratulations!** You've just created your first unified-archive program!

---

## Common Use Cases

### Use Case 1: Extract All Files

```rust
use unified_archive::{Archive, ExtractionOptions};
use std::path::PathBuf;

fn extract_archive(archive_path: &str, output_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    let options = ExtractionOptions {
        destination: PathBuf::from(output_dir),
        preserve_permissions: true,
        preserve_times: true,
        ..Default::default()
    };

    println!("Extracting to {}...", output_dir);
    archive.extract_all(options)?;
    println!("Done!");

    Ok(())
}

// Usage:
// extract_archive("backup.zip", "./output")?;
```

### Use Case 2: Extract Specific Files

```rust
use unified_archive::Archive;
use std::path::PathBuf;

fn extract_readme(archive_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    // Extract a single file
    archive.extract_file("README.md", &PathBuf::from("./"))?;

    println!("README.md extracted!");
    Ok(())
}
```

### Use Case 3: Extract Files by Pattern

```rust
use unified_archive::{Archive, ExtractionOptions};
use std::path::PathBuf;

fn extract_images(archive_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Archive::open(archive_path)?;

    let options = ExtractionOptions {
        destination: PathBuf::from("./images"),
        ..Default::default()
    };

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

    // Use streaming to avoid loading entire file into memory
    let mut stream = archive.extract_to_stream(file_path)?;
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
use unified_archive::{Archive, ArchiveError};

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

            // Extract
            archive.extract_all(Default::default())?;
            println!("Extraction complete!");

            Ok(())
        },
        Err(ArchiveError::Password { details }) => {
            eprintln!("Password error: {}", details);
            Err(Box::new(ArchiveError::Password { details }))
        },
        Err(e) => Err(Box::new(e)),
    }
}
```

### Use Case 8: Progress Tracking

```rust
use unified_archive::{Archive, ExtractionOptions, ProgressCallback};
use std::path::PathBuf;
use std::ops::ControlFlow;

struct MyProgress {
    last_percent: u64,
}

impl ProgressCallback for MyProgress {
    fn on_progress(&mut self, current: u64, total: u64) -> ControlFlow<()> {
        let percent = (current * 100) / total;

        if percent != self.last_percent {
            println!("Progress: {}% ({} / {} bytes)", percent, current, total);
            self.last_percent = percent;
        }

        // Return Continue to keep going, or Break to cancel
        ControlFlow::Continue(())
    }
}

fn extract_with_progress(archive_path: &str)
    -> Result<(), Box<dyn std::error::Error>>
{
    let archive = Archive::open(archive_path)?;

    let mut progress = Box::new(MyProgress { last_percent: 0 })
        as Box<dyn ProgressCallback>;

    let options = ExtractionOptions {
        destination: PathBuf::from("./output"),
        progress_callback: Some(&mut progress),
        ..Default::default()
    };

    archive.extract_all(options)?;

    Ok(())
}
```

---

## Next Steps

### Learn More

- **[API Reference](./API_REFERENCE.md)** - Complete API documentation
- **[README](../README.md)** - Feature overview and examples
- **[Examples](../examples/)** - Complete working examples
- **[Limitations](../Limitations.md)** - Known limitations and workarounds

### Run the Examples

The repository includes several complete examples:

```bash
# Inspect an archive
cargo run --example inspect_archive tests/fixtures/test.zip

# Extract an archive
cargo run --example extract_archive tests/fixtures/test.rar ./output

# Streaming extraction
cargo run --example streaming_extract tests/fixtures/test.7z large_file.bin
```

### Explore Supported Formats

Try your program with different archive formats:

```bash
# Create test archives
zip test.zip file.txt
tar czf test.tar.gz file.txt
7z a test.7z file.txt

# List contents
cargo run test.zip
cargo run test.tar.gz
cargo run test.7z
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
    Err(ArchiveError::NotFound { path }) => {
        eprintln!("File not found: {}", path);
    },
    Err(ArchiveError::UnsupportedFormat { format }) => {
        eprintln!("Unsupported format: {:?}", format);
    },
    Err(ArchiveError::Password { details }) => {
        eprintln!("Password required: {}", details);
    },
    Err(e) => {
        eprintln!("Error: {}", e);
    },
}
```

### Performance Tips

1. **Use Streaming for Large Files**
   ```rust
   // Good for large files
   let stream = archive.extract_to_stream("huge.bin")?;

   // Avoid for large files (loads entirely into memory)
   let data = archive.extract_to_memory("huge.bin")?;
   ```

2. **Leverage Parallel Extraction**
   ```rust
   // Automatically parallel for 4+ files
   archive.extract_filtered(|e| e.path.ends_with(".jpg"), options)?;
   ```

3. **Cache File Listings**
   ```rust
   // Cache the result if you need it multiple times
   let entries = archive.list_files()?;

   // Use the cached entries
   for entry in &entries {
       // ...
   }
   ```

### Join the Community

- Report bugs: [GitHub Issues](https://github.com/yourusername/unified-archive/issues)
- Ask questions: [GitHub Discussions](https://github.com/yourusername/unified-archive/discussions)
- Contribute: See [CONTRIBUTING.md](../CONTRIBUTING.md)

---

**Happy Archiving!** 🦀
