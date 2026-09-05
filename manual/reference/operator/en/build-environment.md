---
type: Reference
title: Build environment
description: What a unified-archive build requires per platform, exactly what build.rs does, which libraries end up on the link line, and the current platform support status.
tags: [build, platform, config]
audience: operator
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: build-script, resource: build.rs }
  - { id: manifest, resource: Cargo.toml }
  - { id: lockfile, resource: Cargo.lock }
  - { id: readme, resource: README.md }
  - { id: open-issues, resource: docs/project/open-issues.md }
  - { id: libarchive-bindings, resource: src/ffi/libarchive.rs }
  - { id: libarchive-writer, resource: src/ffi/libarchive_wrapper/writer.rs }
  - { id: cargo-config, resource: .cargo/config.toml }
  - { id: test-config, resource: tests/common/config.rs }
synced_hash: 1a10c1623050066b404239ca9a7880ec79043277745ff1f0e3d0be0569173442
---

# Build environment

## Requirements by target

| Target | Rust | libarchive | pkg-config | C++ toolchain |
|---|---|---|---|---|
| macOS | 1.85 or newer | required; either a Homebrew keg prefix carrying `lib/libarchive.dylib`, or an installation visible to pkg-config | required only for the pkg-config fallback path | required when `rar-support` is enabled (the default); `cc` selects the platform compiler |
| Linux and other non-Windows targets | 1.85 or newer | required, with pkg-config metadata (`libarchive.pc`, shipped in the development package) | required | required when `rar-support` is enabled |
| Windows | 1.85 or newer | not configured by the build script — no link directives are emitted (see [Platform support status](#platform-support-status)) | not used | required when `rar-support` is enabled; `cc` selects MSVC for an `msvc` target environment |

The toolchain floor is the same for every target: Rust 1.85 or newer, because the
crate declares `edition = "2024"` and `rust-version = "1.85"`.

`make` is not part of the build. `build.rs` shells out to nothing directly; the only
child processes are the ones the `pkg-config` and `cc` crates start — the `pkg-config`
binary and the C++ compiler and archiver. The vendored UnRAR `makefile` is still
present in `src/ffi/native/unrar`, but it is not a build input and is not watched for
changes. `cc` honours Cargo's `TARGET`, `CC`, `CXX`, and `AR`.

No libarchive headers are compiled: the bindings in `src/ffi/libarchive.rs` are
hand-written Rust `unsafe extern "C"` declarations, so libarchive is a link-time and
run-time requirement, not a compile-time include requirement. The pkg-config probe
still needs the `.pc` file that development packages install.

For the per-platform install commands, see
[How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md).
For which features change what is built, see
[Cargo features and MSRV](../../../reference/developer/en/cargo-features.md).

## What `build.rs` does

The script runs four things, in this order.

### 1. Its own change tracking

Emits `cargo:rerun-if-changed=build.rs`.

### 2. libarchive discovery

The discovery block is selected by `#[cfg(target_os = …)]`. In a build script those
`cfg` values describe the **build host**, not Cargo's `--target`; that host-versus-target
mismatch is the still-open half of OI-0080-001.

**macOS host.** Two candidate prefixes are examined, in order:
`/opt/homebrew/opt/libarchive` (Apple Silicon layout) and
`/usr/local/opt/libarchive` (Intel layout). A prefix counts only when
`{prefix}/lib/libarchive.dylib` exists, so a bare or partially removed keg falls
through instead of producing a search path that fails later in the linker. On a hit
the script emits `cargo:rustc-link-search=native={prefix}/lib` and
`cargo:rustc-link-lib=dylib=archive`, and prints
`unified-archive: using Homebrew libarchive from {prefix}` to stderr. On a miss it
calls `pkg_config::probe_library("libarchive")`; if that returns an error the script
panics with `libarchive not found. Install it with: brew install libarchive`.

**Other non-Windows hosts.** `pkg_config::probe_library("libarchive")` is the only
mechanism. On success the `pkg-config` crate emits the link search paths and `-l`
directives it derived. On error the script panics with a message naming the
distribution packages:

```text
libarchive not found via pkg-config. Install libarchive development files: `sudo apt-get install libarchive-dev` (Ubuntu/Debian) or `sudo dnf install libarchive-devel` (Fedora/RHEL).
```

**Windows host.** The script prints
`cargo:warning=Windows libarchive linking will be configured in future implementation`
and emits no `rustc-link-search` and no `rustc-link-lib`. libarchive link
configuration must therefore come from outside the build script; `README.md`
describes supplying it through `RUSTFLAGS="-L native=<path>"` over a vcpkg
installation or vendoring a prebuilt `.lib`.

**Cross builds.** `pkg-config` (0.3.32 in `Cargo.lock`) skips the probe when `TARGET`
differs from `HOST` unless `PKG_CONFIG_ALLOW_CROSS` is set to something other than
`0`, or `PKG_CONFIG` / `PKG_CONFIG_SYSROOT_DIR` is set. A skipped probe reaches
`build.rs` as an error and therefore as the same "libarchive not found" panic.

### 3. The vendored UnRAR build

Compiled only under `#[cfg(feature = "rar-support")]`, which is in the default feature
set. The step is gated on the feature alone, not on the host, and it prints
`unified-archive: building vendored UnRAR (cc) for target_os={target_os}` to stderr.

Target selection reads `CARGO_CFG_TARGET_OS` and `CARGO_CFG_TARGET_ENV`, so the
source set, the preprocessor defines, and the flag choices follow Cargo's target
rather than the build host.

| Aspect | Windows target | Every other target |
|---|---|---|
| Source list | the 50-file `WINDOWS_SOURCES` set, mirroring the upstream `UnRARDll.vcxproj` `<ClCompile>` list — it adds `isnt.cpp`, `motw.cpp`, `rarpch.cpp`, and `rs.cpp`, and drops `resource.cpp` and `list.cpp` | the 48-file `UNIX_SOURCES` set, mirroring `OBJECTS` + `LIB_OBJ` from the vendored makefile's `lib` target |
| Defines | `RARDLL`, `UNRAR`, `SILENT` | `RARDLL`, `_FILE_OFFSET_BITS=64`, `_LARGEFILE_SOURCE`, `RAR_SMP` |
| Flags | none added when `CARGO_CFG_TARGET_ENV` is `msvc` | `-std=c++11`, `-Wno-logical-op-parentheses`, `-Wno-switch`, `-Wno-dangling-else`, each applied only if the compiler supports it; plus position-independent code and `-pthread` |

Many vendored `.cpp` files (`unpack15/20/30/50`, `crypt1`–`crypt5`, `recvol3/5`,
`blake2s_sse`, the `win32*` helpers, `ulinks`, `uowners`, `hardlinks`, and others) are
`#include`d by other translation units and are deliberately not compiled standalone;
compiling them would fail the link on duplicate symbols. `cc`'s extra warnings are
switched off for this third-party code.

`cc::Build::compile("unrar")` writes every object file and the resulting static
library under `OUT_DIR` — the vendored source tree is never modified — and emits
`cargo:rustc-link-search=native=<OUT_DIR>` together with
`cargo:rustc-link-lib=static=unrar`.

### 4. Vendored-source change tracking

The script walks `src/ffi/native/unrar` recursively and emits
`cargo:rerun-if-changed=<path>` for every `.cpp`, `.hpp`, and `.h` file, including
files in subdirectories and the headers and `#include`d sources that `cc` does not
track by itself. Filesystem errors are not swallowed: a directory that cannot be
enumerated, an entry that cannot be read, or a file that cannot be stat-ed panics with
the offending path in the message, rather than allowing a stale native artifact to be
reused.

## Environment variables

`build.rs` reads exactly two variables itself, both in the UnRAR step:

| Variable | Effect |
|---|---|
| `CARGO_CFG_TARGET_OS` | selects the UnRAR source list, defines, and flag set (`windows` versus everything else) and the pthread link directive (`linux` and `android` only). Read with `unwrap_or_default`, so an unset value becomes an empty string and takes the non-Windows path |
| `CARGO_CFG_TARGET_ENV` | suppresses the GNU/clang flags when the value is `msvc`. Read with `unwrap_or_default` |

Cargo sets both. The script reads no other environment variable, and the crate
defines no build-time configuration variable of its own: there is no
`UNIFIED_ARCHIVE_*` build knob anywhere in the tree.

Variables consumed indirectly by the two build dependencies, which also emit their own
`cargo:rerun-if-env-changed` directives (so `build.rs` emits none):

- `pkg-config`, through `probe_library`, which runs with `env_metadata` enabled:
  `PKG_CONFIG`, `PKG_CONFIG_PATH`, `PKG_CONFIG_LIBDIR`, `PKG_CONFIG_SYSROOT_DIR`,
  `PKG_CONFIG_ALL_STATIC`, `PKG_CONFIG_ALL_DYNAMIC`, `PKG_CONFIG_ALLOW_CROSS`, the
  library-specific `LIBARCHIVE_NO_PKG_CONFIG`, `LIBARCHIVE_STATIC`, and
  `LIBARCHIVE_DYNAMIC` overrides, and the target-suffixed and `HOST_`/`TARGET_`-prefixed
  forms of those names.
- `cc`: the compiler and archiver selection and flag variables (`CC`, `CXX`, `AR`,
  `CFLAGS`, `CXXFLAGS`, and their target-suffixed forms).

## Libraries on the link line

| Library | Kind | Where the directive comes from | When |
|---|---|---|---|
| `archive` | dynamic | explicitly on the macOS Homebrew path; otherwise from the directives the `pkg-config` crate emits. Also declared by `#[link(name = "archive")]` on the extern block in `src/ffi/libarchive.rs` | every non-Windows build; nothing is contributed on Windows |
| `unrar` | static, from `OUT_DIR` | `cargo:rustc-link-lib=static=unrar` plus the `OUT_DIR` search path from `cc`. Also declared by `#[link(name = "unrar")]` in `src/ffi/unrar.rs` | only with `rar-support` |
| the C++ standard library | as `cc` chooses | emitted by `cc` itself because the builder sets `cpp(true)`. `build.rs` contains no per-platform C++ runtime branch at all; its comment records the mapping `cc` applies — libc++ on macOS, libstdc++ on Linux, nothing needed for MSVC | only with `rar-support` |
| `pthread` | dynamic | `cargo:rustc-link-lib=dylib=pthread`, emitted only when `CARGO_CFG_TARGET_OS` is `linux` or `android`, for the `RAR_SMP` threadpool. macOS needs no directive because pthread lives in libSystem, and this is the only explicit per-platform link branch in the script | only with `rar-support` |

## Codecs in the linked libarchive

libarchive is linked dynamically, so which compression filters work is a property of
the installed library, not of this crate. The write path calls
`archive_write_add_filter_gzip`, `_bzip2`, `_xz`, `_zstd`, `_lz4`, and `_lzma`
according to the requested format, and treats any return other than `ARCHIVE_OK` as a
hard `Format` error — the check is applied to every filter arm. A libarchive built
without libzstd or liblz4 registers those two filters as an external-program fallback
and returns `ARCHIVE_WARN`, which this crate converts into that error instead of
silently shelling out to an external compressor. Creating TAR.ZST, TAR.LZ4, and
TAR.LZMA therefore requires a libarchive with the matching codec compiled in.
Per-format detail is in
[Format support matrix](../../../reference/user/en/format-support-matrix.md).

## Build outputs

All native artifacts — UnRAR object files and the static `unrar` library — are written
under `OUT_DIR`. The vendored source tree is left untouched by the build.

The repository ships `.cargo/config.toml.example`, not a working `.cargo/config.toml`.
The real file is machine-local and git-ignored, so a fresh clone builds into `./target`
with no configuration at all. Copy the example and edit it only if you want a shared
target directory; because `[build]` `target-dir` there is an absolute path, a value
that is not writable on your machine makes Cargo fail before it compiles anything.

## Diagnostics the build emits

| Channel | Text | Condition |
|---|---|---|
| stderr | `unified-archive: using Homebrew libarchive from {prefix}` | macOS host, a Homebrew prefix carrying the dylib was found |
| stderr | `unified-archive: building vendored UnRAR (cc) for target_os={target_os}` | `rar-support` enabled |
| `cargo:warning` | `Windows libarchive linking will be configured in future implementation` | Windows host, **and** no libarchive link configuration was detected |
| panic | `libarchive not found. Install it with: brew install libarchive` | macOS host, no Homebrew dylib and the pkg-config probe failed |
| panic | `libarchive not found via pkg-config. Install libarchive development files: …` | non-macOS non-Windows host, pkg-config probe failed |
| panic | `Failed to enumerate vendored UnRAR sources in {dir}`, `Failed to read a directory entry in vendored UnRAR sources {dir}`, or `Failed to stat vendored UnRAR source {path}` | `rar-support` enabled and the vendored tree cannot be walked |

Both stderr lines are visible with `cargo build -vv`.

The Windows `cargo:warning` is conditional, not unconditional. `build.rs` first looks
for evidence that linking has already been arranged — a `-L` link-search flag in the
rustflags Cargo will pass on, or `archive.lib` / `libarchive.lib` / `archive_static.lib`
present in a directory named by `LIBARCHIVE_LIB_DIR` or `LIB` — and stays silent when it
finds any. It detects and never configures, so reading those variables decides only
whether to warn. A build whose link was arranged externally therefore does not get told
it is unconfigured.

## Platform support status

macOS and Linux are the tested platforms. Windows support exists in the source but is
not release-verified:

- The vendored UnRAR half is done. `build.rs` compiles the MSVC source set natively,
  and the earlier `make`-only build — along with the Windows `panic!` that replaced it
  as a stopgap — is gone.
- The libarchive half is open. On Windows `build.rs` performs no libarchive discovery of
  its own — it only reports whether link configuration appears to be present — so unless
  you supply that configuration yourself nothing links libarchive: the libarchive-backed
  formats (the TAR family, ISO, the
  standalone compressed streams, and non-ZIP creation) cannot be built there without
  externally supplied link configuration. This is OI-0065-001 in
  `docs/project/open-issues.md`, whose remaining required actions are vcpkg-based
  discovery, documenting the chosen mechanism in the README, and a Windows CI job. The
  choice of discovery mechanism is recorded as an owner decision in `docs/backlog.md`.
- Cross-compilation is not claimed, documented, or tested before v2. The UnRAR half of
  OI-0080-001 was resolved when the build moved to `cc`; the libarchive-discovery half
  — the host-`cfg` branching described above, plus the absence of a cross-compile
  smoke job — is still open.

## Environment inputs outside the build script

The library reads no environment variable at run time. Temporary staging goes through
`tempfile` and `std::env::temp_dir()`, which follows the platform's own temporary
directory convention.

The integration-test harness has its own resolution chain in `tests/common/config.rs`,
consulted in this order:

1. `UNIFIED_ARCHIVE_TEMP_DIR`, if the path exists or can be created.
2. `temp_dir` from the first `.unified-archive.toml` found by walking up from the
   current directory, if that path exists or can be created. `.unified-archive.toml` is
   machine-local and git-ignored; `.unified-archive.toml.example` is what ships, and it
   carries a placeholder path rather than a real one.
3. `std::env::temp_dir()`.

Building and running the suite from a clean checkout is covered by
[Building and testing unified-archive from source](../../../tutorials/developer/en/build-and-test-from-source.md).
