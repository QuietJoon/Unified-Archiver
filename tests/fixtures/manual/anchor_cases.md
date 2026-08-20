---
type: Reference
title: Anchor cases
description: Fixture exercising the manual-conformance extractors; not part of the manual bundle.
tags: [fixture]
audience: developer
language: en
generated:
  by: hand
  at: 2026-08-16T00:00:00Z
sources:
  - { id: manifest, resource: Cargo.toml }
synced_hash: 0000000000000000000000000000000000000000000000000000000000000000
---

# Anchor cases

This fixture is read by `tests/manual_conformance.rs`. It is deliberately *not*
part of the `manual/` bundle, so the live checks never see it.

## A span that wraps across a line break

The reference page quotes `encrypted creation for {:?} is not supported
(MADR-0027); password must be None`, and the quote breaks mid-sentence in the
source markdown. A line-oriented extractor misses it entirely.

## A true file-scoped claim

`Cargo.toml` still says `name = "unified-archive"`, which is the shape of claim
that check (a4) verifies against the named file.

## A false file-scoped claim

`Cargo.toml` still says `name = "unified-archive-that-does-not-exist"`, which is
the same shape with a quotation the file does not contain. Check (a4) must
report exactly this one.

## A path that is not a repo path

The tutorial writes `notes/todo.txt` for a directory the reader creates. Its
parent does not exist in this repo, so check (a3) skips it rather than flagging
it.
