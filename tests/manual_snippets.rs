//! Type-checks the Rust snippets in the manual bundle (ticgit c8d771eb,
//! check (c), expensive half).
//!
//! The cheap, syntactic half of check (c) — "every fence carries a known
//! marker" — lives in `tests/manual_conformance.rs` and runs on every
//! `cargo test`.
//!
//! This target is split the same way, and **not** everything in it is
//! `#[ignore]`d:
//!
//! * Exactly the two tests that shell out to `rustc` are ignored, because a
//!   compiler invocation is far too expensive for the default gate:
//!   [`checked_snippets_type_check`] and
//!   [`broken_snippet_fixture_is_rejected`].
//! * The other four are **not** ignored and run on every `cargo test`:
//!   [`collection_is_not_vacuous`], [`fragment_blocks_are_not_compiled`],
//!   [`control_fixture_shape_is_intact`] and
//!   [`fragment_allowlist_is_not_stale`]. They parse the bundle and the
//!   control fixture without starting a compiler, so the default run goes red
//!   when the collection collapses, when a fence escapes both the compiled set
//!   and the allowlist, when the allowlist grows far enough to swallow the
//!   lane, or when the positive control's fixture is gutted.
//!
//! What the default run deliberately cannot tell you is whether the snippets
//! actually compile. Only the `--ignored` run answers that.
//!
//! Run the expensive half deliberately:
//!
//! ```text
//! cargo build
//! TMPDIR=/Volumes/Temp/claude cargo test --test manual_snippets -- --ignored --test-threads=4
//! ```
//!
//! ## Design notes
//!
//! * **One rustc invocation, not one per snippet.** Every checked block is
//!   emitted into a single generated file as `mod sNNN { ... }`. Measured on
//!   this tree, an individual `rustc --emit=metadata` against the built rlib
//!   costs tens of seconds, so 130 of them would take long enough that nobody
//!   would ever run the lane. The batch's diagnostics are mapped back to the
//!   citing page through the generated file's line numbers, so attribution
//!   stays exact without a second pass.
//! * **`rust` is compiled; `rust,fragment` never is.** A fragment is declared
//!   incomplete by its author. Compiling one would produce noise that trains
//!   people to ignore this lane.
//! * **The profile directory comes from `current_exe()`.** The test binary
//!   lives at `<target>/<profile>/deps/<name>-<hash>`, so its ancestors give
//!   the profile directory whether the target dir is the default `target/` or
//!   the absolute `build.target-dir` this repo pins in `.cargo/config.toml`.
//!   `CARGO_TARGET_DIR` is never read, never set, and never redirected.
//! * **Missing rlib fails, it does not skip.** A snippet lane that silently
//!   no-ops is worse than no lane at all.
//! * **The positive control runs whenever the compiled lane runs.**
//!   [`broken_snippet_fixture_is_rejected`] still exists as its own focused
//!   test, but it is a thin wrapper: the work lives in [`positive_control`],
//!   which [`checked_snippets_type_check`] calls *before* it trusts its own
//!   green. A control that only fires when somebody remembers to name it
//!   does not protect the run that matters, and the two tests are ignored
//!   together, so a filtered `--ignored` run could previously type-check 31
//!   snippets while never proving the compile step can still go red.
//! * **Floors, not non-emptiness.** The compiled lane asserts numeric floors
//!   ([`MIN_CHECKED_SNIPPETS`], [`MIN_RUST_FENCES`]) and a ceiling on how
//!   much [`FRAGMENT_ALLOWLIST`] may absorb ([`MAX_UNMARKED_FRAGMENTS`],
//!   [`MAX_ALLOWLISTED_BLOCKS`]), in the same style as
//!   `tests/manual_conformance.rs`. A `!is_empty()` check passes with one
//!   surviving snippet, so an allowlist that quietly grew to cover the rest
//!   of the bundle would still have reported success.
//! * **Scratch files go under `/Volumes/Temp/claude/`**, never `/tmp`, never
//!   `$TMPDIR`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "common/manual_pages.rs"]
mod manual_pages;

use manual_pages::{Page, bundle_root, concept_pages, fences, load_page, repo_root};

const SCRATCH: &str = "/Volumes/Temp/claude/ua-manual-snippets";

/// Assert the manual bundle is present.
///
/// OI-0056-010: four lanes here opened with
/// `if bundle_root().is_none() { eprintln!(…); return; }`, so a checkout
/// without `manual/` reported all four as passed — including
/// [`collection_is_not_vacuous`], whose entire job is to notice that the
/// collection is empty. `manual/` is tracked in this repository, so its
/// absence is a broken checkout, not a supported configuration.
fn require_bundle() {
    assert!(
        bundle_root().is_some(),
        "manual/ is not present in this checkout, so the snippet collection would be empty and \
         every check here vacuous. The bundle is tracked in-repo — restore it \
         (`git checkout -- manual`) before running this suite."
    );
}

struct Snippet {
    page: String,
    body_line: usize,
    code: String,
}

/// Blocks the pages mark ```rust``` that are really fragments.
///
/// The durable fix is a ```rust,fragment``` marker in the page, which
/// `tests/manual_conformance.rs` already accepts. That is a body edit, so it
/// invalidates `synced_hash` and must ride a `write-diataxis-manual` sync
/// run; until then this table keeps the lane honest rather than red, and
/// `fragment_allowlist_is_not_stale` makes sure it can only shrink.
const FRAGMENT_ALLOWLIST: &str = "tests/fixtures/manual/unmarked_fragments.txt";

/// The deliberately broken page that proves the compile step can still fail.
const BROKEN_FIXTURE: &str = "tests/fixtures/manual/broken_snippet.md";

/// Ceiling on *entries* in [`FRAGMENT_ALLOWLIST`].
///
/// 2026-08-17: ratcheted down from 40 to 34 against the 32 entries the file
/// actually holds. At 40 the table had room to grow by a quarter without
/// anybody deciding to let it, and every entry it gains is one more block the
/// compiler never sees. Raising this is a deliberate act and belongs in the
/// same commit as the entries that need the room.
const MAX_UNMARKED_FRAGMENTS: usize = 34;

/// Ceiling on the *blocks* those entries absorb.
///
/// Distinct from [`MAX_UNMARKED_FRAGMENTS`]: one entry matches by first code
/// line, so a single row can silence several identical openings. 34 blocks are
/// absorbed today by 32 rows, and bounding only the rows would leave that
/// multiplier unwatched.
const MAX_ALLOWLISTED_BLOCKS: usize = 36;

/// Floor on blocks that actually reach `rustc`.
///
/// 31 are compiled today. This is the guarantee the file claims — "the
/// manual's Rust snippets type-check" is worth nothing if the collection can
/// shrink to one block and still pass.
const MIN_CHECKED_SNIPPETS: usize = 28;

/// Floor on ```rust``` fences seen across the bundle, compiled or allowlisted.
///
/// Mirrors `MIN_RUST_FENCES` in `tests/manual_conformance.rs`, which counts
/// the same fences from the other side of the split. 65 are present today; if
/// these two floors ever disagree, the fence parser here has drifted from the
/// one over there.
const MIN_RUST_FENCES: usize = 60;

fn load_allowlist() -> Vec<(String, String)> {
    let path = repo_root().join(FRAGMENT_ALLOWLIST);
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (page, first) = l.split_once(" :: ").unwrap_or_else(|| {
                panic!("{FRAGMENT_ALLOWLIST}: expected `<page> :: <first line>`, got {l:?}")
            });
            (page.trim().to_string(), first.trim().to_string())
        })
        .collect()
}

fn first_code_line(code: &str) -> String {
    code.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

struct Collected {
    checked: Vec<Snippet>,
    /// Blocks marked ```rust,fragment``` in the page itself.
    declared_fragments: usize,
    /// Blocks skipped through [`FRAGMENT_ALLOWLIST`].
    allowlisted: usize,
    /// Allowlist entries that matched at least one block.
    used: BTreeSet<usize>,
}

fn collect(pages: &[Page]) -> Collected {
    let allow = load_allowlist();
    let mut out = Collected {
        checked: Vec::new(),
        declared_fragments: 0,
        allowlisted: 0,
        used: BTreeSet::new(),
    };
    for page in pages {
        for f in fences(&page.canonical_body) {
            match f.info.as_str() {
                "rust" => {
                    let first = first_code_line(&f.content);
                    if let Some(i) = allow
                        .iter()
                        .position(|(p, l)| *p == page.rel && *l == first)
                    {
                        out.used.insert(i);
                        out.allowlisted += 1;
                        continue;
                    }
                    out.checked.push(Snippet {
                        page: page.rel.clone(),
                        body_line: f.body_line,
                        code: f.content.clone(),
                    });
                }
                "rust,fragment" => out.declared_fragments += 1,
                _ => {}
            }
        }
    }
    out
}

/// Asserts the lane is carrying real work, not a collection that decayed to
/// something an empty-check would still wave through.
///
/// Called from both the default-run [`collection_is_not_vacuous`] and the
/// compiled [`checked_snippets_type_check`], so the floors hold whether or
/// not anybody passes `--ignored`.
fn assert_collection_floors(c: &Collected) {
    let fences_seen = c.checked.len() + c.allowlisted;
    assert!(
        fences_seen >= MIN_RUST_FENCES,
        "only {fences_seen} ```rust``` fence(s) collected across the bundle (expected \
         >= {MIN_RUST_FENCES}); either the bundle shrank or the fence parser has drifted \
         from the one in tests/manual_conformance.rs"
    );
    assert!(
        c.checked.len() >= MIN_CHECKED_SNIPPETS,
        "only {} block(s) reach rustc (expected >= {MIN_CHECKED_SNIPPETS}); {} were \
         absorbed by {FRAGMENT_ALLOWLIST} and {} are declared fragments. The allowlist is \
         meant to shrink, so a drop here means the lane is checking less than it claims",
        c.checked.len(),
        c.allowlisted,
        c.declared_fragments
    );
    assert!(
        c.allowlisted <= MAX_ALLOWLISTED_BLOCKS,
        "{} block(s) are absorbed by {FRAGMENT_ALLOWLIST}, past MAX_ALLOWLISTED_BLOCKS \
         ({MAX_ALLOWLISTED_BLOCKS}); mark them ```rust,fragment``` in the page instead of \
         widening the exemption",
        c.allowlisted
    );
}

/// Collects the deliberately broken control page.
fn control_fixture() -> Collected {
    let fixture = repo_root().join(BROKEN_FIXTURE);
    assert!(fixture.is_file(), "missing fixture {}", fixture.display());
    let page = load_page(&fixture);
    collect(std::slice::from_ref(&page))
}

/// Asserts the control page still has the shape the control depends on.
///
/// Cheap enough for the default run, and worth running there: if the fixture
/// loses its broken block the compiled control stops proving anything, and
/// that failure would otherwise stay invisible until somebody ran
/// `--ignored`.
fn assert_control_shape(c: &Collected) {
    assert_eq!(
        c.checked.len(),
        1,
        "{BROKEN_FIXTURE} must carry exactly one compilable ```rust``` block"
    );
    assert_eq!(
        c.declared_fragments, 1,
        "{BROKEN_FIXTURE} must carry exactly one declared ```rust,fragment``` block"
    );
    assert_eq!(
        c.allowlisted, 0,
        "{BROKEN_FIXTURE} must not be reachable from {FRAGMENT_ALLOWLIST}; allowlisting the \
         control would disarm it"
    );
}

/// Wraps a snippet so it can be type-checked as a library item.
///
/// A block that declares its own `fn main` is used verbatim; anything else is
/// a body, and is wrapped in a fallible function so `?` works the way the
/// page's reader would use it.
fn wrap(index: usize, s: &Snippet) -> String {
    let has_main = s.code.contains("fn main(");
    let inner = if has_main {
        s.code.clone()
    } else {
        format!(
            "    #[allow(unused)]\n    fn _snippet() -> Result<(), Box<dyn std::error::Error>> \
             {{\n{}\n        #[allow(unreachable_code)]\n        Ok(())\n    }}\n",
            s.code
                .lines()
                .map(|l| format!("        {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    format!(
        "// {}:{}\n#[allow(unused, dead_code, unused_imports)]\nmod s{index:03} {{\n{inner}\n}}\n",
        s.page, s.body_line
    )
}

fn profile_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // <target>/<profile>/deps/<name>-<hash>
    let deps = exe.parent()?;
    let profile = deps.parent()?;
    profile.is_dir().then(|| profile.to_path_buf())
}

fn rlib(profile: &Path) -> Option<PathBuf> {
    let p = profile.join("libunified_archive.rlib");
    p.is_file().then_some(p)
}

fn rustc() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string())
}

fn compile(source: &Path, profile: &Path, rlib: &Path) -> (bool, String) {
    let out = Command::new(rustc())
        .arg("--edition")
        .arg("2024")
        .arg("--emit=metadata")
        .arg("--crate-type")
        .arg("lib")
        .arg("--extern")
        .arg(format!("unified_archive={}", rlib.display()))
        .arg("-L")
        .arg(format!("dependency={}", profile.join("deps").display()))
        .arg("-o")
        .arg(source.with_extension("meta"))
        .arg(source)
        .output();
    match out {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(e) => (false, format!("could not run {}: {e}", rustc())),
    }
}

fn prepare() -> (PathBuf, PathBuf) {
    let profile = profile_dir().expect(
        "could not derive the target profile directory from current_exe(); \
         CARGO_TARGET_DIR is deliberately never consulted",
    );
    let lib = rlib(&profile).unwrap_or_else(|| {
        panic!(
            "libunified_archive.rlib is not in {}. Run `cargo build` first — this lane \
             must not silently skip. Enabled features affect what snippets can compile; \
             the rlib here is whatever `cargo build` last produced.",
            profile.display()
        )
    });
    fs::create_dir_all(SCRATCH).expect("cannot create the scratch directory under /Volumes/Temp");
    (profile, lib)
}

/// Compiles the control fixture and asserts `rustc` rejects it for the
/// expected reason.
///
/// A free function rather than a test body so [`checked_snippets_type_check`]
/// can run it too. Both callers are `#[ignore]`d, so a control that lived only
/// in its own test could be filtered out of the very run it is meant to guard.
fn positive_control(profile: &Path, lib: &Path) {
    let c = control_fixture();
    assert_control_shape(&c);

    let path = Path::new(SCRATCH).join("control.rs");
    fs::write(&path, wrap(0, &c.checked[0])).expect("cannot write the control snippet");
    let (ok, err) = compile(&path, profile, lib);
    assert!(
        !ok,
        "the deliberately broken fixture compiled cleanly, so this lane is a no-op"
    );
    assert!(
        err.contains("E0282") || err.contains("type annotations"),
        "the control failed for the wrong reason:\n{err}"
    );
}

#[test]
#[ignore = "shells out to rustc, which is far too expensive for the default gate; run with \
            `cargo build && TMPDIR=/Volumes/Temp/claude cargo test --test manual_snippets \
            --all-features -- --ignored --test-threads=4`"]
fn checked_snippets_type_check() {
    let (profile, lib) = prepare();
    // Prove the compile step can still go red before believing anything it
    // says below. The control reads a repo fixture, not the bundle, so it
    // runs even in a checkout that has no `manual/`.
    positive_control(&profile, &lib);

    require_bundle();
    let pages = concept_pages();
    let c = collect(&pages);
    assert_collection_floors(&c);
    let snippets = &c.checked;
    eprintln!(
        "snippet lane: {} checked block(s), {} declared fragment(s), {} allowlisted \
         unmarked fragment(s)",
        snippets.len(),
        c.declared_fragments,
        c.allowlisted
    );

    let mut batch = String::from(
        "// Generated by tests/manual_snippets.rs. Do not edit.\n\
         #![allow(unused, dead_code, unused_imports, unused_variables)]\n",
    );
    // Remember which batch line each snippet occupies, so a failure reported
    // against the generated file can be attributed back to its page.
    let mut spans: Vec<(usize, usize, usize)> = Vec::new(); // (start, end, index)
    for (i, s) in snippets.iter().enumerate() {
        let start = batch.lines().count() + 1;
        batch.push_str(&wrap(i, s));
        spans.push((start, batch.lines().count(), i));
    }
    let batch_path = Path::new(SCRATCH).join("batch.rs");
    fs::write(&batch_path, &batch).expect("cannot write the batched snippet crate");

    let (ok, err) = compile(&batch_path, &profile, &lib);
    if ok {
        return;
    }

    // Attribute the batch's diagnostics back to pages by mapping the
    // generated file's line numbers through `spans`.
    //
    // Deliberately *not* a per-snippet re-run: one `rustc --emit=metadata`
    // over the batch takes seconds, while 130 individual invocations against
    // the same `-L dependency` directory take tens of minutes on this tree,
    // which is long enough that nobody would ever run the lane.
    let needle = format!("{}:", batch_path.display());
    let mut hits: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut unattributed: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in err.lines() {
        if line.starts_with("error") || line.starts_with("warning") {
            current = Some(line.to_string());
            continue;
        }
        let Some(pos) = line.find(&needle) else {
            continue;
        };
        let rest = &line[pos + needle.len()..];
        let lineno: usize = rest
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let msg = current.clone().unwrap_or_else(|| line.trim().to_string());
        match spans.iter().find(|(a, b, _)| lineno >= *a && lineno <= *b) {
            Some((_, _, idx)) => hits.entry(*idx).or_default().push(msg),
            None => unattributed.push(msg),
        }
    }

    let mut failures = Vec::new();
    for (idx, msgs) in &hits {
        let s = &snippets[*idx];
        let mut seen = BTreeSet::new();
        let mut lines = Vec::new();
        for m in msgs {
            if seen.insert(m.clone()) {
                lines.push(format!("  {m}"));
            }
        }
        failures.push(format!(
            "\n{}  (body line {})\n{}",
            s.page,
            s.body_line,
            lines.join("\n")
        ));
    }
    if failures.is_empty() {
        // Could not map anything; hand back the raw diagnostics rather than
        // claiming success.
        failures.push(format!(
            "\nunattributed rustc output (generated crate: {}):\n{}",
            batch_path.display(),
            err.lines()
                .take(60)
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    panic!(
        "{n} manual snippet(s) do not type-check against the built crate.\n\
         Enabled features matter: the rlib in {p} is whatever `cargo build` produced, so a \
         snippet needing an off-by-default feature will fail here.\n\
         Mark a block `rust,fragment` if it is deliberately incomplete.\n\
         Unattributed diagnostics: {u}\n{b}",
        n = failures.len(),
        p = profile.display(),
        u = unattributed.len(),
        b = failures.join("")
    );
}

#[test]
#[ignore = "shells out to rustc, which is far too expensive for the default gate; run with \
            `cargo build && TMPDIR=/Volumes/Temp/claude cargo test --test manual_snippets \
            --all-features -- --ignored --test-threads=4`"]
fn broken_snippet_fixture_is_rejected() {
    // The positive control, as its own named test so a failure here reads as
    // "the control is broken" rather than "a manual snippet is broken".
    // [`checked_snippets_type_check`] runs the same body, so the guarantee
    // does not depend on anyone selecting this test.
    let (profile, lib) = prepare();
    positive_control(&profile, &lib);
}

/// Never `#[ignore]`d: it only parses fences and never starts a compiler, so
/// the reason the other three are ignored does not apply to it. It is also the
/// invariant that makes [`FRAGMENT_ALLOWLIST`] accountable — every fence is
/// either compiled or named in the allowlist, with no third bucket to hide in.
#[test]
fn fragment_blocks_are_not_compiled() {
    require_bundle();
    let pages = concept_pages();
    let c = collect(&pages);
    let snippets = &c.checked;
    let rust_fences = pages
        .iter()
        .flat_map(|p| fences(&p.canonical_body))
        .filter(|f| f.info == "rust")
        .count();
    assert_eq!(
        snippets.len() + c.allowlisted,
        rust_fences,
        "every `rust` fence must be either compiled or explicitly allowlisted"
    );
    assert_eq!(
        pages
            .iter()
            .flat_map(|p| fences(&p.canonical_body))
            .filter(|f| f.info == "rust,fragment")
            .count(),
        c.declared_fragments,
        "no declared fragment may reach the compiler"
    );
    for s in snippets {
        assert!(
            !s.code.is_empty(),
            "{}:{} produced an empty block",
            s.page,
            s.body_line
        );
    }
}

#[test]
fn fragment_allowlist_is_not_stale() {
    require_bundle();
    let allow = load_allowlist();
    assert!(
        allow.len() <= MAX_UNMARKED_FRAGMENTS,
        "{} allowlisted fragments is past MAX_UNMARKED_FRAGMENTS ({MAX_UNMARKED_FRAGMENTS})",
        allow.len()
    );
    let root = repo_root();
    for (page, _) in &allow {
        assert!(
            root.join(page).is_file(),
            "{FRAGMENT_ALLOWLIST}: page {page} does not exist"
        );
    }
    let c = collect(&concept_pages());
    let stale: Vec<String> = allow
        .iter()
        .enumerate()
        .filter(|(i, _)| !c.used.contains(i))
        .map(|(_, (p, l))| format!("{p} :: {l}"))
        .collect();
    assert!(
        stale.is_empty(),
        "stale entries in {FRAGMENT_ALLOWLIST} — the block was marked `rust,fragment` in \
         the page (good) or its first line changed; delete them:\n  {}",
        stale.join("\n  ")
    );
}

/// The default run's vacuity guard.
///
/// Not `#[ignore]`d, and it starts no compiler: it answers "is there still a
/// lane here at all?", which is the question a `!is_empty()` check only
/// pretended to answer. The compiled lane asserts the same floors, so the two
/// runs cannot disagree about how much work the bundle is carrying.
#[test]
fn collection_is_not_vacuous() {
    require_bundle();
    assert_collection_floors(&collect(&concept_pages()));
}

/// Keeps the positive control armed without paying for a compiler.
///
/// [`positive_control`] is `#[ignore]`d along with everything else that shells
/// out to `rustc`, so in a default run nothing would otherwise notice that
/// `BROKEN_FIXTURE` had lost its broken block, been emptied, or been
/// allowlisted away — and the next `--ignored` run would then be checking a
/// control that could no longer fail.
#[test]
fn control_fixture_shape_is_intact() {
    assert_control_shape(&control_fixture());
}
