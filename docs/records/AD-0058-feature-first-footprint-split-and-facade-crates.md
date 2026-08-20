---
type: ADR
title: "AD 0058: Feature-first footprint split and facade crates"
description: "Planned"
tags: [decision, ADR-0058]
timestamp: 2026-04-25T00:00:00Z
status: active
---

# AD 0058: Feature-first footprint split and facade crates

Status: Planned

## Plan

Split the crate footprint in two stages. Stage 1 is feature-first inside the
current crate. Stage 2 introduces read-only and full facade crates only after
the feature matrix is stable.

### Stage 1: Functional features

Define operation-level features before changing package layout:

| Feature | Scope |
|---|---|
| `read` | Open archives, list entries, extract entries, stream entries, format detection needed for read paths. |
| `integrity` | Payload validation and checksum verification. Depends on `read`. |
| `create` | New archive creation APIs and writer backends. Depends on the required format writer features. |
| `modify` | Copy-on-write archive modification APIs. Depends on `read` and `create`. |
| `full` | Compatibility aggregate for read, integrity, create, modify, and all supported formats. |

Migration steps:

1. Add the functional feature names while keeping the current default behavior
   equivalent to today's crate.
2. Gate public modules, re-exports, examples, and tests by operation.
3. Replace runtime `write_mode_only` / unsupported-mode paths with compile-time
   absence where the operation feature is disabled.
4. Document read-only usage through `default-features = false` plus explicit
   read and format features.

### Stage 1: Format features

Define format/backend features orthogonally to operation features:

| Feature | Scope |
|---|---|
| `zip-read` | ZIP inspection and extraction through the read backend. |
| `zip-write` | ZIP creation support. Depends on `create`. |
| `zip-crypto` | Encrypted ZIP read support. Depends on `zip-read`. |
| `sevenzip` | 7z read/extract support, and 7z creation where supported by the selected writer path. |
| `rar` | RAR/RAR5 read/extract support and vendored UnRAR build. |
| `libarchive` | TAR family, raw compressed single-file formats, ISO, and libarchive-backed creation. |
| `sfx` | Self-extracting archive detection and offset opening. Depends on read-capable format features. |

Migration steps:

1. Mark backend dependencies optional in `Cargo.toml`.
2. Gate backend modules and `ArchiveFormat` routing by format feature.
3. Gate `build.rs` probes and native builds so `libarchive` and `rar` are not
   touched unless their features are enabled.
4. Add targeted CI/test profiles:
   `read-minimal`, `read-zip`, `read-all-formats`, `create`, `modify`, and
   `full`.
5. Measure dependency tree, build time, and binary size for `read-minimal` and
   `full` before changing defaults.

### Stage 2: Facade crates

Introduce package-level facades only after Stage 1 proves the feature split:

| Crate | Role |
|---|---|
| `unified-archive-core` | Internal implementation crate with the full feature matrix. |
| `unified-archive-read` | Small public facade for read-only consumers. Depends on core with `read` plus selected default read formats. |
| `unified-archive` | Consolidated public facade for the full API. Depends on core with `full`; preserves the existing crate name for compatibility. |

Migration steps:

1. Move implementation into `unified-archive-core` without changing public API
   names.
2. Make `unified-archive` a full facade re-exporting core types and preserving
   current examples.
3. Add `unified-archive-read` with only `ArchiveReader` / read-side exports.
4. Keep read-only facade documentation separate from full facade documentation
   so small-footprint users do not need to learn create/modify concepts.
5. Publish the facade split in a minor pre-1.0 release with explicit migration
   notes for users who previously relied on default features.

### Ordering constraints

1. Do not create facade crates before optional dependencies and feature gates
   are complete.
2. Do not flip default features until `read-minimal` and `full` profiles have
   CI coverage.
3. Keep `integrity` as a read-side extension, not a separate crate, because it
   reuses archive decoding and backend traversal.
4. Keep the full facade as the compatibility path; optimize the read-only path
   through feature selection first, package layout second.
