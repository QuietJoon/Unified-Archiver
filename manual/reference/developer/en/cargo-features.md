---
type: Reference
title: Cargo features and MSRV
description: Every Cargo feature of unified-archive with its platform gate, licence consequence, and disabled behaviour, plus the crate's edition, MSRV, and dependency set.
tags: [build, api, platform]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: manifest, resource: Cargo.toml }
  - { id: crate-root, resource: src/lib.rs }
  - { id: facade, resource: src/archive.rs }
  - { id: external-rar, resource: src/external/rar.rs }
  - { id: external-mod, resource: src/external.rs }
  - { id: build-script, resource: build.rs }
  - { id: api-reference, resource: docs/API_REFERENCE.md }
synced_hash: f40ecb0baab38ec1bf3ee3b50a3f480e75faa89b2e82c7d7cf57ed35cc4bc0fa
---

# Cargo features and MSRV

## Package identity

All ten keys the manifest's `[package]` table declares, in manifest order:

| Manifest key | Value |
|---|---|
| `name` | `unified-archive` |
| `version` | `0.4.0` |
| `edition` | `2024` |
| `rust-version` | `1.85` |
| `authors` | `YongJoon Joe <developer@yongjoon.net>` |
| `license` | `MIT` |
| `description` | `Unified archive library for Rust — read and extract ZIP/7z/RAR/TAR (gz/bz2/xz/zst/lz4/lzma)/ISO and standalone .gz/.bz2/.xz/.zst/.lz4/.lzma; create ZIP/7z/TAR (gz/bz2/xz/zst/lz4/lzma) and modify ZIP/7z. RAR is read-only via the main facade (creation requires the optional external-rar-create feature on Windows).` |
| `repository` | `https://github.com/QuietJoon/unified-archive` |
| `keywords` | `archive`, `7zip`, `compression`, `extraction`, `unified` |
| `categories` | `compression`, `filesystem` |

The `description` string is the manifest's own text. Its creation list agrees with
`ArchiveFormat::can_create`: TAR.ZST, TAR.LZ4, and TAR.LZMA creation landed on 2026-08-04 and
the description was brought into step on 2026-08-06.

The manifest declares no `[[bin]]` target and no `[lints]` table. The repository
contains no `rust-toolchain` / `rust-toolchain.toml` file, so the toolchain used is
whichever one invokes Cargo.

## Edition and MSRV

`edition = "2024"` and `rust-version = "1.85"`. Edition 2024 requires every `extern`
block to be written `unsafe extern`, and requires explicit `unsafe` blocks inside
`unsafe fn` bodies; the FFI declarations in `src/ffi/libarchive.rs` and
`src/ffi/unrar.rs` are written in that form. `rust-version` is a Cargo-level floor: a
toolchain older than 1.85 is rejected by Cargo before compilation, and edition 2024
itself is unavailable before 1.85.

No feature raises or lowers the MSRV — no feature adds a dependency (see
[Feature declarations](#feature-declarations)).

## Feature declarations

The manifest's `[features]` table has exactly four entries:

| Feature | In default set | Declared value | Compiled when |
|---|---|---|---|
| `default` | — | `["rar-support"]` | always, unless the consumer sets `default-features = false` |
| `rar-support` | yes | `[]` | the feature is enabled, on every target |
| `external-rar-create` | no | `[]` | the feature is enabled **and** `target_os = "windows"` |
| `v2-api` | no | `[]` | the feature is enabled, on every target |

Every feature other than `default` has an empty value list. No feature enables an
optional dependency,
because the manifest declares no optional dependencies: the crate graph is identical
for every feature combination. A feature switches `cfg` compilation of first-party
code, and — for `rar-support` only — one step of the build script.

### `rar-support` (in the default set)

Turns on:

- The vendored UnRAR build in `build.rs`. The `build_unrar` step is compiled under
  `#[cfg(feature = "rar-support")]`; it compiles the C++ sources under
  `src/ffi/native/unrar` with the `cc` crate into a static `unrar` library in
  `OUT_DIR` and emits `cargo:rustc-link-search=native=<OUT_DIR>` plus
  `cargo:rustc-link-lib=static=unrar`. The gate is the feature only, not the host, so
  the step runs for Windows, macOS, and Linux targets alike.
- The FFI modules `crate::ffi::unrar` (raw bindings) and `crate::ffi::wrapper`
  (`UnrarArchive`), both declared under the feature in `src/ffi.rs`.
- The internal `ArchiveBackend::Unrar` variant in `src/archive.rs`, the
  `ReadBackend` implementation for `UnrarArchive` in `src/backend.rs`, and the RAR
  arms of the facade dispatch ladders: `Archive::open`/`open_as_format`,
  `Archive::open_encrypted`, `is_solid`, `has_recovery_record`,
  `recovery_percentage`, the single-file extraction path in `src/extraction.rs`, and
  the write-finalize ladder.

Platform gate: none.

Dependencies: no crate is added. A build-time C++ toolchain is required for the
vendored sources; the details are in
[Build environment](../../../reference/operator/en/build-environment.md).

Licence consequence: the crate itself is MIT (`LICENSE`). The vendored UnRAR sources
carry the UnRAR licence (`src/ffi/native/unrar/license.txt`), whose terms state that
the source may be used in any software to handle RAR archives free of charge, but may
not be used to develop a RAR (WinRAR) compatible archiver or to re-create the RAR
compression algorithm, and that distribution of the source — separately or as part of
other software — must reproduce that paragraph in the licence, in the documentation,
and in source comments. Building with `rar-support` statically links object code
derived from those sources into the consuming binary.

With the feature off:

- No UnRAR source is compiled and no `unrar` library is linked.
- `Archive::open` (through `open_as_format`) and `Archive::open_encrypted` return
  `ArchiveError::unsupported` for `ArchiveFormat::Rar` and `ArchiveFormat::Rar5`,
  with the reason ``RAR/RAR5 support is disabled in this build (enable the
  `rar-support` Cargo feature to include UnRAR)``. These two arms are the only
  `cfg(not(feature = "rar-support"))` blocks in the crate.
- Nothing else changes shape. `ArchiveFormat::Rar` / `ArchiveFormat::Rar5` still
  exist, magic-byte and extension detection in `src/format.rs` still identify RAR,
  and SFX detection in `src/sfx.rs` still classifies RAR payloads — none of those
  modules is feature-gated. The refusal happens when the archive is opened.

### `external-rar-create`

Turns on the `external` module and its single public type, both gated
`#[cfg(all(target_os = "windows", feature = "external-rar-create"))]` in `src/lib.rs`
and again in `src/external.rs`:

- `unified_archive::external::RarCreator` (`src/external/rar.rs`), which creates RAR
  archives by spawning the WinRAR command-line tool. Constructors `RarCreator::new`
  (locates `rar.exe` first, then rejects an existing output path) and
  `RarCreator::with_rar_exe_path` (bypasses discovery, requires the given path to be
  a regular file). Methods `set_compression_level`, `set_password`, `add_file`,
  `add_directory`, `entry_count`, `rar_exe_path`, and the consuming `create`.
- `rar.exe` discovery order: `where rar.exe` against the current `PATH`, then
  `C:\Program Files\WinRAR\rar.exe`, then `C:\Program Files (x86)\WinRAR\rar.exe`.
  Custom and portable installs are not auto-detected.
- The spawned argument vector is `a`, the `-mN` level flag (`Store` → `-m0`,
  `Fastest` → `-m1`, `Fast` → `-m2`, `Normal` → `-m3`, `Maximum` → `-m4`,
  `Ultra` → `-m5`), `-hp<password>` when a password is set, `-r`, the `--`
  end-of-switches sentinel, the output path, then the entry paths, with any
  leading-dash entry re-rooted under `.`.

Platform gate: Windows targets only. Enabling the feature for any other target
compiles nothing — `unified_archive::external` does not exist there.

Dependencies: no crate is added.

Licence consequence: the crate ships no RAR compression code. Creating RAR archives
through this path requires a licensed WinRAR installation on the machine that runs
the program, subject to WinRAR's own licence terms.

Security consequence recorded in the source: with a password set, `-hp<password>` is
part of the child process's argument vector and is therefore visible to anything that
can list running processes for as long as `rar.exe` lives. The `Password` type only
protects the value inside this process's address space.

This surface is independent of `Archive::create`, which never produces RAR and which
rejects password-protected creation for every format.

With the feature off (or on a non-Windows target): `unified_archive::external` does
not exist, and the crate spawns no external process.

### `v2-api`

Turns on the typed-handle module:

- `unified_archive::v2`, declared `#[cfg(feature = "v2-api")]` in `src/lib.rs`,
  re-exporting `ModifyArchive`, `ReadArchive`, and `WriteArchive` from
  `crate::archive::mode_split`.
- `crate::archive::mode_split` itself (`src/archive/mode_split.rs`), gated at the
  module declaration and again by an inner `#![cfg(feature = "v2-api")]`.
- A `pub(crate)` `Archive::finalize_write_on_drop`, used by `WriteArchive`'s `Drop`
  so that a `WriteArchive` dropped without `finish` is finalized exactly once and the
  inner `Archive`'s own `Drop` does not finalize or warn a second time.

The three types wrap an inner `Archive` and delegate to it; they narrow the operation
set by mode at compile time rather than changing behaviour. `WriteArchive::finish`
consumes the handle. The method-by-method surface is in
[Public API surface](../../../reference/user/en/public-api-surface.md).

Platform gate: none. Dependencies: none added. Licence consequence: none.

Version status: additive in 0.4.0 — the legacy `Archive` facade is unchanged whether
the feature is on or off. Both the manifest comment and the rustdoc on
`unified_archive::v2` record that 0.4 will enable it by default.

With the feature off: `unified_archive::v2` does not exist and `mode_split` is not
compiled.

### Effect of `default-features = false`

The default set contains `rar-support` only. A dependency declared with
`default-features = false` and no explicit feature list compiles the crate with all
three features off, and with the full dependency set below still compiled.

## Dependency set

### Always compiled

These are declared unconditionally in `[dependencies]`; no feature adds or removes
any of them.

| Crate | Version requirement | Declared features | Role in the crate |
|---|---|---|---|
| `once_cell` | `1.20` | default | `OnceCell` for lazily initialised per-handle state (the ZIP listing cache, the modification entry cache, and similar memoised metadata) |
| `crc32fast` | `1.4` | default | CRC32 computation and verification |
| `secstr` | `0.5` | default | backing store for `Password` |
| `walkdir` | `2.4` | default | directory traversal for `add_directory_recursive` |
| `sevenz-rust2` | `0.19` | default | 7z read/extract/create backend |
| `zip` | `2.2` | `default-features = false`, `deflate`, `aes-crypto` | sole ZIP reader, extractor, and creator |
| `tempfile` | `3.0` | default | atomic output files and modification temp paths |
| `fs4` | `1` | default | advisory file locking for Modify mode |

The `zip` feature selection is fixed by this crate, not by the consumer: the ZIP
backend is built with `deflate` and `aes-crypto` and without `zip`'s other optional
codecs.

### Target-gated

| Crate | Target predicate | Version | Reason recorded in the manifest |
|---|---|---|---|
| `libc` | `cfg(windows)` | `0.2` | the Windows arm of the libarchive write constructor (in `src/ffi/libarchive_wrapper/writer.rs`; the manifest comment names the parent module `libarchive_wrapper.rs`) calls `libc::_open_osfhandle` and `libc::_close`. The dependency is declared so a Windows-target compile probe does not fail with `E0432`, even though that branch is not exercised by default CI while the Windows libarchive build stays deferred |

### Build dependencies

Used only by `build.rs`, never linked into the library:

| Crate | Version | Use |
|---|---|---|
| `pkg-config` | `0.3` | `probe_library("libarchive")` on non-Windows targets |
| `cc` | `1` | compiling the vendored UnRAR C++ sources under `rar-support` |

### Dev dependencies

Compiled only when building the source tree's own tests, benches, and examples; not
compiled for consumers of the published crate:

| Crate | Version | Declared features |
|---|---|---|
| `criterion` | `0.5` | default |
| `proptest` | `1.4` | default |
| `serial_test` | `3` | `file_locks` |
| `zip` | `2.2` | `default-features = false`, `deflate` |

`zip` appears in both `[dependencies]` and `[dev-dependencies]`; the dev entry exists
for integration tests that inspect ZIP metadata directly.

### Native libraries

Neither libarchive nor UnRAR is a Cargo dependency. libarchive is a system library
located and linked by the build script; UnRAR is compiled from vendored sources under
`rar-support`. Both are described in
[Build environment](../../../reference/operator/en/build-environment.md), and the
system requirements are listed in
[How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md).

## Manifest targets

- Library: the crate root is `src/lib.rs`. There is no binary target, so the crate is
  consumed as a library.
- Examples: four `[[example]]` sections are declared explicitly —
  `inspect_archive`, `extract_archive`, `create_archive`, `modify_archive`. The
  `examples/` directory also contains `archive_crc.rs`, `detect_sfx.rs`,
  `stream_checksum.rs`, and `streaming_extract.rs`, which Cargo's target
  auto-discovery picks up because the manifest does not set `autoexamples = false`.
- Benches: three `[[bench]]` sections, each with `harness = false` —
  `archive_operations`, `integrity_validation_bench`, `sfx_detection`. Their sources
  are `benches/archive_operations.rs`, `benches/integrity_validation_bench.rs`, and
  `benches/sfx_detection.rs`.

## Format coverage by feature

RAR and RAR5 read and extract require `rar-support`. RAR creation is never available
through `Archive::create` and exists only through `external::RarCreator` on Windows
under `external-rar-create`. Every other supported format — ZIP, 7z, the TAR family,
the standalone compressed streams, and ISO — is available in every feature
combination, subject to the native libarchive the build links against. The per-format
detail is in
[Format support matrix](../../../reference/user/en/format-support-matrix.md).
