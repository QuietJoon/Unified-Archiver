---
type: Tutorial
title: Getting started with unified-archive
description: Install the native prerequisites, then write one program that opens a ZIP archive, lists its entries, and extracts it.
tags: [getting-started, archive, extraction, build]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: readme, resource: README.md }
  - { id: manifest, resource: Cargo.toml }
  - { id: build-script, resource: build.rs }
  - { id: archive-facade, resource: src/archive.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: extraction, resource: src/extraction.rs }
  - { id: options, resource: src/options.rs }
synced_hash: 72c1fc8cf30d25d255006a2b6e9f45c7da92f1433d8a2679b4a8f7d776247c04
---

# Getting started with unified-archive

By the end of this lesson you will have a small Rust program that opens a real
ZIP archive, prints its format and entry list, and extracts it to a directory on
disk. Everything is one linear path — type it in the order shown and it works.

This lesson runs on Ubuntu or Debian Linux. On Fedora, macOS, or Windows,
install the same prerequisites the way
[How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md)
describes for your system, then rejoin the lesson at Step 1 — every later
command is identical.

## What you need

- **Rust 1.85 or newer.** The crate declares `rust-version = "1.85"` and edition
  2024 in `Cargo.toml`, so an older toolchain refuses to compile it. Check with
  `rustc --version`.
- **libarchive and `pkg-config`.** On every non-Windows target the build script
  discovers libarchive through `pkg-config` and fails the build loudly if it
  cannot find it.
- **A C++ compiler.** The default feature set includes `rar-support`, and
  `build.rs` compiles the vendored UnRAR C++ sources through the `cc` crate as
  part of the build.
- **The system `zip` tool**, which Step 2 uses to build the sample archive.

One command installs everything except Rust itself:

```bash
sudo apt-get install libarchive-dev pkg-config g++ zip
```

## Step 1 — Start a project and add the dependency

```bash
cargo new archive-tour
cd archive-tour
```

Add the crate to `Cargo.toml`:

```toml
[dependencies]
unified-archive = "0.4.0"
```

> **Not on crates.io.** No version of this crate has been published to a public registry, so
> the line above will not resolve as written. Until it is published, depend on the crate by
> `path` or `git`.

## Step 2 — Make an archive to work with

Build a small directory tree and pack it with the system `zip` tool, so the
archive you inspect was not produced by the library you are learning.

```bash
mkdir -p demo/notes
printf 'hello from unified-archive\n' > demo/readme.txt
printf 'second file\n' > demo/notes/todo.txt
zip -r sample.zip demo
```

`zip` prints one line per stored entry, in the order it walked the tree:

```text
  adding: demo/ (stored 0%)
  adding: demo/notes/ (stored 0%)
  adding: demo/notes/todo.txt (stored 0%)
  adding: demo/readme.txt (stored 0%)
```

## Step 3 — Open the archive and list it

`Archive::open` sniffs the format from the file's magic bytes, so nothing in
this program names ZIP anywhere. Put this in `src/main.rs`:

```rust
use std::env;
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    let path = env::args().nth(1).expect("usage: archive-tour <archive>");

    let archive = Archive::open(&path)?;
    println!("Format:  {:?}", archive.format());
    println!("Entries: {}", archive.entry_count()?);

    for entry in archive.list_files()? {
        let size = match entry.size {
            Some(bytes) => bytes.to_string(),
            None => "-".to_string(),
        };
        println!("{:<24} {:>8}", entry.path, size);
    }

    Ok(())
}
```

Run it:

```bash
cargo run -- sample.zip
```

After Cargo's own build lines, the program prints:

```text
Format:  Zip
Entries: 4
demo/                           -
demo/notes/                     -
demo/notes/todo.txt            12
demo/readme.txt                27
```

The two directory entries show `-` because `ArchiveEntry::size` is `None` for
directories. The order is the order `zip` printed in Step 2, because
`Archive::list_files` reports entries in archive order and does not sort.

## Step 4 — Extract it

`ExtractionOptions::default` fills in every field except the one you care about,
so a destination plus `..Default::default()` is a complete configuration. Extend
`src/main.rs` — the new lines go after the listing loop, and the import list
grows:

```rust
use std::env;
use std::path::PathBuf;
use unified_archive::{Archive, ArchiveError, ExtractionOptions};

fn main() -> Result<(), ArchiveError> {
    let path = env::args().nth(1).expect("usage: archive-tour <archive>");

    let archive = Archive::open(&path)?;
    println!("Format:  {:?}", archive.format());
    println!("Entries: {}", archive.entry_count()?);

    for entry in archive.list_files()? {
        let size = match entry.size {
            Some(bytes) => bytes.to_string(),
            None => "-".to_string(),
        };
        println!("{:<24} {:>8}", entry.path, size);
    }

    let options = ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    };
    let result = archive.extract_all(options)?;

    println!("Warnings: {}", result.warnings.len());
    for warning in &result.warnings {
        println!("  {warning}");
    }

    Ok(())
}
```

Run it once — `overwrite` defaults to `false`, so a second run into the same
`out/` directory is refused rather than silently replacing files.

```bash
cargo run -- sample.zip
```

```text
Format:  Zip
Entries: 4
demo/                           -
demo/notes/                     -
demo/notes/todo.txt            12
demo/readme.txt                27
Warnings: 0
```

`extract_all` creates `out/` for you and returns a `ResultWithWarnings`, whose
`warnings` vector holds the `ArchiveWarning` values for entries the extractor
declined to materialise — skipped symbolic links, skipped hard links, and output
paths that would collide on a case-insensitive filesystem. This archive has
none, hence `Warnings: 0`.

Confirm the files landed:

```bash
find out -type f | sort
```

```text
out/demo/notes/todo.txt
out/demo/readme.txt
```

You now have a working program that inspects and extracts any format the crate
supports.

## Where to go next

- [Your first archive, created then changed](../../../tutorials/user/en/your-first-archive.md)
  — the same lesson shape, but writing archives instead of reading them.
- [How to pick the right extraction call](../../../how-to/user/en/choose-an-extraction-api.md)
  — `extract_all` is one of several; this page maps goals to calls.
- [How to extract an untrusted archive safely](../../../how-to/user/en/extract-untrusted-archives-safely.md)
  — what to change before you point this program at a file you did not create.
- [How to create an archive in a given format](../../../how-to/user/en/create-an-archive.md)
  — the writing side, as a recipe.
- [Format support matrix](../../../reference/user/en/format-support-matrix.md)
  — which formats can be read, created, and modified.
- [One API over many backends](../../../explanation/user/en/one-api-many-backends.md)
  — why `Archive::open` can behave identically across formats that share no code.
