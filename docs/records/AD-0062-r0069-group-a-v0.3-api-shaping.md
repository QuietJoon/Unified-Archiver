---
type: ADR
title: "AD 0062: Review 0069 Group A — v0.3 API-shaping plan"
description: "Planned (v0.3)"
tags: [decision, ADR-0062, ADR-0009, ADR-0058, DCR-003, R0069-0001, R0070-0080, R0070-0092, R0069-0010, R0069-0017, R0069-0052, R0069-0062, R0069-0058]
timestamp: 2026-05-04T00:00:00Z
status: active
---

# AD 0062: Review 0069 Group A — v0.3 API-shaping plan

Status: Planned (v0.3)

## Context and Problem Statement

OI-0069-001 (`docs/project/open-issues.md`) tracks eight Review 0069
findings whose resolutions reshape public API contracts or
format-detection behavior:

- R0069-0001 / R0070-0080 / R0070-0092 — `pub mod ffi` and the
  `pub mod creation/extraction/inspection/modification` behaviour
  modules expose internals that AD 0009 (Isolate Unsafe Behind Safe
  Wrappers) and AD 0058 (Feature-first footprint split) both want
  hidden.
- R0069-0010 — `extract_to_stream*` returns an unbounded reader by
  default; a malicious archive whose declared size disagrees with
  its decoded size can drive caller-side `read_to_end` to allocate
  unbounded memory.
- R0069-0017 — `extract_to_memory_with_options.verify_crc32` has a
  per-backend contract that varies silently (Piz/ZIP honour it; TAR
  family ignores it because libarchive does not surface a per-entry
  CRC; UnRAR always validates regardless of the flag).
- R0069-0052 — Write-mode duplicate-name detection runs only in
  modify mode (resolved for modify under OI-0069-002 R0069-0062);
  create mode still detects duplicates only at the writer-backend
  layer, which produces format-specific errors instead of a
  consistent diagnostic.
- R0069-0058 / R0069-0059 — `ModificationTracker.added: Vec<(String,
  Vec<u8>)>` forces every queued add to live fully in RAM; a 5 GB
  add allocates 5 GB before `commit_changes()` ever runs.
- R0069-0070 — `ArchiveFormat::detect_from_file` reads only 512
  bytes of magic; ISO 9660 PVD detection requires the buffer to
  reach offset 32769 (`CD001`), so ISO is structurally
  undetectable from a file path through the magic-only path.
- R0069-0071 — TAR.GZ / TAR.BZ2 / TAR.XZ extension fallback can
  route a plain gzip-with-`.tar.gz`-filename to the tar parser
  because the post-decompression payload is never verified to be a
  tar stream.

These findings interact: a v0.3 minor that addresses them
piecemeal would land contradictory commits and partial deprecation
states, so this ADR records the coordinated plan.

The project has **no public users yet** — the user explicitly
authorised hard breaking changes for v0.3 rather than the
deprecate-then-flip migration originally proposed. That permission
is what makes the visibility tightening (Group A.1) and streaming
rename (Group A.2) tractable in a single release.

## Decision Drivers

- AD 0009 (Isolate Unsafe Behind Safe Wrappers) — the FFI surface
  must not be reachable from external code so backend renames
  remain non-breaking.
- AD 0058 (Feature-first footprint split) — a future facade-crate
  split needs the behaviour modules under `pub(crate)` so the
  facade can re-export only the curated API.
- No-public-users posture — release notes, deprecation cycles, and
  semver compatibility are intentionally not in scope. Each item is
  free to land as a clean break.
- Cargo / Rust idioms — for source polymorphism on the modify add
  surface (Group A.4), prefer typed enums over `dyn Trait`
  abstractions when each variant has distinct commit-time
  semantics; defer the polymorphic-`Into<…>` ergonomics to a
  follow-up once the variants have stabilised.

## Plan by group

### A.1 — Visibility tightening (R0069-0001, R0070-0080, R0070-0092)

**Decision: option α — hide everything.**

- `src/lib.rs` flips `pub mod ffi` → `pub(crate) mod ffi` and
  `pub mod creation` / `extraction` / `inspection` / `modification`
  to `pub(crate) mod …`. The curated facade
  (`pub use archive::Archive;` etc.) stays the only public entry
  point.
- `pub use ffi::*` re-exports drop. Any item that external callers
  currently reach through `ffi::*` and that the facade does not
  already re-export gets either:
  - A facade re-export added next to the existing ones in
    `src/lib.rs`, or
  - A typed-equivalent in the public surface (e.g. if external
    code constructs a backend struct directly, the public API
    should expose a constructor rather than the struct).
- `docs/API_REFERENCE.md` updates to describe the closed surface;
  the prior wording about `ffi` being "exposed for crate-internal
  composition" goes away.

**Concrete migration steps**:

1. Audit `pub use` in `src/lib.rs` and confirm every facade-visible
   type round-trips through the facade only.
2. Search for `unified_archive::ffi::` / `unified_archive::creation::`
   / `extraction::` / `inspection::` / `modification::` references in
   `examples/`, `tests/`, `docs/` and the user's private project.
   Migrate each to the facade equivalent.
3. Flip the `pub mod` keywords.
4. Run `cargo doc --no-deps --all-features` and confirm the rendered
   API matches the curated facade.

### A.2 — Bounded streaming default (R0069-0010)

**Decision: option β with renamed methods.**

- `extract_to_stream(file_path)` becomes the bounded form.
  Internally returns `StreamingExtractor::take(declared_size)` so
  `Read::read_to_end` cannot exceed the archive's declared
  uncompressed size.
- `extract_to_stream_unbounded(file_path)` is the new name for the
  raw unbounded reader. Callers that need it must opt in
  explicitly. Rustdoc spells out the trust contract:
  "*returns an unbounded reader; a hostile archive whose decoded
  size exceeds its declared size will let `read_to_end` allocate
  past the limit.*"
- `extract_to_stream_with_options(...)` keeps its name but its
  default behaviour also bounds; an `unbounded: true` field on
  `ExtractionOptions` (or a separate `_unbounded` variant) gates
  the legacy behaviour.

**No deprecation cycle** — the rename lands clean.

### A.3 — CRC contract for `extract_to_memory_with_options` (R0069-0017)

**Decision: option β — normalise: `verify_crc32: true` errors when
the backend cannot honour it.**

- TAR family (libarchive without per-entry CRC): `verify_crc32 =
  true` returns `ArchiveError::Unsupported { reason:
  "Format X does not carry a per-entry CRC; cannot honour
  verify_crc32" }`. The caller must explicitly opt out by setting
  `verify_crc32 = false` for these formats.
- 7z, RAR: backends always validate; `verify_crc32 = false` is a
  no-op (CRC still runs because the codec checks it). Rustdoc
  documents this so the field is honest.
- Piz, ZIP: existing behaviour, both honour the flag.

The user-visible effect: any caller that previously passed
`verify_crc32 = true` against a TAR-family archive and got silent
no-op behaviour now gets a typed error and must decide
consciously.

### A.4 — Modify-side streaming (R0069-0058 / R0069-0059)

**Decision: revised option α — typed enum internal,
distinct-method API, with `add_entry(&[u8])` kept exactly as today.**
**Future evolution: option γ — polymorphic constructor via
`Into<EntrySource>` — recorded as a non-breaking follow-up.**

#### Internal representation

```rust
// src/modification.rs
pub(crate) enum EntrySource {
    /// Snapshot semantic — bytes captured at `add_entry` time.
    Buffered(Vec<u8>),
    /// Live filesystem path; opened + streamed at commit time.
    /// `commit_changes` routes to the writer's `add_file_from_path`
    /// helper so source mtime / atime / btime / Unix permissions
    /// flow into the archive entry when `preserve_metadata = true`.
    Path(PathBuf),
    /// Live reader; size is the libarchive header value when
    /// `Some`, else `commit_changes` stages through
    /// `stage_unknown_size_entry` to learn it (already implemented
    /// for retained entries under OI-0069-002 R0069-0065).
    Reader {
        reader: Box<dyn Read + Send + 'static>,
        size: Option<u64>,
    },
}

pub(crate) struct ModificationTracker {
    pub(crate) removed: HashSet<usize>,
    pub(crate) added: Vec<(String, EntrySource)>,
    pub(crate) added_directories: Vec<String>,
}
```

#### Public API

Three explicit constructor methods on `Archive`:

```rust
/// Snapshot the byte slice into the queued add. Unchanged from today.
pub fn add_entry(&mut self, archive_path: &str, data: &[u8]) -> Result<()>;

/// Capture the filesystem path. Opened and streamed at commit time.
/// When `preserve_metadata = true` on the active `ModificationOptions`,
/// the source path's mtime / atime / btime / Unix permissions are
/// emitted into the new entry via the writer's `add_file_from_path`
/// helper.
pub fn add_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()>;

/// Capture an owned reader. `size` is the entry's uncompressed
/// length:
/// - `Some(n)`: trusted; written into libarchive's header verbatim.
///   Mismatch with the actual stream length is treated as
///   `ArchiveError::Corruption` at commit time.
/// - `None`: commit_changes drains the reader through
///   `stage_unknown_size_entry` (already used for retained-entry
///   streaming) to learn the actual length before writing the
///   header.
pub fn add_entry_from_reader<R: Read + Send + 'static>(
    &mut self,
    archive_path: &str,
    reader: R,
    size: Option<u64>,
) -> Result<()>;
```

`replace_entry` gets symmetric `_from_path` / `_from_reader`
companions.

#### Commit-time branching

`commit_changes` branches over the `EntrySource` variant:

- `Buffered(data)` → existing path: writer's `add_file_from_data` /
  `add_file_from_data_with_metadata`. Snapshot semantics preserved.
- `Path(p)` → writer's `add_file_from_path` (which already exists on
  both ZipWriter and LibarchiveArchive); no separate metadata
  plumbing required.
- `Reader { reader, size: Some(n) }` → writer's
  `add_file_from_reader(... size = n)`.
- `Reader { reader, size: None }` → drain through
  `stage_unknown_size_entry` to materialise size, then route to
  `add_file_from_reader(... size = staged_len)`.

#### Failure semantics

Documented per-source on the constructor rustdoc:

| Source | Failure window |
|---|---|
| `Buffered` | Caller can retry `commit_changes` indefinitely; bytes are owned. |
| `Path` | If the file is deleted/replaced between `add_entry_from_path` and `commit_changes`, the commit returns the underlying I/O error; the tracker's other entries are unaffected. |
| `Reader` | Reader is consumed during commit. A failed commit cannot be retried without the caller re-supplying the reader; `clear_operations` resets the tracker. |

#### Future evolution: option γ

Once the typed enum stabilises in v0.3, a non-breaking follow-up
can add `From` impls that fold the three constructor methods into
a single polymorphic `add_entry`:

```rust
impl From<Vec<u8>> for EntrySource { ... }
impl From<&[u8]> for EntrySource { ... }
impl From<PathBuf> for EntrySource { ... }
impl From<&Path> for EntrySource { ... }
// Reader stays explicit — the size hint is structurally part of
// the construction.

impl Archive {
    pub fn add_entry<S: Into<EntrySource>>(
        &mut self,
        archive_path: &str,
        source: S,
    ) -> Result<()>;
}
```

This collapses to the call-site uniformity of option β
(`archive.add_entry("foo", b"hello".to_vec())?` /
`archive.add_entry("foo", PathBuf::from("..."))?`) without
sacrificing the typed-enum branching α delivers.

The follow-up is intentionally **not in scope for v0.3**:

1. Variant set must be observed in real use before promoting it to
   `pub` via `Into`. Adding a fourth variant (e.g. zero-copy
   memory-mapped slice) is non-breaking on α; on γ it is breaking
   if the new variant cannot be constructed from existing types.
2. `Into<EntrySource>` is mildly controversial in some Rust style
   guides for IDE-discoverability reasons — the explicit
   `add_entry_from_path` / `add_entry_from_reader` names show up in
   autocomplete; the polymorphic single method does not.
3. The migration is purely additive once the enum is `pub`.

### A.5 — Detect heuristic (R0069-0070; generalised)

**Decision: extension-guided format detection — read the magic
window an extension *suggests* the file might need, not the
maximum window.**

- `ArchiveFormat::detect_from_file(path)` walks the path's
  extension and chooses a magic-buffer size accordingly:
  - `.iso`, `.bin`, `.img`, `.cd` → 33 KiB read (covers the ISO
    PVD at offset 32769).
  - All others → 512 byte read (the current default), unchanged.
- The magic-buffer-detection branch then runs in priority order
  (full magic table first), falling back to extension-guided
  format inference only when no magic matched.
- This generalises to a single "extension hint, magic confirmation,
  fall back when uncertain" policy that A.6 also uses.

**Documented limitation**:

- An ISO file with a non-ISO extension (`installer.exe` carrying
  ISO bytes) detects as not-ISO from-file because the wider read
  is gated on the extension hint. False negatives in this shape
  are explicitly accepted in exchange for the cheap default.
- The from-bytes path (`detect_from_bytes(magic: &[u8])`) is
  unchanged — it already detects ISO when given a buffer ≥ 32774
  bytes.

**Update (DCR-003, 2026-05-04).** The post-magic-byte fallback set
expanded from `{Tar, Iso}` to `{Tar, Iso, Lzma, TarLzma}` to keep the
documented `.lzma` / `.tar.lzma` / `.tlz` open path reachable —
raw LZMA streams have no stable short magic. All other extensions
still fail with `Unknown archive format` after a missed magic check;
the policy itself is unchanged. See
[`DCR-003-review-0078-lzma-extension-fallback.md`](DCR-003-review-0078-lzma-extension-fallback.md).

### A.6 — TAR.* trust policy (R0069-0071; generalised)

**Decision: extension-guided trust with backend confirmation,
mirroring A.5.**

- `.tar.gz` / `.tar.bz2` / `.tar.xz` / `.tar.zst` filename
  fallback continues to route to the tar-wrapped backend, but the
  *open* path verifies the post-decompression payload is a valid
  tar stream. A plain gzip-with-`.tar.gz`-filename surfaces a
  typed `ArchiveError::Format` at open time instead of producing a
  malformed listing.
- The fast `detect_from_file` path stays optimistic — extension
  hint is sufficient to *guess*; backend open is what *confirms*.
- For paranoid callers, `Archive::open(path)` already does the
  confirmation since libarchive will fail to parse a non-tar
  payload. The new behaviour is to surface that failure with a
  precise `ArchiveError::Format` diagnostic instead of an opaque
  libarchive error string.

This becomes the second instance of the "extension-guided
heuristic, backend-confirmed truth" pattern A.5 introduces.

## Sequencing

1. Land additive items first (no caller migration required):
   - A.4 typed `EntrySource` + new `add_entry_from_path` /
     `add_entry_from_reader` methods.
   - A.5 extension-guided 33 KiB read for ISO-extension paths.
   - A.6 backend-confirmed TAR.* error mapping.
2. Land breaking items:
   - A.2 streaming method rename.
   - A.3 `verify_crc32` typed-error normalisation.
   - A.1 visibility tightening (last — unblocks the AD 0058
     facade-crate split).

After each step: `cargo fmt --all`, `cargo clippy --all-features
--all-targets`, `cargo test --all-features`, `cargo doc --no-deps
--all-features`.

## Consequences

- Good: AD 0009 visibility contract realised; bounded streaming
  by default closes a class of allocation-DoS surfaces;
  modify-side streaming removes the in-RAM cap on queued adds; ISO
  detection from-file works for the natural extension cases;
  TAR.* false positives surface a clean error.
- Good: future option γ migration is non-breaking once it lands.
- Bad: every `unified_archive::ffi::*`, `creation::*`,
  `extraction::*`, `inspection::*`, `modification::*` reference in
  the user's private project requires a one-time migration to
  facade equivalents. The migration is mechanical (search and
  replace the import paths) but unavoidable.
- Bad: extension-guided detection (A.5, A.6) admits false
  negatives by design. A user who renames an ISO file to
  `mystery.dat` will not see ISO detection from-file. The OI
  acknowledges this as an accepted cost.

## Related

- AD 0009 (Isolate Unsafe Behind Safe Wrappers)
- AD 0058 (Feature-first footprint split and facade crates)
- OI-0069-001 (R0069 Group A meta-tracker)
- OI-0058-001 (Feature-first footprint split)
- OI-0069-002 R0069-0062 (modify dir/file conflict — landed
  2026-04-28; commit `9ea5fdb`) — the create-side namespace gate
  R0069-0052 calls for follows the same pattern.
- OI-0069-002 R0069-0065 (retained-entry streaming — landed
  2026-04-28; same commit) — `stage_unknown_size_entry` is reused
  by A.4's `Reader { size: None }` branch.

## Amendment (2026-07-17, DCR-006 / R0080-0007)

§A.2's `StreamingExtractor::take(declared_size)` bound capped memory but made an
over-producing decoder indistinguishable from clean EOF — `read_to_end` stopped at
the declared size and reported success, silently truncating hostile output. The
bounded public methods (`extract_to_stream`, `extract_to_stream_with_options`, and
the v2-api `ReadArchive` wrappers) now wrap the reader with the hard-cap prober
(`with_hard_cap`, the same machinery the trait-default `_with_limit` path and
manifest digests already used): reading past the declared size yields a structured
`io::ErrorKind::InvalidData` error. The public return type changed from
`io::Take<StreamingExtractor>` to `StreamingExtractor`. The unbounded opt-in variant
and its trust contract are unchanged. See DCR-006.

## Amendment (2026-07-22, R0081-0027 / I2)

§A.2 as planned (and the DCR-006 amendment above) left a **four-method**
public stream surface: `extract_to_stream`, `extract_to_stream_unbounded`,
`extract_to_stream_with_options`, and `extract_to_stream_with_options_unbounded`.
The two `_unbounded` methods drifted — the no-options one called the plain
`list_files()` preflight and installed no mmap cap on the backend read, while the
with-options one used the budgeted preflight and propagated
`options.limits.max_mmap_size`. That divergence surfaced as R0081-0027 ("two
unbounded APIs disagree"), which was hand-patched but left structurally possible.

I2 collapses the surface to **one bounded entry point plus a typed bound**. A new
public `enum StreamBound { DeclaredSize, Cap(u64), Unbounded }` (in
`src/streaming.rs`, re-exported from the crate root) is passed to the two remaining
methods:

- `extract_to_stream(file_path, bound)` — uses `ExtractionOptions::default()`.
- `extract_to_stream_with_options(file_path, options, bound)` — carries both
  `ExtractionOptions` and the bound.

Both delegate to one private `Archive::extract_to_stream_impl`, which runs the
verify-CRC gate, password reopen, budgeted safety pre-check, and mmap-cap plumbing
**unconditionally** and then interprets `bound` in exactly one `match`:

- `DeclaredSize` — hard-cap at the entry's declared uncompressed size, falling back
  to `options.limits.max_file_size` when the size is unknown. This is the former
  default bounded behaviour; DCR-006's over-production ⇒
  `io::ErrorKind::InvalidData` contract is preserved verbatim via `with_hard_cap`.
- `Cap(n)` — hard-cap at exactly `n` bytes (new capability; same over-production
  error at a caller-chosen ceiling).
- `Unbounded` — no output cap: the former `_unbounded` reader with the same
  documented trust caveat.

The two `_unbounded` methods are **deleted** (public-API-breaking, authorised
pre-1.0). Because "unbounded" is now one value chosen in one place rather than a
parallel method whose shared preflight can drift, R0081-0027 becomes structurally
impossible. The no-options path now also honours the caller-neutral default mmap
cap and budgeted listing that the with-options path always used, so the two entry
points are behaviourally identical modulo the supplied options.

The v2-api `ReadArchive` wrappers collapse to the same two-method shape
(`src/archive/mode_split.rs`). The internal `ValidatedSource` and backend-trait
stream methods (`extract_to_stream[_with_caps|_with_limit|_by_listing_id]`) are
unchanged — they are the mechanism `extract_to_stream_impl` drives. The memory-side
public API (`extract_to_memory` / `extract_to_memory_with_options`) never had an
`_unbounded` twin and is untouched; the internal `CopyByteBudget` descriptor
(`src/ffi/common.rs`) remains the memory layer's typed bound.
