---
type: Taxonomy
title: "Documentation Taxonomy"
description: "Approved vocabulary and directory governance for the staged OKF v0.2 migration."
tags: [project-control]
generated:
  by: unknown/unknown
  at: 2026-08-04T23:07:37+09:00
status: draft
---

# Documentation Taxonomy

## Purpose

This document records the vocabulary and directory policy approved for the staged OKF v0.2 migration. It currently covers the C1, C2, C3, and C4 migration batches, preserved legacy types and tags, and the existing directory scheme. Later batches may expand it only through the same review and approval process.

## Types

### `ADR`

- **Meaning:** A design decision and its rationale.
- **Include:** Existing AD and MADR decision records whose bodies state a decision or rationale.
- **Exclude:** Design change records and general analysis.
- **Aliases:** `AD` and `MADR` remain filename and presentation conventions; frontmatter uses `ADR`.
- **Initially affected:** `records/AD-0003-safety-gates-before-extraction.md`, `records/AD-0009-isolate-unsafe-behind-safe-wrappers.md`, `records/AD-0014-reject-deferred-password-validation.md`, and `records/AD-0040-sfx-payload-size-ceiling.md`, followed by approved ADR batches.
- **Verification:** Confirm that the body is a decision record before applying the type.

### `Taxonomy`

- **Meaning:** A concept that governs bundle vocabulary and classification.
- **Include:** This approved taxonomy document.
- **Exclude:** Indexes and general reference documents.
- **Initially affected:** `project/taxonomy.md`.

## Categories

### `security-safety`

- **Meaning:** The primary domain for hostile input, resource bounds, path safety, secret or password boundaries, and damage containment.
- **Include:** Concepts whose main concern is preventing unsafe archive handling or limiting its effects.
- **Exclude:** General correctness or performance work without a material safety impact.
- **Boundaries:** Keep API-contract concerns and runtime-architecture concerns separate when those become approved categories.
- **Aliases:** Merge `security`, `safety`, and `security-policy` into `security-safety` if encountered and approved.
- **Initial mapping:** An absent legacy category maps to `security-safety` for `records/AD-0003-safety-gates-before-extraction.md` in the C1 batch, `records/AD-0014-reject-deferred-password-validation.md` in the C3 batch, and `records/AD-0040-sfx-payload-size-ceiling.md` in the C4 batch.
- **Confidence:** High.
- **Verification:** Compare the concept with current security policy, implementation, and tests.

### `ffi-safety`

- **Meaning:** The primary domain for native FFI boundaries, unsafe Rust encapsulation, raw pointer and C-string handling, native handle ownership and release, C-to-Rust error translation, and hiding native implementation details from public APIs.
- **Include:** Concepts whose main concern is safely containing native or unsafe interoperability behind auditable Rust interfaces.
- **Exclude:** Hostile archive input policy, path traversal, passwords, and extraction resource limits, which belong to `security-safety`; also exclude general backend architecture without a material FFI or memory-safety concern.
- **Boundaries:** `ffi-safety` governs soundness and native interoperation boundaries; `security-safety` governs unsafe input and damage-containment policy.
- **Aliases:** Merge `native-safety`, `unsafe-boundary`, and `ffi-boundary` into `ffi-safety` if encountered and approved.
- **Initial mapping:** An absent legacy category maps to `ffi-safety` only for `records/AD-0009-isolate-unsafe-behind-safe-wrappers.md` in the C2 batch.
- **Confidence:** High.
- **Verification:** Inspect external-function declarations, unsafe call sites, wrapper APIs, native-resource release paths, error translation, and orchestration callers.

## Tags

- `decision`: Area tag for ADRs that record a decision and rationale; exclude DCRs and general change tracking.
- `ADR-0003`, `ADR-0009`, `ADR-0014`, `ADR-0015`, and `ADR-0040`: Traceability tags used only when the corresponding ADR identity or explicit cross-reference appears in the record.
- `R0025-0029`, `R0025-0030`, and `R0062-0004`: Review-finding traceability tags used only when the corresponding review finding appears in the record.
- `project-control`: Area tag for taxonomy and documentation-governance concepts.

## Directory Scheme

- `/records/` retains consolidated ADR and DCR records; frontmatter `type` distinguishes them. No record move is approved in the C1, C2, C3, or C4 batches.
- `/project/` contains project-control and taxonomy concepts.
- `/architecture/` contains architecture concepts.
- Existing moved indexes remain reserved indexes for now. No concept-identifier move is approved in the C1, C2, C3, or C4 batches.

## Migration Governance

- Legacy `status: active` to v0.2 `status: draft` is a lifecycle reset, not a taxonomy mapping.
- `generated.by: unknown/unknown` is the migration sentinel for unattested legacy-derived production. It is not verification evidence and must not appear in `verified`.
- Taxonomy changes require staged human approval and same-batch updates to affected indexes and the documentation log.

## Initially Governed Concepts

- [AD: Safety Gates Before Extraction](../records/AD-0003-safety-gates-before-extraction.md)
- [AD: Isolate Unsafe Behind Safe Wrappers](../records/AD-0009-isolate-unsafe-behind-safe-wrappers.md)
- [AD: Reject proposal to validate passwords at open time](../records/AD-0014-reject-deferred-password-validation.md)
- [AD 0040: Cap SFX payload size before copying to disk](../records/AD-0040-sfx-payload-size-ceiling.md)
