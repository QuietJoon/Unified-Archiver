# Architecture Decisions

This directory contains architecture decision records for unified-archive.

Records 0001-0010 are **durable architecture decisions** captured during the
initial design investigation. They document foundational choices about API
shape, backend strategy, safety boundaries, and concurrency model.

Records 0011-0018 are **review-driven change records** created during code
reviews. Each one captures an implementation fix that carried architectural
rationale worth preserving (e.g., correcting a concurrency assumption, changing
a public-facing semantic).

Both types are kept together so that the full decision trail -- from initial
design through review-driven refinement -- is traceable in one place.

| # | Title | Type |
|---|-------|------|
| 0001 | Single Archive Facade with Backend Enum | Architecture |
| 0002 | Hybrid Backend Selection | Architecture |
| 0003 | Safety Gates Before Extraction | Architecture |
| 0004 | Entry Metadata Caching | Architecture |
| 0005 | Per-Entry Reopen for Parallel Extraction | Architecture |
| 0006 | Staged SFX Detection Pipeline | Architecture |
| 0007 | Dual ZIP Backend Strategy | Architecture |
| 0008 | Split Archive Behavior Across Modules | Architecture |
| 0009 | Isolate Unsafe Behind Safe Wrappers | Architecture |
| 0010 | Unified Stream Checksum Extraction | Architecture |
| 0011 | Fix solid-archive parallelism check | Review-driven |
| 0012 | Treat CRC32 value zero as valid | Review-driven |
| 0013 | Plumb compression levels to backends | Review-driven |
| 0014 | Reject deferred password validation | Review-driven |
| 0015 | SFX iterate all candidates, demote CD signature | Review-driven |
| 0016 | Rename ShellScript, add Unknown stub variant | Review-driven |
| 0017 | Archive creation rejects existing files | Review-driven |
| 0018 | Remove standalone gz/bz2/xz support claims | Review-driven |
| 0019 | UnRAR FFI calls serialized behind a process-wide mutex | DCR-001 (Phase A.2) |
| 0020 | `Archive::modify_with_options` is an additive extension | DCR-001 (Phase B.2) |
| 0021 | Creation-side progress callbacks fire per entry | DCR-001 (Phase B.1) |
