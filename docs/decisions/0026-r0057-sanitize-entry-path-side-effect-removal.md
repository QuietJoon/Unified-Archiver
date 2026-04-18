# AD: `sanitize_entry_path` No Longer Creates Directories

## Context and Problem Statement

Found in Review 0057 (Issues R0057-0009, R0057-0010, R0057-0011, Severity: Medium).
Location: `src/security.rs::sanitize_entry_path`.

The previous implementation called `std::fs::create_dir_all(parent)` before canonicalizing the parent path. That was a hidden write-to-disk effect inside a function named "sanitize," which violated caller expectations and could leave orphan directories behind if any later step failed.

The companion `validate_entry_path` was already documented as side-effect-free, so the asymmetric behavior was also confusing.

## Decision Drivers

* A sanitizer should not mutate the filesystem.
* The existing symlink check below the removed block still needs a canonical base to compare against; we cannot drop canonicalization entirely.
* Extraction callers (e.g., `create_output_file`) already `create_dir_all` their target directory, so the implicit mkdir was redundant on the happy path.

## Considered Options

1. Keep the current function but remove `create_dir_all`; walk up to the deepest existing ancestor and canonicalize that for symlink-escape detection.
2. Move the parent-creation responsibility into a new helper `prepare_extract_dest(entry_path, dest)` and update every extraction backend.
3. Replace canonicalization entirely with a lexical `starts_with` check on the joined path. Drops the defense against pre-placed symlinks inside the destination.

## Decision Outcome

**ACCEPT Option 1.** The function now walks up `full_path`'s parents, finds the first existing one, canonicalizes it, and asserts it is within `canonical_dest`. Attackers cannot escape via a symlink the archive itself tries to plant, because `normalize_entry_components` strips traversal components before the join happens. Attackers also cannot escape via a symlink that already exists in `dest`, because the ancestor canonicalization still catches it. The only case we lose is "the archive planted a symlink in a new intermediate directory" — but we explicitly reject symlink entries elsewhere (AD 0021), so this case is moot.

Status: Implemented.

### Implementation

```rust
if let Some(parent) = full_path.parent() {
    let mut existing: Option<&Path> = None;
    let mut candidate: Option<&Path> = Some(parent);
    while let Some(p) = candidate {
        if p.exists() { existing = Some(p); break; }
        candidate = p.parent();
    }
    if let Some(p) = existing {
        let canonical_ancestor = p.canonicalize().map_err(..)?;
        if !canonical_ancestor.starts_with(&canonical_dest) {
            return Err(ArchiveError::InvalidPath { .. });
        }
    }
}
```

## Consequences

* Good, because the function is now honestly side-effect-free.
* Good, because extraction callers (which already mkdir) are unaffected.
* Bad, because any caller that relied on the implicit mkdir must now create dirs themselves. Mitigation: audit in OI-0057-004 confirms the known extraction paths already do this; integration tests will catch any straggler.
