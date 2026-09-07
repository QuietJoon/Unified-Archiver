---
type: Design Note
title: "libzstd-rs-sys (Trifecta Tech Foundation) — adoption evaluation"
description: "Status: research note, evaluated 2026-06-10."
tags: [design-note, R0075-0031, OI-0058-001]
timestamp: 2026-06-10T00:00:00Z
status: active
---

# libzstd-rs-sys (Trifecta Tech Foundation) — adoption evaluation

**Status:** research note, evaluated 2026-06-10. **Verdict: hold — no adoption path today; re-evaluate on the triggers in §7.** No code change, no dependency change.

## 1. What was evaluated

- Repository: <https://github.com/trifectatechfoundation/libzstd-rs-sys>
- Announcement: <https://trifectatech.org/blog/announcing-zstandard-in-rust/> (published 2026-06-01)
- Question: is this library worth adopting in `unified-archive`?

`libzstd-rs-sys` is the Trifecta Tech Foundation's memory-safe Rust reimplementation of the Zstandard (zstd) compression library — the third in their compression series after `zlib-rs` (production-grade, ships inside `flate2`, 30M+ downloads) and `libbzip2-rs`. The starting point is a `c2rust` translation of upstream C zstd, incrementally cleaned into safe, idiomatic Rust, validated against the upstream implementation at every step.

## 2. Maturity snapshot (as of 2026-06-10)

| Component | State |
|---|---|
| Version | `0.0.1-prerelease.2` (released 2026-06-01 — nine days old at evaluation) |
| crates.io | **Not published.** Cannot be added as a normal Cargo dependency. |
| Decoder | Cleaned up from the c2rust output, "mostly safe". README: *"ready for experimental use, but not yet battle-tested."* |
| Encoder | **Exists and is functional**, but is *"still mostly just the raw c2rust translation and hence full of unsafe code."* Cleanup is the project's Milestone 4 (future work). |
| Dictionary builder | Complete (NLnet + Sovereign Tech Agency funded). |
| Testing | Upstream reference suite (`zstreamtest.c` from Meta), 3 fuzz targets (decompression, dictionary ops, Huffman), Miri on nightly. CI green as of 2026-06-09. |
| Performance | Decoder ~3 % slower than C by default; parity with the `unsafe-performance-experimental` feature flag (disables 4 bounds checks). |
| Backing | Chainguard, Astral, NLnet Foundation, Sovereign Tech Agency. Credible, funded team with a strong track record (`zlib-rs`). |

> Note: one early research pass claimed "the encoder is not yet implemented." That is wrong — adversarial re-verification against the repository README confirmed the encoder exists and functions; Milestone 4 is encoder *cleanup*, not initial implementation. Recorded here so the error is not repeated.

## 3. Consumption model

`libzstd-rs-sys` is a **C-ABI drop-in**, not an idiomatic Rust crate:

- `cargo build --features=export-symbols` produces a static `liblibzstd_rs_sys.a` exporting the C `ZSTD_*` symbol surface. C projects (or anything that links libzstd) link against it instead of system libzstd.
- The safe-Rust consumption path is Trifecta's **fork of the `zstd` crate** at <https://github.com/trifectatechfoundation/zstd-rs>, which swaps the C backend for `libzstd-rs-sys`. The fork is **not upstreamed** to `gyscos/zstd-rs` and not published.

## 4. Why there is no insertion point in unified-archive today

All zstd capability in this crate (`.zst` and `.tar.zst` read/extract) flows through exactly one path:

```
unified-archive ──build.rs/pkg-config──▶ system libarchive (dylib=archive)
                                              └──▶ system C libzstd (.dylib)
```

- `Cargo.lock` contains **no `zstd` or `zstd-sys` crate** anywhere in the tree (verified by grep). `sevenz-rust2` and `zip` pull none in our feature configuration. (`piz` was evaluated and is not a dependency — DCR-009 left the `zip` crate as the sole ZIP backend.) There is zero Rust-level zstd code to swap.
- A Cargo `[patch]` cannot help: it only redirects Rust crates in our dependency graph. The system `libarchive.dylib` was compiled and dynamically linked against C libzstd long before our build runs; nothing at the Cargo level changes that.
- The only way to put `libzstd-rs-sys` underneath this crate is to **rebuild libarchive from source** against the Rust static `.a` — abandoning the pkg-config/Homebrew system-library model for a vendored per-platform libarchive build, all to obtain a slightly slower, not-yet-battle-tested decoder. The risk/reward is upside-down.

## 5. The scenario where it becomes relevant: v0.4 zstd create

`.zst` / `.tar.zst` **creation** was deferred to v0.4 (R0075-0031), planned through libarchive's `archive_write_add_filter_zstd`. **Partly shipped:** `.tar.zst` creation landed by exactly that route (`archive_write_add_filter_zstd` in `src/ffi/libarchive_wrapper/writer.rs`); standalone `.zst` creation remains unimplemented. The Route B reasoning below is kept as live analysis for the standalone case. Two routes exist for that work:

- **Route A (planned):** wire `archive_write_add_filter_zstd` through the existing libarchive FFI layer. No new dependency; consistent with how every other libarchive-backed format works here.
- **Route B (alternative):** bypass libarchive for zstd and use a pure-Rust zstd implementation directly. This is the only route in which the Trifecta work matters to this project — and even then the dependency would be the safe `zstd` crate (Trifecta's fork, once published/upstreamed), **never** the bare `-sys` crate.

Route B is blocked today on exactly the immature half: creation is the **encoder** path, and the encoder is the raw-c2rust, unsafe-heavy part. Adopting it now would trade libarchive's battle-tested C encoder for an uncleaned machine translation — backwards on safety, the stated motivation.

Existing pure-Rust alternative for comparison: `ruzstd` (decoder-focused, 1.4–3.5× slower than C, no encoder parity). Not compelling for either route.

## 6. License

- `Cargo.toml` declares **BSD-3-Clause**; the repo also carries a GPL-2.0 `COPYING` file — the same *BSD OR GPLv2* dual-licensing inherited from upstream Facebook/Meta zstd.
- Taking the BSD-3-Clause option is compatible with this crate's MIT license. **Not a blocker**, but any future adoption PR should state the BSD election explicitly in the dependency-license inventory.

## 7. Decision and re-evaluation triggers

**Hold.** Do not adopt, do not add a dependency, keep Route A (libarchive `archive_write_add_filter_zstd`) as the v0.4 plan of record.

Re-evaluate when **both** of these become true:

1. **Architecture motivation:** the project decides v0.4+ zstd create should be pure-Rust (e.g. to drop the system-libarchive dependency for zstd, enable static/portable builds, or extend the memory-safety boundary). At that point compare the safe `zstd` fork against `ruzstd` — not the `-sys` crate directly.
2. **Library maturity:** `libzstd-rs-sys` ships a tagged stable release on crates.io with a **cleaned-up encoder** (Milestone 4 done), and/or the Trifecta backend is upstreamed into the mainline `zstd` crate.

Bellwether to watch: `zlib-rs` reached production via exactly this path (clean-up → crates.io → adopted as a `flate2` backend). zstd-rs is on the same trajectory but is at the start of it.

## Related

- R0075-0031 / v0.4 deferral of `.zst` / `.tar.zst` create (`src/format.rs`, `TarZst` / `Zst` doc comments)
- `build.rs` — pkg-config discovery and `dylib=archive` linkage (the sole zstd entry point)
- OI-0058-001 — feature-first footprint split (a future facade-crate split would change where a pure-Rust zstd backend could slot in)
