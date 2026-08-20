---
type: Tutorial
title: Your first archive, created then changed
description: Write a ZIP with Archive::create, reopen it to confirm the entries, then add and remove entries through Archive::modify.
tags: [creation, modification, archive, getting-started]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: creation, resource: src/creation.rs }
  - { id: modification, resource: src/modification.rs }
  - { id: options, resource: src/options.rs }
  - { id: inspection, resource: src/inspection.rs }
  - { id: format-caps, resource: src/format.rs }
  - { id: example-create, resource: examples/create_archive.rs }
synced_hash: bf9925abaf0cc40b43bd66d5201e87e7424e15e9f39d55e847c6e8fe26fd292b
---

# Your first archive, created then changed

In this lesson you build a ZIP archive from scratch, reopen it to see what you
wrote, then change its contents in place and reopen it once more to see the
result. Four steps, one program, one run per step.

Start from the finished state of
[Getting started with unified-archive](../../../tutorials/user/en/getting-started.md):
a Cargo project called `archive-tour` that depends on `unified-archive`, with the
`demo/` directory tree still present. Every command below runs from the project
root.

## Step 1 — Create a ZIP archive

`Archive::create` returns a handle in write mode; entries go in one call at a
time and `finish` writes the central directory. Replace `src/main.rs` with:

```rust
use unified_archive::{Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions};

fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    Ok(())
}

fn create_bundle() -> Result<(), ArchiveError> {
    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("bundle.zip", options)?;
    archive.add_file_from_data("notes/readme.txt", b"created from memory\n")?;
    archive.add_file_from_path_as("demo/notes/todo.txt", "notes/todo.txt")?;
    println!("staged {} entries", archive.entry_count()?);
    archive.finish()?;
    println!("wrote bundle.zip");

    Ok(())
}
```

Run it:

```bash
cargo run
```

```text
staged 2 entries
wrote bundle.zip
```

`entry_count` reports 2 because in write mode it counts the entries the writer
has already emitted, not the entries of a finished archive.

## Step 2 — Reopen it and confirm the entries

Reading back is the only way to know what actually landed in the file. Add a
second function and call it:

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    Ok(())
}

fn show_bundle(label: &str) -> Result<(), ArchiveError> {
    let archive = Archive::open("bundle.zip")?;
    println!("{label}: format {:?}", archive.format());
    for entry in archive.list_files()? {
        println!("  {} ({} bytes)", entry.path, entry.size.unwrap_or(0));
    }
    Ok(())
}
```

`Archive::create` refuses to write over a file that already exists, so delete
last run's output before running again:

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
```

Both entries carry the archive-internal paths you chose, not the filesystem
paths they came from.

## Step 3 — Change the archive in place

`Archive::modify` opens an existing archive in modify mode, queues your edits in
memory, and applies them all at `commit_changes`. Add a third function and call
it:

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    change_bundle()?;
    Ok(())
}

fn change_bundle() -> Result<(), ArchiveError> {
    let mut archive = Archive::modify("bundle.zip")?;
    archive.add_entry("notes/changelog.txt", b"added by modify\n")?;
    let removed = archive.remove_entry("notes/todo.txt")?;
    println!("queued 1 add and {removed} removal");
    archive.commit_changes()?;
    println!("committed");
    Ok(())
}
```

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
queued 1 add and 1 removal
committed
```

`remove_entry` returns how many entries in the source listing matched the path
you named — `1` here, and `0` if the path is not in the archive at all.

## Step 4 — Reopen it once more

No new code is needed; call `show_bundle` a second time.

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    change_bundle()?;
    show_bundle("after commit")?;
    Ok(())
}
```

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
queued 1 add and 1 removal
committed
after commit: format Zip
  notes/readme.txt (20 bytes)
  notes/changelog.txt (16 bytes)
```

`notes/todo.txt` is gone, `notes/changelog.txt` is present, and the surviving
entry keeps its position ahead of the new one: a commit copies the retained
entries in their original order first, then appends what you added.

## Two rules to remember

- **Delete `bundle.zip` before each run.** `Archive::create` never writes over a
  file that already exists, so every run above started with `rm -f bundle.zip`.
  The exact error it raises is in
  [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).
- **Only ZIP and 7z can be modified.** `Archive::modify` refuses every other
  format without rewriting anything;
  [Format support matrix](../../../reference/user/en/format-support-matrix.md)
  lists what each format allows.

## Where to go next

- [How to create an archive in a given format](../../../how-to/user/en/create-an-archive.md)
  — the same task for TAR, 7z, and the compressed tarballs.
- [How to add, replace, or remove entries in an existing archive](../../../how-to/user/en/modify-a-zip-or-7z.md)
  — replacement, backups, and the rest of the modify surface.
- [Options and defaults](../../../reference/user/en/options-and-defaults.md)
  — every field of `CompressionOptions`, `ExtractionOptions`, and
  `ModificationOptions`, with its default.
- [Why modification rewrites the archive](../../../explanation/developer/en/modification-is-a-rewrite.md)
  — what a commit preserves, what it drops, and why it works this way.
