---
type: How-To Guide
title: How to satisfy the native build dependencies
description: Per-platform recipes for libarchive, pkg-config, and the C++ toolchain, plus how to check the libarchive write filters and how to drop RAR support.
tags: [build, platform, formats, OI-0065-001]
audience: operator
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: build-script, resource: build.rs }
  - { id: manifest, resource: Cargo.toml }
  - { id: readme, resource: README.md }
  - { id: user-manual, resource: docs/USER_MANUAL.md }
  - { id: libarchive-writer, resource: src/ffi/libarchive_wrapper/writer.rs }
  - { id: open-issues, resource: docs/project/open-issues.md }
synced_hash: a988b99a3d918ffaba98a17273bbffe3e3e2c5d9de112a233a89c49280793025
---

# How to satisfy the native build dependencies

`unified-archive` 0.4.0 links two pieces of native code: the system libarchive, and a vendored
UnRAR C++ source tree that `build.rs` compiles in place. This page gives the per-platform recipes
and the two checks that catch the failures people actually hit.

## Preconditions that hold on every platform

- Rust 1.85 or newer (the crate is on edition 2024).
- libarchive is optional as of AD-0058 Stage 1, but it is on by default. `src/ffi.rs` declares
  its module behind `#[cfg(feature = "libarchive")]`, and `build.rs` runs its whole pkg-config
  and Homebrew probe only when `CARGO_FEATURE_LIBARCHIVE` is set, so a build that leaves the
  `libarchive` feature out neither links `archive` nor needs it installed. Any build that keeps
  the feature — which includes every default build — does require it.
- A C++ compiler is needed whenever the `rar-support` feature is on, which it is by default.
- You do **not** need `make`. `build.rs` compiles the vendored UnRAR sources through the `cc`
  crate, which drives your C++ compiler directly.

## Debian and Ubuntu

```bash
sudo apt-get update
sudo apt-get install libarchive-dev pkg-config g++
```

`build.rs` resolves libarchive through `pkg_config::probe_library("libarchive")` on Linux and
panics with an install hint if that fails, so confirm the `.pc` file is visible before you build:

```bash
pkg-config --modversion libarchive
```

If libarchive lives outside the default search path, put its `pkgconfig` directory on
`PKG_CONFIG_PATH`. The pkg-config crate runs with env metadata enabled, so cargo will re-run the
build script when `PKG_CONFIG_PATH`, `PKG_CONFIG_LIBDIR`, `PKG_CONFIG_SYSROOT_DIR`, or the
`LIBARCHIVE_*` overrides change.

## Fedora and RHEL

```bash
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++
```

Then the same `pkg-config --modversion libarchive` check as above. The Linux discovery path in
`build.rs` is identical for both families.

## macOS with Homebrew

```bash
brew install libarchive pkg-config
```

libarchive is keg-only in Homebrew, so `pkg-config` will not see it without help. `build.rs`
handles that case itself: it looks for `lib/libarchive.dylib` under `/opt/homebrew/opt/libarchive`
and then `/usr/local/opt/libarchive`, and if it finds the dylib it emits the link search path and
`archive` link directive directly, printing `unified-archive: using Homebrew libarchive from …`.
So the stock `brew install` layout needs no environment variables.

Two consequences worth knowing:

- The check is for the dylib, not the directory. A stale or partially removed keg that still has
  the prefix directory but no `lib/libarchive.dylib` is deliberately ignored, and the build falls
  through to pkg-config rather than emitting a search path that would produce an obscure linker
  error.
- If your libarchive is somewhere else entirely, that fallback is the route in: put its
  `pkgconfig` directory on `PKG_CONFIG_PATH`. If neither the Homebrew probe nor pkg-config
  succeeds, `build.rs` panics with `libarchive not found. Install it with: brew install
  libarchive`.

## Windows

Windows is not release-verified for 0.4.0 and the repository has no Windows CI job. What follows
is the current state of the build, not a supported configuration.

**The libarchive half is not wired up.** The Windows arm of `build.rs` prints
`cargo:warning=Windows libarchive linking will be configured in future implementation` and emits
no `rustc-link-search` and no `rustc-link-lib` at all. This is tracked as OI-0065-001 in
`docs/project/open-issues.md`, where the discovery mechanism (vcpkg autodetection versus explicit
environment variables versus vendoring) is still an open owner decision.

Because `src/ffi/libarchive.rs` declares `#[link(name = "archive")]`, rustc already asks the
linker for `archive.lib` by name. You only have to supply the search path. Either:

```cmd
vcpkg install libarchive:x64-windows
set RUSTFLAGS=-L native=C:\vcpkg\installed\x64-windows\lib
cargo build
```

or vendor libarchive yourself, put the resulting `archive.lib` in a directory of your own, and
point `RUSTFLAGS` at that directory the same way. Keep `RUSTFLAGS` set for every cargo invocation
in the session — changing it invalidates the build cache, so set it once and leave it.

**The RAR half does build on Windows.** `build.rs` selects its UnRAR source list and preprocessor
defines from `CARGO_CFG_TARGET_OS` and `CARGO_CFG_TARGET_ENV`, and its Windows set mirrors
upstream's `UnRARDll.vcxproj`, so `cc` compiles it with MSVC. The precondition is an MSVC C++
toolchain (Visual Studio Build Tools) rather than `make`.

Dropping `rar-support` is therefore a size and licence choice on Windows, not a workaround for the
libarchive gap. Dropping the separate `libarchive` feature *is* that workaround: it removes the
`#[link(name = "archive")]` block and the `build.rs` probe, at the cost of the TAR family, ISO, the
standalone compressed streams, 7z writing, and `modify`. A bare `--no-default-features` is not a
valid build in either case.

**RAR creation without the SDK.** The optional `external-rar-create` feature is unaffected by any
of the above. It compiles only on a Windows target and shells out to a WinRAR `rar.exe` that you
install and license yourself:

```cmd
cargo build --no-default-features --features read,zip-read,create,zip-write,external-rar-create
```

(`external-rar-create` only implies `create`; a build still needs a backend feature, so the
read/write ZIP pair is named explicitly. This is the same set the `check-windows-cfg` release-gate
lane probes.)

Its preconditions are a licensed WinRAR installation and a discoverable `rar.exe`:
`RarCreator::new` runs `where rar.exe` against `PATH` first, then tries
`C:\Program Files\WinRAR\rar.exe` and `C:\Program Files (x86)\WinRAR\rar.exe`. Portable or custom
installs are not auto-detected — either add the directory to `PATH` or construct the creator with
`RarCreator::with_rar_exe_path`, which bypasses discovery.

## Check whether your libarchive can write zstd, lz4, and lzma

Creating `TAR.ZST`, `TAR.LZ4`, or `TAR.LZMA` needs a libarchive that was built against libzstd,
liblz4, and liblzma respectively. Nothing in `build.rs` verifies this, so check the library you
are actually linking against before you rely on those formats.

The `bsdtar` that ships with libarchive reports its compiled-in libraries:

```bash
# macOS: use the Homebrew keg's copy, which is the one build.rs links
"$(brew --prefix libarchive)/bin/bsdtar" --version

# Linux
bsdtar --version
```

```
bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.12 liblzma/5.8.3 bz2lib/1.0.8 liblz4/1.10.0 libzstd/1.5.7 expat/expat_2.7.4 CommonCrypto/system libb2/system
```

Read it as: `libzstd` present means `TAR.ZST` creation will work, `liblz4` means `TAR.LZ4`, and
`liblzma` covers both `TAR.LZMA` and `TAR.XZ`. A name that is absent is a filter that is not
compiled in.

On macOS, do not run `/usr/bin/tar --version` for this. Apple's copy reports the system
libarchive, which is not the keg `build.rs` links against.

The equivalent check against the shared library itself:

```bash
# macOS
otool -L "$(brew --prefix libarchive)/lib/libarchive.dylib"

# Linux
ldd "$(pkg-config --variable=libdir libarchive)/libarchive.so"
```

Look for `libzstd`, `liblz4`, and `liblzma` in the dependency list.

### What the failure looks like

If the filter is missing, `Archive::create` fails at construction time with
`ArchiveError::Format` carrying the target format and libarchive's own message. It does not fall
back to an external compressor. libarchive registers an external-program fallback and returns
`ARCHIVE_WARN` from `archive_write_add_filter_zstd` (or `_lz4`, or `_lzma`); the create path in
`src/ffi/libarchive_wrapper/writer.rs` treats anything other than `ARCHIVE_OK` as fatal, frees the
handle, and returns the error. So a build with an incomplete libarchive produces no archive at
all rather than one written by a subprocess.

In particular you will not see `ArchiveError::CodecUnavailable`, despite the name. That variant is
never constructed on this path.

Reading those formats is a separate question from writing them; the per-format read and create
columns are in
[Format support matrix](../../../reference/user/en/format-support-matrix.md).

## Drop RAR support for licence reasons

The vendored UnRAR sources under `src/ffi/native/unrar` are RARLAB code, and their licence is in
`src/ffi/native/unrar/license.txt`. It permits the source to "be used in any software to handle
RAR archives without limitations free of charge" — commercial software included — but it "cannot
be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm".
Distributing the source, separately or as part of other software, requires the full text of that
paragraph to be reproduced in your licence (or in your documentation if you ship no licence) and
in the source comments of the resulting package. Building with `rar-support` statically links
object code derived from those sources into your binary, which is what makes that reproduction
requirement yours.

The repository discharges its own copy of that requirement in `LICENSE` (the "Third-party: UnRAR"
section) and in the module comment at the top of `src/ffi/unrar.rs`. If you redistribute, you owe
the same reproduction in your own licence or documentation.

If you would rather not carry the requirement at all, build without the sources:

```bash
cargo build --no-default-features --features read,integrity,create,modify,zip-read,zip-write,zip-crypto,sevenzip,libarchive,sfx,v2-api
```

That is the default (`full`) set with `rar-support` removed; a bare `--no-default-features` is not
a valid build, because `src/lib.rs` raises a `compile_error!` unless at least one backend feature
is named.

What changes:

- `build.rs` skips `build_unrar` entirely, so no C++ sources are compiled and no `unrar` static
  library enters the link. The C++ compiler is then only needed if something else in your
  dependency tree wants one.
- `src/ffi/unrar.rs` and `src/ffi/wrapper.rs` are not compiled.
- `Archive::open` on a RAR or RAR5 file returns `ArchiveError::Unsupported` stating that RAR/RAR5
  support is disabled in this build.
- libarchive is still required *because the list above keeps the `libarchive` feature on*, and
  every other format is unchanged. Drop `libarchive` from the list too and you no longer need it
  installed at all — at the cost of the TAR family, ISO, the standalone compressed streams, 7z
  *writing*, and `modify` (AD-0071 binds modification to libarchive for every format).

On Windows you can keep RAR *creation* while dropping the SDK, by adding
`--features external-rar-create` as shown above: that path contains no RAR compression code and
defers the licence question to the WinRAR installation you provide.

## Where to look next

- The complete listing of tools, libraries, environment variables, and what `build.rs` emits on
  each platform: [Build environment](../../../reference/operator/en/build-environment.md).
- What each Cargo feature turns on, and the MSRV policy:
  [Cargo features and MSRV](../../../reference/developer/en/cargo-features.md).
- A first end-to-end pass through build, test, lint, and example:
  [Building and testing unified-archive from source](../../../tutorials/developer/en/build-and-test-from-source.md).
