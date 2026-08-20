---
type: ADR
title: "AD: Entry Metadata Caching"
description: "Accepted; superseded in part by AD 0065 / OI-0065-003 — the OnceCell listing cache still governs but now holds an Arc shared with per-backend memoisation, and list_files_for_limits' CRC-avoidance premise died with MADR-0001 (amended 2026-08-05, §B)."
tags: [decision, ADR-0004]
timestamp: 2026-04-12T00:00:00Z
status: active
---

# AD: Entry Metadata Caching

Status: Accepted; amended 2026-08-05 (decision-review-2026-07-19 §B) — superseded **in part** by AD 0065: the caching intent still governs, but the mechanism is now the frozen-listing baseline with one `Arc` shared between the facade cell and the backend cells (OI-0065-003). See the amendment at the end.

## Context and Problem Statement
Repeated listing calls were expensive and some backends required heavy work for CRC metadata. UnRAR iterators are single-pass, making re-listing particularly costly. Preflight safety checks also need entry metadata but do not require full CRC precomputation.

## Decision Drivers
- Repeated-read performance for consumers that call `list_files()` multiple times
- Preflight scan cost reduction when only entry counts and sizes are needed
- UnRAR iterator exhaustion forcing full re-open on repeated list calls

## Considered Alternatives
- **No caching** -- rejected because it results in repeated expensive scans, especially for RAR archives where the iterator must be re-opened.
- **Fully backend-managed caches** -- rejected because each backend would implement caching with inconsistent semantics and lifetimes.

## Decision Outcome
We decided to cache `list_files()` results with `OnceCell` and introduce `list_files_for_limits()` to avoid heavy CRC precomputation when only limits are needed, because it balances performance with the need for a cheaper preflight path.

## Consequences
- Good: Improves repeated-read performance while preserving a cheaper preflight path for safety checks.
- Bad: Cache freshness is tied to handle lifecycle; some backend-specific edge cases still leak through the caching abstraction.

## Amendment (2026-08-05, decision-review-2026-07-19 §B — superseded in part by AD 0065)

The 2026-07-19 review found this record's Decision Outcome half-stale. It has two limbs — cache
`list_files()` "with `OnceCell`", and "introduce `list_files_for_limits()` to avoid heavy CRC
precomputation" — and only the first still describes the code. **The `OnceCell` limb survives and
was extended, not replaced.** The facade cell is `Archive::entry_cache` in `src/archive.rs`, today
typed `OnceCell<Arc<Vec<ArchiveEntry>>>` and populated by `Archive::list_files`
(`src/inspection.rs`) via `get_or_try_init`; **AD 0065** ("AD 0065: Backend caching baseline —
frozen-view at first use (Option A)") then added a second layer beneath it, so the memoised full listing — not the
per-backend primitive AD 0054 cached — is the canonical unit. Every read backend now carries its own
listing cell of the same type, reached through `ReadBackend::list_files` /
`list_files_budgeted` in `src/backend.rs`:

- `ZipArchive::listing` (`src/ffi/zip_wrapper.rs`), `SevenZArchive::listing`
  (`src/ffi/sevenz_wrapper.rs`), `UnrarArchive::listing` (`src/ffi/wrapper.rs`).
- `LibarchiveArchive::cached_listing` (`src/ffi/libarchive_wrapper.rs`), filled by
  `list_files_metadata_only_budgeted` in `src/ffi/libarchive_wrapper/reader.rs`.

That is **four** read backends, not the "all five" AD 0065 and OI-0065-003 still say: the piz
backend was deleted 2026-07-23 (DCR-009, AD 0007's collapse amendment). This record's UnRAR driver
("iterator exhaustion forcing full re-open on repeated list calls") is likewise now discharged by a
memoised fresh-handle walk rather than by the facade cache alone (DCR-005 / R0079-0001, per AD 0065's
2026-06-11 amendment). OI-0065-003 (**RESOLVED 2026-07-06**, `docs/project/open-issues.md` §
"OI-0065-003: Two-layer caching memory tradeoff (AD 0065)") removed the double-pinning the two
layers first introduced: facade and backend hold the *same* `Arc`, so a listing is pinned once per
handle — the tradeoff is recorded here only for provenance, not as a live cost.

**The cheap-preflight limb no longer holds.** `Archive::list_files_for_limits()`
(`src/inspection.rs`) still exists, but it dispatches through the same `Archive::list_entries` →
`ReadBackend::list_files` walk as `list_files()`, and there is no CRC precomputation left anywhere
on a listing path to avoid: MADR-0001 (legacy "AD 0001" — the id the `src/backend.rs` and
`src/ffi/libarchive_wrapper/reader.rs` rustdoc still cite; today's `AD-0001` is the unrelated
single-archive-facade-with-backend-enum record) routed the public listing through libarchive's
metadata-only walk on 2026-04-14, two days after this record, and the CRC-walking
`LibarchiveArchive::list_files()` it preserved has since been dropped from the wrapper. What
survives of `list_files_for_limits()` is only that it skips the facade cell and returns an owned
`Vec` (one clone for the return contract) —
pinned by `src/inspection/tests.rs::test_list_files_for_limits_does_not_cache`. The internal
preflight moved the opposite way: `Archive::list_files_for_limits_budgeted` delegates to
`Archive::list_files_shared_budgeted` (`src/archive.rs`), which goes *through* `entry_cache` and
returns the shared `Arc`, with a parse-time entry-count budget (OI-0080-003) — not a cheaper
metadata path — doing the preflight-cost work this record assigned to the second limb.

Finally, this record's "Bad" consequence — *"Cache freshness is tied to handle lifecycle; some
backend-specific edge cases still leak through the caching abstraction"* — has been promoted from
a caveat to a stated contract ("frozen at first observation", AD 0065) and the leak it predicted is
now guarded rather than merely acknowledged: the libarchive drift tests
`stale_cached_listing_surfaces_drift_errors` / `stale_cached_listing_surfaces_bulk_drift_errors`
(`src/ffi/libarchive_wrapper.rs`) assert that an on-disk swap after listing surfaces an error
instead of extracting content the safety gate never saw.

**Disposition: ACTIVE, superseded in part.** AD 0065 replaced the *mechanism* and generalised the
contract; it did not reverse the ruling that listings are memoised per handle, and the `OnceCell`
facade cache this record introduced is still the one serving `Archive::list_files()`. Cross-refs:
AD 0065 (baseline + its 2026-06-11 / 2026-07-06 / 2026-07-17 / 2026-07-23 amendments); AD 0054
(whose rejected option 1, "cache only the entry list", was rejected *because* this record had
already done it at the facade); MADR-0001 (the CRC-walk removal that voided limb two); DCR-005;
DCR-009; OI-0065-003; OI-0080-003.
