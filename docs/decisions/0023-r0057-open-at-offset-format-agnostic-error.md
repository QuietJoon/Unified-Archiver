# AD: `Archive::open_at_offset` Returns `NotImplemented`, Not a Forged-Format `Unsupported`

## Context and Problem Statement

Found in Review 0057 (Issue R0057-0013, Severity: Medium).
Location: `src/archive.rs::open_at_offset`.

`open_at_offset` is a deferred feature (DEF-001). The previous implementation returned:

```rust
ArchiveError::unsupported("open_at_offset", ArchiveFormat::Zip, Some("..."))
```

`ArchiveFormat::Zip` is a placeholder — the real archive at the supplied offset might be any format. Callers pattern-matching on the format field would see a false claim.

## Decision Drivers

* Deferred features should use `NotImplemented`, not `Unsupported`. The `Unsupported` variant semantically means "this format cannot do this operation," which is false here — every format could eventually.
* Returning a fake format in a structured error is worse than omitting the format.

## Considered Options

1. Switch to `ArchiveError::not_implemented("open_at_offset", "DEF-001 deferred…")` with no format field.
2. Keep `Unsupported` but change the format to `Option<ArchiveFormat>::None`.
3. Remove the function entirely until implemented.

## Decision Outcome

**ACCEPT Option 1.** The error is now `NotImplemented`, matching the variant split in commit `c9b6685`. The updated library test (`test_open_at_offset_returns_not_implemented`) asserts the new variant. Option 3 would break the SFX workflow that already references this API.

Status: Implemented.

### Implementation

- `src/archive.rs::open_at_offset` — returns `ArchiveError::not_implemented("open_at_offset", "Offset-based archive opening is deferred (DEF-001). Use extract_stub() to materialize the embedded archive as a temporary file.")`.
- `src/archive.rs` test `test_open_at_offset_returns_unsupported` → `test_open_at_offset_returns_not_implemented`.

## Consequences

* Good, because callers can now reliably pattern-match `NotImplemented { feature: "open_at_offset", .. }` to detect the deferral.
* Good, because no fabricated format leaks into error telemetry.
* Neutral — the caller-facing message is strictly more honest.
