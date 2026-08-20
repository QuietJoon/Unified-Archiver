---
type: Overview
title: "unified-archive Documentation"
description: "Public documentation for unified-archive v0.4.0."
tags: [reference, index]
timestamp: 2026-04-30T00:00:00Z
status: active
---

# unified-archive Documentation

Public documentation for `unified-archive` `v0.4.0`.

If you are evaluating or using the library, start here:

- [User Manual](./USER_MANUAL.md) - End-user guide covering installation, common workflows, supported formats, and caveats
- [Getting Started](./GETTING_STARTED.md) - Fast path from `Cargo.toml` to a working program
- [API Reference](./API_REFERENCE.md) - Public API surface and behavior notes
- [Limitations](../Limitations.md) - Current unsupported or partially supported cases
- [Changelog](../CHANGELOG.md) - Release history and release notes

## Recommended Reading Order

1. [README](../README.md) for the project overview and release snapshot
2. [User Manual](./USER_MANUAL.md) for practical usage and format-specific expectations
3. [Getting Started](./GETTING_STARTED.md) for the first program and quick recipes
4. [API Reference](./API_REFERENCE.md) when you need exact method-level behavior

## Examples

The repository ships runnable examples in [`examples/`](../examples/):

- `inspect_archive.rs` - list entries and metadata
- `extract_archive.rs` - extract to disk with options
- `streaming_extract.rs` - incremental reads for large entries; stages to a sibling
  `.part` file and renames it into place only after a clean EOF, and refuses to
  overwrite an existing destination unless `--force` is passed
- `create_archive.rs` - create ZIP / 7z / TAR-family archives
- `modify_archive.rs` - queue and commit ZIP / 7z modifications
- `detect_sfx.rs` - detect and open self-extracting archives
- `archive_crc.rs` - archive-level checksum: the wrapping sum of the listing's
  per-entry CRC32s, plus an opt-in `--digest` cross-format content-multiset digest
  that streams CRC-less entries
- `stream_checksum.rs` - raw stream checksum extraction helpers

## Public Scope

The files above are the release-facing docs for library users.

The following directories are primarily maintainer / design material and are not the best entry point for new users:

- [`docs/architecture/`](./architecture/)
- [`docs/records/`](./records/)
- [`docs/project/`](./project/)
- [`docs/implementation/`](./implementation/)
- [`specs/`](../specs/)

They are useful when you need design rationale, verification details, or implementation notes, but they intentionally contain more internal context than the user-facing guides.
