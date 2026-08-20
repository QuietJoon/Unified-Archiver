---
type: Reference
title: Options and defaults
description: Field-by-field description of every option and limit type in unified-archive, with types, defaults, and per-backend semantics.
tags: [config, api, extraction, creation, modification, security]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: options, resource: src/options.rs }
  - { id: security, resource: src/security.rs }
  - { id: modification, resource: src/modification.rs }
  - { id: streaming, resource: src/streaming.rs }
  - { id: password, resource: src/password.rs }
  - { id: zip-writer, resource: src/ffi/zip_writer.rs }
  - { id: libarchive-writer, resource: src/ffi/libarchive_wrapper/writer.rs }
synced_hash: e9a1c0660c23bcfbd4643ee4289fe8c3e802dd4cf15a13ec35dfd566870b57c2
---

# Options and defaults

Every configuration type the public API accepts, with its fields, types, defaults, and the
semantics the code records. Format-level capability questions belong to
[Format support matrix](../../../reference/user/en/format-support-matrix.md); error variants named
here are described in [Errors and warnings](../../../reference/user/en/errors-and-warnings.md).

## `ExtractionOptions`

Defined in `src/options.rs`. All fields are public and the struct is not `#[non_exhaustive]`, so
struct-literal construction works. It derives nothing: there is no `Debug` and no `Clone`, because
`filter` and `progress` hold boxed trait objects.

| Field | Type | Default |
|---|---|---|
| `destination` | `PathBuf` | `PathBuf::from(".")` |
| `password` | `Option<Password>` | `None` |
| `overwrite` | `bool` | `false` |
| `preserve_permissions` | `bool` | `true` |
| `preserve_times` | `bool` | `true` |
| `verify_crc32` | `bool` | `false` |
| `limits` | `ExtractionLimits` | `ExtractionLimits::default()` |
| `filter` | `Option<EntryFilter>` | `None` |
| `progress` | `Option<Box<dyn ProgressCallback>>` | `None` |

One builder method exists: `ExtractionOptions::password(self, impl Into<String>) -> Self` wraps the
argument in a `Password` and returns the value for chaining off `ExtractionOptions::default()`.

`overwrite: false` means extraction fails with an error when an output file already exists rather
than replacing it.

### `preserve_permissions`

Unix mode bits on extracted files. Per-backend behaviour as recorded in the field's
documentation:

- **ZIP** — Unix mode bits from the central directory are applied, masked to the permission bits,
  when the entry was written by a Unix-style host. Entries without Unix metadata keep the staging
  file's default mode.
- **7z** — applied when the archive stores a Unix mode in the upper half of the attribute field
  (the p7zip convention).
- **RAR / RAR5** — the flag is not consulted. The UnRAR library applies entry attributes itself,
  so permissions are always preserved.
- **libarchive-backed** (TAR family, ISO, standalone compressed streams) — honoured through
  `ARCHIVE_EXTRACT_PERM`.

On non-Unix platforms the flag has no effect for the native Rust backends (ZIP and 7z).

### `preserve_times`

Modification times on extracted files:

- **ZIP** — the DOS-precision (two-second) modification time is applied to each extracted file.
- **7z** — the NT-time modification timestamp is applied when the archive carries one.
- **RAR / RAR5** — the flag is not consulted; the UnRAR library applies entry times itself, so
  they are always preserved.
- **libarchive-backed** — honoured through `ARCHIVE_EXTRACT_TIME`.

The native Rust backends restore only the modification time. Accessed and created times are
listing-only metadata there.

### `verify_crc32`

Per-entry CRC32 verification during extraction:

- **ZIP** — explicit per-entry CRC32 verification is honoured.
- **7z** — CRC32 always validates regardless of this flag, because the codec checks integrity
  unconditionally. Setting `false` expresses "I do not require CRC32" without changing behaviour.
- **RAR / RAR5** — same as 7z: CRC32 always validates.
- **libarchive-backed** — no per-entry CRC32 is surfaced. `verify_crc32 = true` returns
  `ArchiveError::Unsupported`; the wrapping codec's own integrity check still runs.

The gate runs before any I/O, on every extraction entry point, so an unsupported request fails
before files are written. What a CRC32 match does and does not prove is discussed in
[What archive checksums actually prove](../../../explanation/user/en/checksums-and-integrity.md).

## `CompressionLevel`

An enum with six variants, in declaration order: `Store`, `Fastest`, `Fast`, `Normal`, `Maximum`,
`Ultra`. Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. It has no default of its own; the
options types that carry it default to `Normal`.

Backends map the variants onto their own scales:

| Variant | ZIP writer | libarchive writer (`compression-level`) |
|---|---|---|
| `Store` | stored method, no deflate level | `0` (`1` for `TarZst`) |
| `Fastest` | deflate, level 1 | `1` |
| `Fast` | deflate, level 3 | `3` |
| `Normal` | deflate, level 6 | `6` |
| `Maximum` | deflate, level 8 | `8` |
| `Ultra` | deflate, level 9 | `9` |

For the libarchive writer the value is set as a filter option on the `Tar*` formats and as a format
option on ZIP and 7z. A libarchive that ignores the option returns a warning status, which the
writer converts into `ArchiveError::Format` rather than silently falling back to the codec default.

## `CompressionOptions`

Defined in `src/options.rs`. All fields are public and the struct is deliberately **not**
`#[non_exhaustive]`, preserving struct-literal source compatibility for 0.3.

| Field | Type | Default (`new` / `Default`) |
|---|---|---|
| `format` | `ArchiveFormat` | `new`: the argument. `Default`: `ArchiveFormat::Zip` |
| `level` | `CompressionLevel` | `CompressionLevel::Normal` |
| `password` | `Option<Password>` | `None` |
| `split_size` | `Option<u64>` | `None` |
| `progress` | `Option<Box<dyn ProgressCallback>>` | `None` |

`CompressionOptions::new(format)` sets the format and takes the defaults above.
`CompressionOptions::default()` is `new(ArchiveFormat::Zip)`.

Methods:

- `format(&self) -> ArchiveFormat` — reads the field back.
- `password(self, impl Into<String>) -> Self` — builder setter storing a `Password`.
- `strip_progress(&self) -> Self` — see below.
- `validate_for_format(&self) -> Result<()>` — see below.

There is a hand-written `Debug` implementation. It prints `format`, `level`, `split_size`,
`password` as `Some("***")` or `None`, and `progress` as the boolean `is_some()`.

### Why it is not `Clone`

`progress` is a `Box<dyn ProgressCallback>`, which cannot be cloned. An earlier `Clone`
implementation dropped the callback silently, so a cloned options value stopped emitting progress
without the caller noticing. The implementation was removed rather than kept lossy.

### `strip_progress`

Returns a new `CompressionOptions` carrying the same `format`, `level`, `password`, and
`split_size`, with `progress` set to `None`. The original callback stays on `self` unless the
returned value is assigned back. This is the explicit replacement for the removed `Clone`.

### `validate_for_format`

An optional preflight that reports `ArchiveError::OperationBlocked` for a configuration
`Archive::create` would reject. `Archive::create` calls the same method internally, so the two
surfaces cannot disagree. The rules, in the order they are checked:

1. `format` must satisfy `ArchiveFormat::can_create()`. Otherwise the reason reads
   `format {:?} is not supported for creation via Archive::create`.
2. `password` must be `None`, for every format. Otherwise the reason reads
   `encrypted creation for {:?} is not supported (MADR-0027); password must be None`.
3. `split_size` must be `None`, for every format. Otherwise the reason reads
   `split_size is not supported by any backend yet (DEF-002); leave it None`.

The operation label on all three is the crate's `create` operation constant.

### Format-specific builders

Three narrower builders exist. Each exposes only the fields its format honours, so unsupported
combinations are unrepresentable rather than rejected at run time. Their fields are `pub(crate)`;
values are set through the methods.

| Type | Fields exposed | `new` | `Default` | Paired constructor |
|---|---|---|---|---|
| `ZipCompressionOptions` | `level`, `progress` | `new()` | yes, equals `new()` | `Archive::create_zip` |
| `SevenZCompressionOptions` | `level`, `progress` | `new()` | yes, equals `new()` | `Archive::create_seven_zip` |
| `LibarchiveCompressionOptions` | `format`, `level`, `progress` | `new(format)` | none | `Archive::create_libarchive` |

All three provide `level(self, CompressionLevel) -> Self` and
`progress(self, Box<dyn ProgressCallback>) -> Self`. `new()` (and `new(format)`) start at
`CompressionLevel::Normal` with no progress callback.

Neither ZIP nor 7z builder exposes a `password` field: create-time encryption is out of scope for
every format, so there is no encrypted-creation state to construct.
`LibarchiveCompressionOptions` takes the format up front and does not validate it there; a format
`can_create()` rejects is refused at the `Archive::create_libarchive` call.

Each builder lowers into `CompressionOptions` through a `From` implementation:

- `From<ZipCompressionOptions>` sets `format: ArchiveFormat::Zip`, carries `level` and `progress`,
  and sets `password: None`, `split_size: None`.
- `From<SevenZCompressionOptions>` sets `format: ArchiveFormat::SevenZip` and is otherwise
  identical.
- `From<LibarchiveCompressionOptions>` carries `format`, `level`, and `progress`, and sets
  `password: None`, `split_size: None`.

`Archive::create_zip` and `Archive::create_seven_zip` are exactly
`Archive::create(path, opts.into())`. `Archive::create_libarchive` first rejects
`ArchiveFormat::Zip` with `OperationBlocked` and then does the same.

## `ModificationOptions`

Defined in `src/modification.rs`. All fields are public. Derives `Debug` only — not `Clone`,
because `compression` embeds a `CompressionOptions` and therefore a non-cloneable progress
callback.

| Field | Type | Default (`new` / `Default`) |
|---|---|---|
| `preserve_metadata` | `bool` | `true` |
| `create_backup` | `bool` | `false` |
| `backup_suffix` | `String` | `".bak"` |
| `compression` | `Option<CompressionOptions>` | `None` |

`ModificationOptions::default()` is `ModificationOptions::new()`.

### `preserve_metadata`

When `true`, the commit re-emits `modified`, `accessed`, and `created` times where the source
listing exposes them, plus Unix permissions on backends that support them. The libarchive writer
uses `archive_entry_set_atime` and `archive_entry_set_birthtime`; the ZIP writer attaches a `0x5455`
"Universal Time" extra-field block so accessed and created times survive the rewrite.

It applies to **regular file entries only**. Retained directory entries are re-emitted with
backend-default permissions (`0o755` on libarchive) and the rewrite's wall-clock time regardless of
this flag.

### `compression`

`None` means format defaults (`CompressionLevel::Normal`, no password). When `Some`, `level` and
`progress` participate in the rewrite; `password` is rejected by the create facade and `split_size`
remains unused. The rewrite must keep the source container format: the commit rejects an override
whose `format` field disagrees with the archive's detected format rather than substituting either
side.

### `with_backup(suffix)`

Sets `create_backup = true` and records `suffix`. `suffix` must be a sibling-file suffix such as
`.bak` or `.orig`; the backup is always written as `<archive_path><suffix>` next to the source.

A suffix that is empty or contains `/` or `\` is invalid. In that case the supplied value is
ignored and `.bak` is used instead. Debug builds additionally fire a `debug_assert!`; release
builds keep the silent fallback so a programming mistake does not escalate to a panic. The commit
path re-validates the suffix, which is the load-bearing check.

### `without_metadata_preservation()`

Sets `preserve_metadata = false` and returns `self`.

## `ExtractionLimits`

Defined in `src/security.rs`. Derives `Debug` and `Clone`. All fields are **private**; construct
with `ExtractionLimits::default()` or `ExtractionLimits::builder()`, and read through the
accessors. Each ceiling is a typed `Cap` or `CompressionRatio` rather than a raw number with a
sentinel value.

| Accessor | Return type | Default |
|---|---|---|
| `max_total_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_TOTAL_SIZE)` |
| `max_file_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_FILE_SIZE)` |
| `max_compression_ratio()` | `Option<CompressionRatio>` | `Some(1000 / 1)` |
| `max_entry_count()` | `Cap` | `Cap::Limited(DEFAULT_MAX_ENTRY_COUNT as u64)` |
| `max_sfx_payload_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_SFX_PAYLOAD_SIZE)` (reported, not enforced) |
| `reject_unsafe_paths()` | `bool` | `false` (reported, not enforced) |

`max_total_size` bounds the cumulative uncompressed size across all extracted entries;
`max_file_size` bounds any single entry; `max_entry_count` bounds the number of entries.

Two of the six fields are reported but not enforced.

`max_sfx_payload_size()` returns the configured value, and no code path consults it. SFX payload
staging is bounded by the compile-time `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE`, a
`pub(crate)` alias of `DEFAULT_MAX_SFX_PAYLOAD_SIZE` (16 GiB), and the SFX entry points
`Archive::open_sfx`, `Archive::open_with_sfx_progress`, and `Archive::open_at_offset` accept no
`ExtractionLimits` argument at all, so a value set here cannot reach the staging gate. Lowering
the cap does not harden SFX opening and raising it does not permit a larger payload.

`reject_unsafe_paths` reports the configured flag, but the strict-reject extraction path is not
yet wired: the default of `false` preserves the lossy path-repair baseline, and setting it to
`true` records intent without changing extraction behaviour today. The policy behind that is in
[The extraction safety model](../../../explanation/user/en/extraction-safety-model.md).

### Named default constants

Declared in `src/security.rs` and reachable as `unified_archive::security::<NAME>`.

| Constant | Type | Value | Value in bytes |
|---|---|---|---|
| `DEFAULT_MAX_TOTAL_SIZE` | `u64` | `10 * 1024 * 1024 * 1024` | 10 737 418 240 |
| `DEFAULT_MAX_FILE_SIZE` | `u64` | `1024 * 1024 * 1024` | 1 073 741 824 |
| `DEFAULT_MAX_COMPRESSION_RATIO` | `u64` | `1000` | not a byte count; the numerator of a `1000:1` ratio |
| `DEFAULT_MAX_ENTRY_COUNT` | `usize` | `100_000` | not a byte count; an entry count |
| `DEFAULT_MAX_SFX_PAYLOAD_SIZE` | `u64` | `16 * 1024 * 1024 * 1024` | 17 179 869 184 |
| `MAX_COMMENT_SIZE` | `u32` | `64 * 1024` | 65 536 |

`MAX_COMMENT_SIZE` is not part of `ExtractionLimits` and is not read by any code in `src/`. It is
a declared ceiling only: the UnRAR header path never populates a comment buffer, so no comment
length is ever compared against it.

### `unlimited()`

`ExtractionLimits::unlimited()` sets every `Cap` to `Cap::Unlimited` and the ratio to `None`. It
is **`pub(crate)`**: not callable from outside the crate. It is the typed successor to a removed
public constructor that used sentinel numbers, and it exists for the one internal caller that must
bypass every ceiling — the content-multiset digest, which bounds itself by each entry's declared
size instead. There is no single public call that disables every gate; a caller wanting that must
set each ceiling through the builder.

### `Cap`

`unified_archive::Cap` is a single resource ceiling. Derives `Debug`, `Clone`, `Copy`,
`PartialEq`, `Eq`. Two variants: `Cap::Limited(u64)` and `Cap::Unlimited`. It replaces a former
`u64::MAX` sentinel, so "unlimited" is a distinct state rather than an extreme number.

| Method | Signature | Behaviour |
|---|---|---|
| `get` | `const fn get(self) -> u64` | The raw ceiling; `u64::MAX` for `Unlimited`. |
| `to_option` | `const fn to_option(self) -> Option<u64>` | `Some(v)` when limited, `None` when unlimited. |
| `as_usize` | `const fn as_usize(self) -> usize` | The ceiling as `usize`, saturating on 32-bit targets; `usize::MAX` for `Unlimited`. |
| `exceeded_by` | `const fn exceeded_by(self, value: u64) -> bool` | `value > v` when limited; always `false` for `Unlimited`. |
| `is_unlimited` | `const fn is_unlimited(self) -> bool` | `true` only for `Unlimited`. |

`From<u64> for Cap` produces `Cap::Limited(value)`. `Cap::Unlimited` must be written explicitly.
All five methods are `#[must_use]`.

### `CompressionRatio`

`unified_archive::CompressionRatio` is a maximum compression ratio held as an exact rational
`numerator / denominator`, both strictly positive. Derives `Debug`, `Clone`, `Copy`, `PartialEq`,
`Eq`. Both fields are private.

| Item | Signature | Notes |
|---|---|---|
| `new` | `fn new(numerator: u64, denominator: u64) -> Result<Self>` | Returns `ArchiveError::OperationBlocked` with operation `extraction_limits` when either operand is zero. |
| `whole` | `fn whole(ratio: u64) -> Result<Self>` | Equals `new(ratio, 1)`; the shipped default is `1000:1`. |
| `numerator` | `const fn numerator(self) -> u64` | `#[must_use]`. |
| `denominator` | `const fn denominator(self) -> u64` | `#[must_use]`. |

Because the constructors are the only way in and both reject zero, an invalid ratio cannot exist.
The gate compares `uncompressed * denominator > numerator * compressed` in `u128`, so the decision
is exact for every `u64` input; there is no floating-point value anywhere in the comparison. "No
limit" is expressed by `ExtractionLimits::max_compression_ratio()` being `None`, not by an extreme
ratio.

The ratio check treats a zero compressed size with a non-zero uncompressed size as a violation
rather than an undefined ratio to wave through; both zero passes.

### `ExtractionLimitsBuilder`

Returned by `ExtractionLimits::builder()`, which starts from the shipped defaults. Derives `Debug`
and `Clone`. Every method is `#[must_use]`, infallible, and chainable; the only fallible step is
constructing the `CompressionRatio` beforehand.

| Method | Signature | Effect |
|---|---|---|
| `max_total_size` | `(self, cap: impl Into<Cap>) -> Self` | Sets the cumulative-size ceiling. |
| `max_file_size` | `(self, cap: impl Into<Cap>) -> Self` | Sets the single-entry ceiling. |
| `max_entry_count` | `(self, cap: impl Into<Cap>) -> Self` | Sets the entry-count ceiling. |
| `max_compression_ratio` | `(self, ratio: CompressionRatio) -> Self` | Enables the ratio gate at `ratio`. |
| `unlimited_compression_ratio` | `(self) -> Self` | Sets the ratio to `None`, disabling that gate. |
| `max_sfx_payload_size` | `(self, cap: impl Into<Cap>) -> Self` | Records the staged-payload ceiling; behaviour is not wired. |
| `reject_unsafe_paths` | `(self, reject: bool) -> Self` | Records the flag; behaviour is deferred. |
| `build` | `(self) -> ExtractionLimits` | Returns the finished value. Always valid. |

Any ceiling not overridden keeps its default. Because the `Cap` setters take `impl Into<Cap>`, a
bare `u64` and `Cap::Unlimited` are both accepted.

## `EntryFilter`

```rust
pub type EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>;
```

Returning `true` selects the entry. The bound is `FnMut`, not `Fn`, so stateful closures such as
counters, accumulators, and dedup tables work without interior-mutability wrappers. The bound is
`Send` only, not `Send + Sync`, because filters are invoked synchronously during selective
extraction; closures capturing non-`Sync` values are therefore accepted.

A plain `Fn` closure satisfies `FnMut`, so direct `Box::new(closure)` call sites keep compiling.
`Box<dyn Fn>` does not coerce to `Box<dyn FnMut>`, so a caller holding an already-boxed `Fn` must
rebox.

`entry_filter_from_fn` lifts a known-`Fn` closure or function pointer into the type without the
`Box::new` ceremony:

```rust
pub fn entry_filter_from_fn<F>(f: F) -> EntryFilter
where
    F: Fn(&ArchiveEntry) -> bool + Send + 'static;
```

## `ProgressCallback`

```rust
pub trait ProgressCallback: Send {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> std::ops::ControlFlow<()>;
}
```

`processed` is the number of bytes processed so far; `total` is the total to process when known.
Returning `ControlFlow::Continue(())` continues the operation; `ControlFlow::Break(())` cancels
it.

The supertrait bound is `Send` only. Callbacks are invoked through `&mut self` from a single
thread per archive operation, so `Sync` is not required, and closures capturing `Cell` or
`RefCell` need no `Mutex` wrapper.

A blanket implementation covers closures:

```rust
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> std::ops::ControlFlow<()> + Send
```

**Re-entrancy constraint.** The callback must not perform any archive operation — open, list,
extract, create, and so on — from inside `on_progress`. Backends invoke it synchronously while
holding backend-internal state, and for UnRAR the call happens while the process-global UnRAR lock
is held. Re-entering the archiver on the same thread during an UnRAR operation is detected and
rejected, because it would otherwise deadlock that non-reentrant lock and hang every UnRAR
operation in the process.

## `RateLimiter`

Defined in `src/options.rs`. It is not re-exported at the crate root; the path is
`unified_archive::options::RateLimiter`. It throttles progress-callback frequency using
`std::time::Instant`.

| Item | Signature | Behaviour |
|---|---|---|
| `new` | `fn new() -> Self` | Interval of 16 ms, roughly 60 updates per second. |
| `with_interval` | `fn with_interval(interval: Duration) -> Self` | Custom interval. |
| `should_update` | `fn should_update(&mut self) -> bool` | `true` when the interval has elapsed; records the new instant. |
| `should_call` | `fn should_call(&mut self) -> bool` | Alias for `should_update`. |

`Default` equals `new()`. The first call always returns `true`: the last-update instant starts as
`None` and is used as the "first call must pass" flag, so `with_interval(Duration::MAX)` behaves
the same as the default constructor on its first call and never underflows on platforms whose
`Instant` is monotonic from boot.

## `StreamBound`

Defined in `src/streaming.rs`. Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. It selects how
`Archive::extract_to_stream` and `Archive::extract_to_stream_with_options` cap the returned
`StreamingExtractor`. There is no default; the value is a required argument.

| Variant | Cap |
|---|---|
| `DeclaredSize` | Exactly the entry's declared uncompressed size, taken from the preflight listing. Degrades to a ceiling-only cap at the effective entry limit — `min(max_file_size, max_total_size)` — when the entry declares no size. |
| `Cap(u64)` | Ceiling-only at `n` bytes: a caller-chosen budget, not an assertion about the entry's size. |
| `Unbounded` | No caller-chosen output cap; decoded bytes are forwarded verbatim. |

Under `DeclaredSize` and `Cap(n)`, production past the cap surfaces as an
`io::ErrorKind::InvalidData` read error rather than a silent EOF at the cap. Under `DeclaredSize`
with a known declared size, an end of stream *below* the declaration surfaces as a sticky
`io::ErrorKind::UnexpectedEof` read error rather than a short read. Under `Unbounded` a hostile
archive whose decoded size exceeds its declared size can drive a caller-side `read_to_end` to
allocate up to the effective entry limit.

The `ExtractionLimits` size and ratio pre-check runs identically for all three variants. The bound
shapes both the returned reader's cap and the budget the backend may materialize while preparing it
(`min(bound, max_file_size, max_total_size)`), so on ZIP, 7z and RAR — which buffer or stage the
entry first — a `Cap(n)` below the declared size fails at the extract call with
`ArchiveError::OperationBlocked` instead of returning a reader. Memory behaviour per backend is
covered in [Streaming and memory behaviour](../../../explanation/developer/en/streaming-and-memory.md).

## `SfxStagingProgress`

Defined in `src/options.rs`. Opening a self-extracting archive copies the embedded payload into a
temporary file so a normal backend can reopen it at offset zero. This type is the hook that
observes that copy and may abort it. It is passed to `Archive::open_with_sfx_progress`.

| Constructor | Signature | Behaviour |
|---|---|---|
| `new` | `fn new(cb: impl FnMut(u64) + Send + 'static) -> Self` | Observe-only. The closure receives cumulative bytes copied and cannot cancel. |
| `with_cancel` | `fn with_cancel(cb: impl FnMut(u64) -> bool + Send + 'static) -> Self` | The closure receives cumulative bytes copied; returning `false` requests cancellation. |

When the `with_cancel` closure returns `false`, staging aborts and
`Archive::open_with_sfx_progress` returns `ArchiveError::Cancelled { operation: "sfx_staging" }`.
The partial temporary file is dropped automatically. The staged payload is separately bounded by
the crate's built-in 16 GiB ceiling (`DEFAULT_MAX_SFX_PAYLOAD_SIZE`, reached through the
`pub(crate)` alias `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE`), not by any `ExtractionLimits`
value: this entry point takes no limits argument.

## `Password`

Defined in `src/password.rs`. A newtype over a `secstr::SecStr`, which is an implementation detail
that never appears in a public signature. Derives `Clone`, `PartialEq`, `Eq`.

| Item | Signature | Notes |
|---|---|---|
| `new` | `fn new(password: impl Into<String>) -> Self` | The only constructor. |
| `as_str` | `fn as_str(&self) -> &str` | Infallible: UTF-8 validity holds by construction. |

`From<String>`, `From<&str>`, and `From<&String>` are all implemented and all route through
`new`.

Because construction accepts only `String` or `&str`, the stored bytes are always valid UTF-8 —
which is what every wrapped backend expects. The bytes are zeroed when the `Password` is dropped.
`Debug` prints `Password(***)` and `Display` prints `***`; neither ever prints the plaintext.

## See also

- [Format support matrix](../../../reference/user/en/format-support-matrix.md) — which formats
  accept which operation.
- [Errors and warnings](../../../reference/user/en/errors-and-warnings.md) — `OperationBlocked`,
  `Unsupported`, `Cancelled`, and the warning list.
- [Public API surface](../../../reference/user/en/public-api-surface.md) — the export path of each
  type on this page.
- [How to extract an untrusted archive safely](../../../how-to/user/en/extract-untrusted-archives-safely.md)
  — applying `ExtractionLimits`.
- [How to report progress and cancel an operation](../../../how-to/user/en/report-progress-and-cancel.md)
  — applying `ProgressCallback` and `RateLimiter`.
