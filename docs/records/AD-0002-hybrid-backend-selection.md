---
type: ADR
title: "AD: Hybrid Backend Selection"
description: "Accepted"
tags: [decision, ADR-0002]
timestamp: 2026-04-12T00:00:00Z
status: active
---

# AD: Hybrid Backend Selection

Status: Accepted

## Context and Problem Statement
No single backend met all requirements. UnRAR is needed for RAR/RAR5 support, libarchive provides broad format coverage and read-write capability, and native Rust crates (piz, sevenz-rust2, zip) offer better performance and metadata quality for specific formats.

## Decision Drivers
- Format coverage across RAR, 7z, ZIP, tar, and compressed streams
- Performance characteristics (especially for ZIP and 7z)
- Metadata quality and completeness

## Considered Alternatives
- **libarchive-only** -- rejected because of RAR constraints and metadata limitations that reduce extraction fidelity.
- **UnRAR-only extension model** -- rejected because the format scope is too narrow to serve as a general-purpose archive library.

## Decision Outcome
We decided to combine UnRAR (RAR/RAR5), libarchive (broad format fallback and read-write), and native Rust backends (piz, sevenz-rust2, zip writer/reader) because it maximizes format coverage while letting each backend contribute its strongest capability.

## Consequences
- Good: Improves capability coverage and performance/metadata quality where native crates are stronger.
- Bad: More integration surfaces, duplicated extraction logic patterns, and backend inconsistency risks that require careful testing.

## Amendment (2026-07-23, R4 collapse — piz removed from the hybrid)

Per the AD 0007 collapse amendment (owner-approved; paired DCR-009), the `piz` backend is removed
from the hybrid. The native-Rust ZIP contribution to the mix is now the `zip` crate alone, serving
**both** encrypted and unencrypted ZIP; `Archive::open` on a ZIP builds `ArchiveBackend::ZipReader`.
The hybrid roster is unchanged otherwise: UnRAR (RAR/RAR5), libarchive (broad-format fallback and
read-write), sevenz-rust2 (7z), and the `zip` crate (ZIP read/extract/create). This does not rewrite
the Decision Outcome above; it records that "native Rust backends (piz, sevenz-rust2, zip …)" now
reads "(sevenz-rust2, zip …)". Removing the second ZIP reader also retires one of the
backend-inconsistency risks the original "Bad" consequence warned about (the piz-vs-zip divergence
class). See AD 0007 (2026-07-23 amendment) and DCR-009.
