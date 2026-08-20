---
type: Tutorial
title: Broken snippet
description: Positive control for the snippet lane; must fail to type-check.
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

# Broken snippet

This block reproduces the historical defect the snippet lane exists to catch:
a boxed closure whose parameter types cannot be inferred, which rustc rejects
with `error[E0282]`. If `tests/manual_snippets.rs` ever reports this file as
clean, the lane has degraded into a no-op.

```rust
use std::ops::ControlFlow;

fn main() {
    let cb = Box::new(|progress| ControlFlow::Continue(()));
    let _ = cb;
}
```

This one is declared incomplete and must never be compiled:

```rust,fragment
let archive = Archive::open("example.zip")?;
```
