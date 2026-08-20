---
type: Design Review
title: "ADR / DCR design review — revert / improve / innovate"
description: "Cross-cutting critique of all 93 decision records (57 architecture ADs + 30 review-gate MADRs + 8 DCRs), prioritising reversible decisions. Design-only; no code changes made."
tags: [design-review, architecture, adr, dcr]
timestamp: 2026-07-19T00:00:00Z
status: advisory
---

# Decision-record design review (2026-07-19)

Scope: every ADR/MADR/DCR as a **design artifact**, at two zoom levels (small = record integrity;
big = is the decision still right). Reviewed by eight parallel read-only reviewers. **No code or
records were changed** — this is an advisory synthesis. Priority order per the request:
**revert candidates → improvements → innovations.**

Headline counts across 93 records: ~30 KEEP, ~45 IMPROVE (mostly stale/superseded records that
never got their status flipped), **8 REVERT-CANDIDATES**, and a family of INNOVATE targets that
cluster into four concrete redesigns.

One meta-finding threads through everything: a large share of the IMPROVE load is not bad
decisions but **records that shipped `status: active` and were then silently superseded** by later
work — the decision system has no supersession-propagation, so reviewers (including the R0080/R0081
agents) keep re-litigating or missing prior rulings. That is itself the #1 revert candidate below.

---

## A. REVERT CANDIDATES (do these first — reversible, decision-level)

Ranked by leverage-to-cost. "Revert" here means *reverse or formally supersede the decision*, not
necessarily rip out code.

### R1 — The multi-store record system (Cluster H, META)
**Reverse going forward.** There are four overlapping record stores with **colliding numbering**:
`docs/architecture/decisions` (ADs 0001–0067, with unexplained gaps 0022–0028/0032/0034/0043/0045/0048
and an `archive/` subdir), `docs/decisions` (30 MADRs whose 0001–0030 numbering *collides* with AD
numbers — "0013" names two different decisions by directory), `reviews/{Decisions,Ignores}.md`
(legacy DD-*/IG-* logs *still receiving* new IG-0081-* entries), and `docs/project/design-change-records`.
Plus the OI tracker and a TicGit mirror. This is the direct cause of the Review 0080 reviewer citing
"None" for a decision that existed in `Ignores.md`, and of repeated mis-citation.
- **Revert:** freeze the four stores; route all *new* records into one typed, monotonic namespace with
  a machine-readable index (`docs/records/index.yaml`: id → {type, status, supersedes, amends, reviews, ois}).
- **Cost:** an index generator + one line of frontmatter on ~100 existing records — mechanical, **no
  body rewrites** (records stay immutable per policy). **Buys:** end of collisions, phantom rows, and
  reviewer misses. Difficulty: medium.
- **Cheap sub-revert (do immediately):** stop writing new `IG-*`/`DD-*` entries to `reviews/` — the
  AD↔IG double-bookkeeping is the specific mechanism of the 0080 miss. Near-zero cost.

### R2 — AD 0027: permanent rejection of encrypted-archive creation (Cluster E, #1 in cluster)
**Reverse "permanent" → "deferred behind an explicit opt-in."** The record's core premise — that
owning crypto is a large commitment and no backend exposes creation encryption — is **factually
wrong today**: the `zip` crate (AES already enabled in the tree) and `sevenz-rust2` (`aes256` +
header encryption, already a dependency) own the crypto and cover ZIP *and* 7z. The read side
already parses the harder ciphertext surface. For a Win/macOS/Linux archiver in 2026, users expect
password-protected zips.
- **Cost:** moderate, well-scoped (wire the existing backend crypto behind an opt-in creation flag;
  honest counter-cost = AE-2 tool-compatibility support burden).
- **Note the reversal chain this untangles:** gate-0007 *accepted* ZIP AES-256 creation → gate-0022
  tracked its regression → AD 0027 rejected it permanently. Three records point in two directions;
  reverting AD 0027 to "deferred opt-in" makes the chain coherent.

### R3 — AD 0005: per-entry reopen "run extraction in rayon for batches of 4+" (Clusters A & D — both flagged independently)
**Already de-facto reverted; the record lies.** rayon is **not a dependency** and no internal
parallel-extraction path exists in `src`. What ships is `Archive: Send + !Sync` with the *caller*
spawning threads (one handle each). The record advertises a feature the library does not have.
- **Cost: near-zero** (nothing to un-ship). Rewrite the Decision Outcome to the real contract, or
  record why internal rayon was dropped (almost certainly the record's own "open/close I/O overhead +
  weaker progress" bullet). **AD 0011** (fixed a parallel branch AD 0029 later deleted) is moot for the
  same reason — banner both as superseded-by-0029 in one edit, and add the rayon-removal line AD 0029
  currently omits from its Consequences.

### R4 — AD 0007: dual ZIP backend strategy (Clusters A & F)
**Benchmark-gated revert to a single `zip`-crate backend (or feature-gate piz).** The recorded "Bad:
*slight* behavioral differences" has become a divergence *class*: duplicate-name dedup (zip crate
last-record-wins vs piz rejects, R0079-0026), special-type mapping splits (R0081-0076/0077, just
fixed by a shared classifier), and the piz mmap SIGBUS/silent-mutation hazard the File-reading `zip`
crate doesn't have (OI-0080-002). Two backends for one format also doubles the single-entry defense
surface (OI-0076-002).
- **Cost:** blocked only on a throughput benchmark. If piz's mmap edge is modest, collapsing to `zip`
  (optionally over a self-managed mmap `Cursor` to keep speed) erases the whole divergence class.
  If the edge is large, feature-gate piz so the default surface is single-backend.

### R5 — AD 0056: defer the libarchive reader/writer split (D9) (Cluster F)
**Reverse the deferral.** The blocking premise — "D2 will reshape the backend enum first" — was
falsified when D2 shipped as *additive* typed handles over the unchanged `ArchiveBackend`. Meanwhile
`libarchive_wrapper.rs` grew to ~3,775 LOC (2.36× its size at deferral), and three consecutive
reviews landed fixes in that one file. D9 is unblocked and only gets costlier to defer.
- **Cost:** a mechanical reader/writer type split, now with no D2 dependency.

### R6 — AD 0029 (gate, R0060): reject the scratch-path portability sweep (Cluster G)
**Revert — the premise is dead.** "CI is out of scope" no longer holds (first-class platforms +
committed Windows CI), and its examples clause was *already* factually reversed by AD 0041. 21
hardcoded test-path literals remain.
- **Cost:** low-moderate — route the 21 literals through `temp_test_dir()` (AD 0036 Option 3) and
  consolidate the 0036/0041/0029-gate scratch-path triangle into one policy record. Unblocks CI.

### R7 — AD 0010: unified stream-checksum extraction (Cluster C)
**Reverse the *framing*, not necessarily the code.** The module is marketed as an integrity feature
but only *reads stored* checksums without verifying, XZ is permanently half-built (check-type only,
no value), it has **zero internal consumers**, and reviews keep landing correctness patches on it
(bzip2 bit-scan in R0079/R0080). It's an attractive nuisance.
- **Cost:** narrowing to "read stored checksum" (finish or cut XZ) is cheap; full public removal is
  breaking and needs a deprecation cycle. Alternatively *promote* it to a real `verify_stream()` with
  actual recomputation — but only if a consumer exists.

### R8 — gate-0015 (R0002): deferred SFX "confirmed-detection" patterns (Cluster C)
**End the 3-month limbo.** The blocker (DEF-001) is gone and `open_at_offset` now works. Either
implement the confirmed-detection pattern **or** collapse the confidence float to a
`{NotSfx, Probable, Confirmed}` + evidence enum (which also retires gate-0014's clamp). Currently a
documented-but-unimplemented decision sitting `active`.

---

## B. TOP IMPROVEMENTS (record integrity / premise-refresh — non-reverting)

These are decisions that are *right* but whose records are stale, over-claim closure, or were
silently superseded. Grouped; each is a status/wording amendment, not a design change.

**Stale-status / supersession flips (highest churn, lowest risk):**
- **AD 0050** — record claims a "limits-only, password-ignored" stream contract that the code (R0072-0002)
  and its own cited rustdoc now contradict. Actively misleading; cheapest high-value fix in Cluster B.
- **gate-0007 / gate-0022** — mark superseded-by-AD-0027; strike the false "Implemented + tests" claim;
  reconcile "regression" vs "never realized."
- **gate-0011 / gate-0012** — both superseded (the ArchiveError variant split landed the Option 2
  gate-0011 rejected; gate-0024 replaced gate-0012's error variant). Still `active` on stale claims.
- **gate-0023 + AD 0006** — `open_at_offset` is implemented; mark gate-0023 Superseded, fix AD 0006's
  "unimplemented" consequence.
- **AD 0018** — text is factually false (standalone gz/bz2/xz read/extract shipped per gate-0019);
  re-scope to creation-only.
- **AD 0004 / AD 0054** — mark superseded-in-part by AD 0065; record the listing-cache-over-primitive-cache
  choice for iterator backends.
- **AD 0011 / gate-0016** — code no longer matches (gate-0016's "same modify handle" claim is false after
  a separate encryption probe was reintroduced); amend.
- **gate-0014** — clamp is `[0.5, 0.99]` and `is_confirmed` is `>0.99`+structural now; record still says
  `[0.0, 0.99]` / `==1.0`.
- **DCR-002** — inverted AD 0014 citation ("validate at the gate, don't defer" is the opposite of what
  AD 0014 decided). Fix the reference.

**Premise-inverted by the 2026-07-17 first-class-Windows directive:**
- **AD 0046 / AD 0049** — both reject records predate the directive; the "not release-verified" wording
  AD 0046 defended is now the stale artifact, and AD 0049's Windows-rename-retry revisit trigger is near
  firing. Add supersession/amendment notes; cross-link OI-0065-001.
- **AD 0020** — default-on `rar-support` + the Windows `build.rs` `panic!` means the **default build fails
  on a first-class platform**. Make Windows `rar-support` a warn+no-op (RAR → `UnsupportedOperation`), or
  drop it from `default`. (This is arguably a correctness bug in the decision, not just the record.)

**Over-claimed closure / orphaned deferral targets:**
- **AD 0064** — downgrade "closes the asymmetry" to the read/`add_file_from_path` scope actually shipped;
  forward-link the still-open OI-0076-001 (12 sites + missing raw-byte selective surface).
- **AD 0052** — the deferral target was orphaned: D1 shipped without the promised `validate()` hook.
  Land it or formally retire it ("first-op validation + AD 0065 listing cache is the harmonised contract").
- **AD 0053** — reconcile D2/D3/D4/D9/D10 statuses (part-implemented, part-diverged, part-abandoned);
  record the D4 primitive-cache → listing-cache pivot explicitly.
- **AD 0055** — `ValidatedSource` does not enforce its named invariant; R0070-0008 is still live in the
  code. Converge it onto the working `ValidatedEntry`/by-listing-id token.
- **AD 0058** — re-scope: commit Stage 1 features *before* the v0.4 `v2-api` default-flip; downgrade
  Stage 2 facade crates to "revisit only if measurement proves features insufficient." Three months
  stale, zero progress, surface hardening around it.
- **AD 0067 + closure-record family (0051/0059/0060)** — the omnibus "one AD records N dispositions"
  form fails citability at wide-review volume (AD 0061 is the control case: fine when scope is bounded).
  Also: AD 0051/0059 contain a dangling **out-of-repo** forward-work pointer
  (`/Users/sg/.claude/plans/...`) that violates the durable-on-disk rule.

**Wrong-primitive (record right, mechanism improvable):**
- **AD 0017** — swap the racy `path.exists()` creation gate for atomic `O_EXCL`/`CREATE_NEW`.
- **AD 0009 (gate)** — `fs2` is unmaintained; move to `fs4` (drop-in). Also reframe the 2026-07-17
  amendment as an accepted contract, not a stopgap.
- **AD 0021 (gate, R0056)** — false "ZIP has no symlinks" premise (ZIP stores them via Unix attrs);
  rebase on FR-022 policy.
- **AD 0047** — label the digest non-cryptographic; guard the O(N²) TAR path (pair with OI-0080-003);
  name the AD 0012 `Some(0)`-escapes-recompute seam.

---

## C. INNOVATIONS (decision sound; a better design is now available)

Four of these recur across multiple clusters — those are the real signal.

### I1 — Typed `ExtractionLimits` redesign (Clusters A, B, C, F converge here)
Replace the flat struct of `f64` ratio + `f64::MAX`/`unlimited()` sentinels with private fields +
validating builder and a `Cap { Limited(u64), Unlimited }` per-field type plus an integer/rational
ratio. This **structurally retires** the `f64::MAX`-vs-`INFINITY` sentinel hazard (hand-patched in
R0080-0006) and the 2^53 precision cliff (hand-patched in R0081-0024), and folds in `max_sfx_payload_size`
(AD 0040's bare const) and `reject_unsafe_paths` (AD 0066's deferred flag) as first-class validated
fields. Subsumes OI-0076-005. Cheap pre-1.0.

### I2 — Collapse the stream bounded/unbounded variant set (Clusters A/B, DCR-006 territory)
The 4-method stream surface produced the R0081-0027 "two unbounded APIs disagree" drift. Replace with
one bounded entry point taking an explicit typed bound —
`enum StreamBound { DeclaredSize, Cap(u64), Unbounded }` — so "unbounded" is chosen in exactly one
place and the drift becomes structurally impossible. Align the memory side to the same descriptor.

### I3 — Collapse SFX detection to enum + evidence (Cluster C, spans AD 0006/0015, gate-0014/0015)
The float confidence provably carries no information (only 0.0 and ~0.9 appear in production). Replace
`SfxDetectionResult`'s float with `{NotSfx, Probable, Confirmed}` + an evidence list. Retires gate-0014,
resolves gate-0015, and simplifies the staged pipeline (AD 0006). Pair with the typed multipart
`VolumeSet` parser already tracked as OI-0080-004 (stop the third heuristic round).

### I4 — `cc`-crate UnRAR build (Cluster D — single highest-leverage change there)
Replace the vendored `make`-based builder (AD 0039) with a `cc`-crate-driven build of the *same*
source (mirroring the `unrar_sys` crate). Delivers native Windows RAR **and** cross-compilation,
retires the staging/artifact-purge machinery, and **resolves AD 0020's default-build problem and
OI-0080-001 at once** — without touching the concurrency stack. Explicitly *not* the out-of-process
worker (that dissolves the mutex/sentinel/trampoline but contradicts the library-only non-goal and
isn't justified while RAR is read-only and internal parallelism is gone).

**Other single-record innovations:**
- **I5 — `Password` newtype** (AD 0042): crate-owned, SecStr-backed, UTF-8-validated at construction.
  Validation moves to the gate, the fallible `password_as_str` accessor and its ~17 `?` sites disappear,
  and `secstr` stops leaking through two `pub` fields. Natural fit with OI-0076-005.
- **I6 — Owned-fd / `openat` hand-off to libarchive** (DCR-007 / AD 0009): replace the seven by-name
  identity-revalidation sites with a single descriptor hand-off, retiring the whole revalidation class —
  and the read-side OI-0081-001 and the Windows identity gap — at once.
- **I7 — `#[expect(dead_code)]` per item** (gate-0018): replace the module-level `#![allow(dead_code)]`
  with per-item `#[expect]` (Rust 1.85), which *enforces* the "periodic audit" the record only recommends
  and self-heals as the AD 0058 feature-split lands.
- **I8 — Loud codec-option failure** (AD 0013): surface a warning / `CodecUnavailable` when libarchive
  refuses a compression-level string, instead of silently swallowing it (aligns with the crate's
  loud-failure posture elsewhere).

---

## D. What to keep untouched
Sound and well-maintained: AD 0002 (hybrid backends), AD 0009 (unsafe isolation), AD 0012 (CRC-0 valid),
AD 0015 (SFX candidate iteration), AD 0019 + DCR-008 (UnRAR mutex + sentinel), AD 0031/0036/0037,
AD 0061 (model closure record), AD 0065 *core* decision, DCR-001/003/005, gate-0004/0025/0030 (0030 is
cited as a model record). These need no action.

---

## Suggested sequencing
1. **Free, high-integrity now:** R3 (AD 0005 rewrite) + R1 cheap sub-revert (stop new IG/DD writes) +
   the stale-status flip batch in §B — pure record hygiene, no code, ends the reviewer-miss problem.
2. **Decision reversals needing owner sign-off:** R2 (encrypted creation), R4 (dual-ZIP benchmark),
   R6 (scratch-path CI), R8 (SFX confirmed) — each is a product/design call.
3. **Unblocked refactors:** R5 (D9 split), I4 (`cc` UnRAR build — also fixes AD 0020 + OI-0080-001).
4. **Pre-1.0 API innovations:** I1 (typed limits), I2 (stream bound), I5 (Password newtype) —
   best landed before v0.4 hardens more surface.

All findings are design-level and advisory; no records or code were modified by this review.
