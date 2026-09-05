//! Staleness and conformance checks for the Diataxis manual bundle
//! (ticgit c8d771eb).
//!
//! The bundle under `manual/` is derived one-way from `docs/` and the code.
//! Nothing re-checked it after generation, so it drifted: the 2026-08-06
//! hygiene batch falsified ten claims that the 2026-08-07 sync had to repair
//! by hand, including a message quoted verbatim on two reference pages and
//! five citations of a record id that no longer exists. These checks make
//! that class visible from `cargo test`, which — with no CI in this repo — is
//! the only place they will ever actually run.
//!
//! Four checks, in the profile's own vocabulary:
//!
//! * **(a) quoted strings resolve in the source.** A backticked message on a
//!   page must be anchorable to a real string literal in `src/**` or
//!   `build.rs`, a backticked repo path must exist, and a "`<file>` still
//!   says X" claim must actually be quoting that file.
//! * **(b) record ids resolve.** Every `AD`/`MADR`/`DCR`/`DD`/`IG`/`OI`/`DEF`
//!   id a page cites must exist in the catalogue.
//! * **(c) rust fences carry a known marker**, so the snippet lane in
//!   `tests/manual_snippets.rs` knows what it is allowed to compile.
//! * **(d) frontmatter is structurally sound**, and source drift is reported
//!   in the profile's `OK` / `stale` / `conflicted` / `orphaned` / `unknown`
//!   vocabulary.
//!
//! ## What these checks do *not* cover
//!
//! They cover quoted **text**, not claims about behaviour. Of the ten claims
//! the hygiene batch falsified, this design catches the encrypted-creation
//! quotation and the record-id citations. It does **not** catch the
//! `Cancelled` label-set claim (the string `"extract_file"` genuinely exists
//! in `src/error.rs`; the defect was which label reaches which code path) or
//! the DEF-004 backend count. Closing this ticket does not mean "the manual
//! is verified".
//!
//! ## Ownership
//!
//! This target owns record-id **resolution** inside `manual/**/en/`.
//! `tests/record_citations_test.rs` (ticgit dd119253) owns **subject
//! matching** repo-wide and resolution everywhere else. Both share
//! `tests/common/records.rs`.
//!
//! ## Why (d) is report-only by default
//!
//! Under the honest rule — one `git status`, one `git log`, and no mtime
//! fallback — every page is `stale` right now, because the tree moved through
//! 0.3.1 and the StreamBound work after the bundle was generated, and 13 of
//! 27 pages fail the canonical body hash because the 0.3.1 release edited
//! English page bodies without re-hashing. Both are real findings and both
//! would make this suite red on arrival, which is how a suite gets ignored.
//! So the census reports, `UA_MANUAL_STRICT=1` (or the `#[ignore]`d
//! `strict_bundle_is_in_sync`) enforces, and the flip belongs to whoever owns
//! the release ritual. Rewriting `synced_hash` from a test would destroy the
//! only hand-edit signal the bundle has and is refused outright.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

#[path = "common/manual_pages.rs"]
mod manual_pages;
#[path = "common/records.rs"]
mod records;

use manual_pages::{
    Page, bundle_root, canonical_body, concept_pages, fences, inline_spans, is_reserved, repo_root,
    reserved_pages, sha256_hex, source_literals, source_text, strip_fences,
};

// ---------------------------------------------------------------------------
// Vacuity floors — a parser regression that empties a corpus would otherwise
// make every check below pass silently. These are floors, not exact counts,
// so ordinary authoring does not churn them.
// ---------------------------------------------------------------------------

const MIN_CONCEPT_PAGES: usize = 20;
const MIN_SOURCE_LITERALS: usize = 1_000;
const MIN_RUST_FENCES: usize = 60;
const MIN_RECORD_CITATIONS: usize = 40;
const MIN_MESSAGE_CANDIDATES: usize = 10;
const MIN_LITERAL_CANDIDATES: usize = 1;
const MIN_PATH_CANDIDATES: usize = 20;

/// Fence info strings the bundle is allowed to use.
///
/// `rust` means "checked standalone" and is what `tests/manual_snippets.rs`
/// compiles. `rust,fragment` means "declared incomplete, never compiled".
/// The marker rides in the info string because GitHub and mdBook key
/// highlighting off its first token, so `rust,fragment` still renders as
/// Rust. Any other info string fails, which is what catches a typo'd language
/// tag or a snippet smuggled in under an unknown marker.
const ALLOWED_FENCE_INFO: &[&str] = &["", "rust", "rust,fragment", "text", "bash", "cmd", "toml"];

/// Extensions that make a backticked, space-free, slash-bearing span look
/// like a repo path worth resolving.
const PATH_EXTENSIONS: &[&str] = &[
    ".rs", ".toml", ".md", ".yaml", ".yml", ".lock", ".sh", ".json", ".txt",
];

const EXEMPT_FILE: &str = "tests/fixtures/manual/checks_exempt.txt";

// ---------------------------------------------------------------------------
// Exemptions
// ---------------------------------------------------------------------------

/// One exemption: a page, the exact normalized candidate span it excuses, and
/// why.
///
/// The manual profile's "unknown producer keys are preserved" rule would have
/// allowed a page-local `checks_exempt:` frontmatter key, which diffs next to
/// the quotation it excuses. That is the nicer home, but adding it means
/// editing pages in `manual/`, so this repo uses the profile's documented
/// fallback: a test-side table keyed by page path. Same three fields, same
/// hygiene rules.
#[derive(Debug, Clone)]
struct Exemption {
    page: String,
    candidate: String,
    why: String,
    used: std::cell::Cell<bool>,
}

const MIN_EXEMPT_REASON: usize = 30;
const MAX_EXEMPTIONS: usize = 12;

fn load_exemptions() -> Vec<Exemption> {
    let path = repo_root().join(EXEMPT_FILE);
    // OI-0056-010: an unreadable table used to become an empty one, so
    // `exemption_table_hygiene` and both consuming checks passed over
    // nothing. The file is tracked, so a read failure is a broken checkout.
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read the tracked exemption table {}: {e}. An empty table would silently \
             re-arm every exempted candidate, so this is a failure rather than a default.",
            path.display()
        )
    });
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(" :: ").collect();
        assert_eq!(
            parts.len(),
            3,
            "{EXEMPT_FILE} line {}: expected `<page> :: <candidate> :: <reason>`, got {line:?}",
            n + 1
        );
        out.push(Exemption {
            page: parts[0].trim().to_string(),
            candidate: parts[1].trim().to_string(),
            why: parts[2].trim().to_string(),
            used: std::cell::Cell::new(false),
        });
    }
    out
}

fn exempt(list: &[Exemption], page: &str, candidate: &str) -> bool {
    for e in list {
        if e.page == page && e.candidate == candidate {
            e.used.set(true);
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

/// Assert the bundle is present.
///
/// OI-0056-010: this was `skip_if_no_bundle() -> bool`, and eleven lanes
/// began with `if skip_if_no_bundle() { return; }` — so a checkout without
/// `manual/` reported all eleven as passed. `manual/` is tracked in this
/// repository, so its absence is a broken checkout rather than a supported
/// configuration, and the honest response is a failure naming the missing
/// directory.
fn require_bundle() {
    assert!(
        bundle_root().is_some(),
        "manual/ is not present in this checkout, so every check in this file would be vacuous. \
         The bundle is tracked in-repo — restore it (`git checkout -- manual`) before running \
         this suite."
    );
}

fn stripped(page: &Page) -> String {
    strip_fences(&page.canonical_body)
}

/// A span that is exactly `"..."` — the (a1) literal rule's candidate shape.
fn is_quoted_literal(span: &str) -> bool {
    span.len() >= 3
        && span.starts_with('"')
        && span.ends_with('"')
        && !span[1..span.len() - 1].contains('"')
}

/// A span that looks like a formatted message: it has whitespace and at least
/// one `{...}` placeholder, and is not Rust struct syntax.
fn is_message_candidate(span: &str) -> bool {
    if !span.contains(' ') {
        return false;
    }
    if span.contains("{ ") || span.contains(" }") {
        // `Single { path: PathBuf }`, `Format { format: None, .. }`.
        return false;
    }
    let mut i = 0;
    let b: Vec<char> = span.chars().collect();
    while i < b.len() {
        if b[i] == '{' {
            let mut j = i + 1;
            let mut inner = String::new();
            while j < b.len() && b[j] != '}' {
                inner.push(b[j]);
                j += 1;
            }
            if j < b.len() && !inner.contains(char::is_whitespace) {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// A span that names a file in this repo, slash or not. Used by (a4), where
/// `Cargo.toml` is as much a claim subject as `src/error.rs`.
fn names_a_repo_file(span: &str) -> bool {
    if span.contains(char::is_whitespace) || span.starts_with("http") {
        return false;
    }
    PATH_EXTENSIONS.iter().any(|e| span.ends_with(e)) && repo_root().join(span).is_file()
}

fn looks_like_repo_path(span: &str) -> bool {
    if span.contains(char::is_whitespace) || !span.contains('/') {
        return false;
    }
    if span.starts_with("http") || span.starts_with('<') {
        return false;
    }
    PATH_EXTENSIONS.iter().any(|e| span.ends_with(e))
}

// ---------------------------------------------------------------------------
// (a2) source-anchored message resolution
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Anchor {
    /// Matched: every segment of the chosen literal occurs in the quote, in
    /// order.
    Matched { file: String, literal: String },
    /// The chosen literal has a segment the quote does not contain.
    Missing {
        file: String,
        literal: String,
        segment: String,
    },
    /// No literal in the corpus has any segment inside the quote.
    Unanchored,
}

/// Resolves a quoted message against the source corpus.
///
/// The naive rule — "the manual's quote must appear verbatim in `src/`" —
/// does not work, because a page routinely fills a placeholder in:
/// `src/error.rs` carries `"Entry '{}' is a {}; refusing to materialize per
/// FR-022 link-skip policy"` while the page writes `is a symbolic link;`.
/// So the match is **source-anchored**: pick the literal whose longest
/// placeholder-free segment occurs in the quote, then require *every* segment
/// of that literal to occur in the quote, in order. A drifted quote fails on
/// a specific named segment, which is the diagnostic that matters.
fn anchor_message(quote: &str) -> Anchor {
    let mut best: Option<(usize, &manual_pages::Literal)> = None;
    for lit in source_literals() {
        let mut longest = 0usize;
        for seg in &lit.segments {
            if quote.contains(seg.as_str()) {
                longest = longest.max(seg.chars().count());
            }
        }
        if longest == 0 {
            continue;
        }
        if best.map(|(n, _)| longest > n).unwrap_or(true) {
            best = Some((longest, lit));
        }
    }
    let Some((_, lit)) = best else {
        return Anchor::Unanchored;
    };
    let mut cursor = 0usize;
    for seg in &lit.segments {
        match quote[cursor..].find(seg.as_str()) {
            Some(i) => cursor += i + seg.len(),
            None => {
                return Anchor::Missing {
                    file: lit.file.clone(),
                    literal: lit.text.clone(),
                    segment: seg.clone(),
                };
            }
        }
    }
    Anchor::Matched {
        file: lit.file.clone(),
        literal: lit.text.clone(),
    }
}

// ---------------------------------------------------------------------------
// (d) drift classification — a pure function, so it is testable without a
// synthetic worktree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceState {
    Ok,
    Changed,
    Orphaned,
    /// The working tree has uncommitted changes to this source, so the drift
    /// answer is not knowable from commit times. The profile's own rule: "if
    /// undecidable, report as `unknown`, never guess."
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageState {
    Ok,
    Stale,
    Conflicted,
    Orphaned,
    Unknown,
}

fn classify(hand_edited: bool, sources: &[SourceState]) -> PageState {
    if sources.contains(&SourceState::Orphaned) {
        return PageState::Orphaned;
    }
    let changed = sources.contains(&SourceState::Changed);
    if hand_edited && changed {
        return PageState::Conflicted;
    }
    if sources.contains(&SourceState::Unknown) {
        return PageState::Unknown;
    }
    if changed {
        return PageState::Stale;
    }
    if hand_edited {
        return PageState::Conflicted;
    }
    PageState::Ok
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo_root())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Why a git query came back empty, distinguished well enough to act on.
///
/// The census used to report every failure as "not a git worktree (or git is
/// unavailable)". That is one of at least three causes, and it sends a reader
/// to check the wrong thing: on this repository git is installed, the
/// directory *is* a worktree, and `git status` succeeds — what fails is
/// history traversal, because an object referenced by the commit graph cannot
/// be read. Naming the cause is the difference between a one-line fix and an
/// afternoon.
fn git_failure_reason() -> String {
    let root = repo_root();
    match Command::new("git")
        .arg("--version")
        .current_dir(&root)
        .output()
    {
        Err(e) => return format!("git could not be executed: {e}"),
        Ok(o) if !o.status.success() => return "`git --version` failed".to_string(),
        Ok(_) => {}
    }
    let in_worktree = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&root)
        .output()
        .is_ok_and(|o| o.status.success());
    if !in_worktree {
        return "not a git worktree".to_string();
    }
    // git runs and this is a worktree, so one specific query failed. The
    // history walk is the one that traverses parents, so it is the one that
    // surfaces an unreadable object; report its own words.
    match Command::new("git")
        .args(["log", "--format=%x00%cI", "--name-only"])
        .current_dir(&root)
        .output()
    {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let first = stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("no stderr")
                .trim();
            format!(
                "git is present and this is a worktree, but the history could not be traversed: {first}"
            )
        }
        _ => "a git query failed, but re-running it here succeeded".to_string(),
    }
}

/// Paths with uncommitted changes, as repo-relative strings. `None` means the
/// question could not be asked (not a git worktree, or git is missing).
fn dirty_paths() -> Option<BTreeSet<String>> {
    let out = git(&["status", "--porcelain"])?;
    let mut set = BTreeSet::new();
    for line in out.lines() {
        if line.len() < 4 {
            continue;
        }
        let p = line[3..].trim();
        // Renames read `old -> new`; both sides count as touched.
        for part in p.split(" -> ") {
            set.insert(part.trim_matches('"').to_string());
        }
    }
    Some(set)
}

/// Last commit time per path, as an RFC-3339 string, for commits at or after
/// `since`. One `git log` for the whole bundle, not one per source.
fn last_commit_times(since: &str) -> Option<BTreeMap<String, String>> {
    let out = git(&[
        "log",
        "--format=%x00%cI",
        "--name-only",
        &format!("--since={since}"),
    ])?;
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut current = String::new();
    for line in out.lines() {
        if let Some(ts) = line.strip_prefix('\u{0}') {
            current = ts.trim().to_string();
            continue;
        }
        if line.trim().is_empty() || current.is_empty() {
            continue;
        }
        map.entry(line.trim().to_string())
            .or_insert_with(|| current.clone());
    }
    Some(map)
}

// ---------------------------------------------------------------------------
// Tests: corpus and extractors
// ---------------------------------------------------------------------------

#[test]
fn extractor_finds_the_expected_corpus() {
    require_bundle();
    let pages = concept_pages();
    assert!(
        pages.len() >= MIN_CONCEPT_PAGES,
        "only {} concept pages found (expected >= {MIN_CONCEPT_PAGES})",
        pages.len()
    );
    assert!(
        source_literals().len() >= MIN_SOURCE_LITERALS,
        "only {} anchorable source literals found (expected >= {MIN_SOURCE_LITERALS}); \
         the Rust literal lexer has probably regressed",
        source_literals().len()
    );

    let mut rust_fences = 0usize;
    let mut messages = 0usize;
    let mut literals = 0usize;
    let mut paths = 0usize;
    let mut record_cites = 0usize;
    for page in &pages {
        for f in fences(&page.canonical_body) {
            if f.info.split(',').next() == Some("rust") {
                rust_fences += 1;
            }
        }
        let body = stripped(page);
        for span in inline_spans(&body) {
            if is_quoted_literal(&span.text) {
                literals += 1;
            }
            if is_message_candidate(&span.text) {
                messages += 1;
            }
            if looks_like_repo_path(&span.text) {
                paths += 1;
            }
        }
        record_cites += records::scan_ids(
            &page.canonical_body,
            &["MADR", "AD", "DCR", "DD", "IG", "OI", "DEF"],
            0,
        )
        .len();
    }
    assert!(
        rust_fences >= MIN_RUST_FENCES,
        "only {rust_fences} rust fences"
    );
    assert!(
        messages >= MIN_MESSAGE_CANDIDATES,
        "only {messages} message candidates"
    );
    assert!(
        literals >= MIN_LITERAL_CANDIDATES,
        "only {literals} literal candidates"
    );
    assert!(paths >= MIN_PATH_CANDIDATES, "only {paths} path candidates");
    assert!(
        record_cites >= MIN_RECORD_CITATIONS,
        "only {record_cites} record citations"
    );
}

#[test]
fn inline_spans_span_line_breaks() {
    // The bug that made the line-oriented prototype miss the exact quotation
    // this ticket was filed over.
    let body = "Text before `encrypted creation for {:?} is not supported\n\
                (MADR-0027); password must be None` and text after.\n";
    let spans = inline_spans(body);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(
        spans[0].text,
        "encrypted creation for {:?} is not supported (MADR-0027); password must be None"
    );
}

#[test]
fn struct_shaped_spans_are_not_message_candidates() {
    for s in [
        "Single { path: PathBuf }",
        "Format { format: None, message: \"x\" }",
        "ExtractionOptions { overwrite: true }",
    ] {
        assert!(
            !is_message_candidate(s),
            "{s:?} must not be a message candidate"
        );
    }
    for s in [
        "encrypted creation for {:?} is not supported",
        "Entry '{}' is a {}; refusing to materialize",
    ] {
        assert!(is_message_candidate(s), "{s:?} must be a message candidate");
    }
}

#[test]
fn fenced_code_is_not_scanned_for_spans() {
    let body = "Before.\n\n```rust\nlet s = `not a span`;\n```\n\nAfter `a real span`.\n";
    let spans = inline_spans(&strip_fences(body));
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0].text, "a real span");
}

#[test]
fn canonical_body_hash_matches_the_awk_definition() {
    // The profile defines the body as everything after the SECOND `---` line.
    // A `---` inside the body is content, not a delimiter.
    let doc = "---\ntitle: x\n---\nbody line one\n---\nbody line three\n";
    let (body, _) = canonical_body(doc);
    assert_eq!(body, "body line one\n---\nbody line three\n");
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256 is wrong, so every hash comparison below is meaningless"
    );
    // A file with no frontmatter hashes the empty string rather than its body.
    assert_eq!(canonical_body("no frontmatter here\n").0, "");

    require_bundle();
    // At least one live page must reproduce its stored hash, or the pipeline
    // reproduction is wrong rather than the bundle being stale.
    let matching = concept_pages()
        .iter()
        .filter(|p| p.synced_hash() == Some(p.body_hash().as_str()))
        .count();
    assert!(
        matching > 0,
        "no live page reproduces its stored synced_hash; the canonical-body \
         reproduction disagrees with the profile's awk|shasum pipeline"
    );
}

#[test]
fn korean_mirrors_are_never_read() {
    require_bundle();
    for p in manual_pages::walk_manual() {
        let r = manual_pages::rel(&p);
        assert!(
            !r.ends_with(".ko.md") && !r.split('/').any(|s| s == "ko"),
            "translation artifact reached the page walker: {r}"
        );
    }
    // And the mirrors really are there to be skipped, so this is not vacuous.
    assert!(
        repo_root().join("manual/reference/user/ko").exists()
            || repo_root()
                .join("manual/reference/user/en/errors-and-warnings.ko.md")
                .exists(),
        "no Korean mirror found at all; the exclusion may be passing vacuously"
    );
}

#[test]
fn reserved_files_are_excluded() {
    require_bundle();
    let reserved: Vec<String> = reserved_pages().iter().map(|p| p.rel.clone()).collect();
    assert!(
        reserved.contains(&"manual/log.md".to_string()),
        "{reserved:?}"
    );
    assert!(
        reserved.iter().any(|r| r.ends_with("/index.md")),
        "{reserved:?}"
    );
    for p in concept_pages() {
        assert!(!is_reserved(&p.rel), "{} should be reserved", p.rel);
    }
}

// ---------------------------------------------------------------------------
// (a) quoted strings resolve in the source
// ---------------------------------------------------------------------------

#[test]
fn quoted_strings_resolve_in_source() {
    require_bundle();
    let exemptions = load_exemptions();
    let corpus = source_text();
    let mut findings = Vec::new();
    let mut checked = 0usize;

    for page in concept_pages() {
        let body = stripped(&page);
        for span in inline_spans(&body) {
            // (a1) exact literal rule
            if is_quoted_literal(&span.text) {
                checked += 1;
                if !corpus.contains(&span.text) && !exempt(&exemptions, &page.rel, &span.text) {
                    findings.push(format!(
                        "\n{}  (body line {})\n  span    : {}\n  \
                         verdict : no such string literal in src/ or build.rs\n  \
                         fix     : quote the literal as the code writes it, or add a line to\n  \
                         \x20         {EXEMPT_FILE} with a reason.\n",
                        page.rel, span.line, span.text
                    ));
                }
                continue;
            }
            // (a2) source-anchored message rule
            if is_message_candidate(&span.text) {
                checked += 1;
                if exempt(&exemptions, &page.rel, &span.text) {
                    continue;
                }
                match anchor_message(&span.text) {
                    Anchor::Matched { file, literal } => {
                        let _ = (file, literal);
                    }
                    Anchor::Missing {
                        file,
                        literal,
                        segment,
                    } => findings.push(format!(
                        "\n{}  (body line {})\n  page says      : {}\n  \
                         anchored to    : {file}\n  source literal : {literal:?}\n  \
                         missing segment: {segment:?}\n  \
                         verdict        : the page quotes a message the code no longer emits.\n  \
                         fix            : requote from the source literal.\n",
                        page.rel, span.line, span.text
                    )),
                    Anchor::Unanchored => findings.push(format!(
                        "\n{}  (body line {})\n  page says: {}\n  \
                         verdict  : no source literal shares a {}-char run with this quote.\n  \
                         fix      : requote from the source, or — if the message really is \
                         built by concatenation — add a line to {EXEMPT_FILE} with a reason.\n",
                        page.rel,
                        span.line,
                        span.text,
                        manual_pages::MIN_SEGMENT
                    )),
                }
            }
        }
    }
    assert!(
        checked > 0,
        "no quoted-string candidates were examined at all"
    );
    assert!(
        findings.is_empty(),
        "{n} quoted string(s) do not resolve in the source ({checked} candidates checked):\n{b}",
        n = findings.len(),
        b = findings.join("")
    );
}

#[test]
fn message_anchor_detects_a_seeded_defect() {
    // The anti-vacuity regression for (a2), and the one that encodes the
    // incident: `src/options.rs` says `(MADR-0027)`, the pre-2026-08-07 pages
    // said `(AD 0027)`.
    let good = "encrypted creation for {:?} is not supported (MADR-0027); password must be None";
    match anchor_message(good) {
        Anchor::Matched { file, literal } => {
            assert!(
                file.starts_with("src/"),
                "the quotation must anchor to a source file, got {file}"
            );
            assert!(
                literal.contains("MADR-0027"),
                "the anchoring literal must be the real one, got {literal:?}"
            );
        }
        other => panic!("the live quotation must anchor cleanly, got {other:?}"),
    }
    let seeded = "encrypted creation for {:?} is not supported (AD 0027); password must be None";
    match anchor_message(seeded) {
        Anchor::Missing { segment, .. } => assert!(
            segment.contains("MADR-0027"),
            "the missing segment must name the id that drifted, got {segment:?}"
        ),
        other => panic!("the seeded defect must be caught, got {other:?}"),
    }
}

#[test]
fn cited_paths_exist() {
    require_bundle();
    let exemptions = load_exemptions();
    let root = repo_root();
    let mut findings = Vec::new();
    for page in concept_pages() {
        let body = stripped(&page);
        for span in inline_spans(&body) {
            if !looks_like_repo_path(&span.text) {
                continue;
            }
            let candidate = root.join(&span.text);
            // Only resolve paths whose parent directory exists in this repo;
            // that alone removes tutorial fixtures such as `notes/todo.txt`.
            let Some(parent) = candidate.parent() else {
                continue;
            };
            if !parent.is_dir() {
                continue;
            }
            if candidate.exists() || exempt(&exemptions, &page.rel, &span.text) {
                continue;
            }
            findings.push(format!(
                "\n{}  (body line {})\n  cites path: {}\n  \
                 verdict   : no such file, but `{}` exists so the page means this repo.\n  \
                 fix       : correct the path, or add a line to {EXEMPT_FILE} with a reason.\n",
                page.rel,
                span.line,
                span.text,
                manual_pages::rel(parent)
            ));
        }
    }
    assert!(
        findings.is_empty(),
        "{n} cited path(s) do not exist:\n{b}",
        n = findings.len(),
        b = findings.join("")
    );
}

/// Markers that turn a sentence into a checkable claim about a named file.
const FILE_CLAIM_VERBS: &[&str] = &[
    " still says ",
    " still describes ",
    " still documents ",
    " still lists ",
    " still names ",
    " reads ",
];

/// (a4) "`<file>` still says `<quote>`" — the class that burned the bundle:
/// four such observations were falsified within a day of being written.
///
/// Zero live candidates today. It is implemented anyway so the class is
/// checkable the moment such a sentence is written again.
fn file_scoped_claim_findings(pages: &[Page]) -> (usize, Vec<String>) {
    let root = repo_root();
    let mut candidates = 0usize;
    let mut findings = Vec::new();
    for page in pages {
        let body = stripped(page);
        let spans = inline_spans(&body);
        for (i, span) in spans.iter().enumerate() {
            // A file reference here need not contain a slash: the class this
            // rule exists for is "`Cargo.toml` still says X" as much as
            // "`src/error.rs` still says X".
            if !names_a_repo_file(&span.text) {
                continue;
            }
            let Some(next) = spans.get(i + 1) else {
                continue;
            };
            let between = &body[span.offset..next.offset];
            // The claim and its quote must be in the same paragraph.
            if between.contains("\n\n") {
                continue;
            }
            let Some(verb) = FILE_CLAIM_VERBS.iter().find(|v| between.contains(**v)) else {
                continue;
            };
            let target = root.join(&span.text);
            if !target.is_file() {
                continue;
            }
            candidates += 1;
            let Ok(text) = fs::read_to_string(&target) else {
                continue;
            };
            let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.contains(&next.text) {
                continue;
            }
            findings.push(format!(
                "\n{}  (body line {})\n  claims  : `{}`{}`{}`\n  \
                 verdict : that text does not occur in {}.\n  \
                 fix     : requote it, or drop the claim.\n",
                page.rel, span.line, span.text, verb, next.text, span.text
            ));
        }
    }
    (candidates, findings)
}

#[test]
fn file_scoped_claims_quote_their_file() {
    require_bundle();
    let (_, findings) = file_scoped_claim_findings(&concept_pages());
    assert!(
        findings.is_empty(),
        "{n} \"<file> still says X\" claim(s) do not match the file:\n{b}",
        n = findings.len(),
        b = findings.join("")
    );

    // Zero live candidates today, so the rule is exercised against a fixture.
    let fixture = repo_root().join("tests/fixtures/manual/anchor_cases.md");
    assert!(fixture.is_file(), "missing fixture {}", fixture.display());
    let page = manual_pages::load_page(&fixture);
    let (candidates, findings) = file_scoped_claim_findings(std::slice::from_ref(&page));
    assert!(
        candidates >= 2,
        "the fixture must produce at least two file-scoped claims, got {candidates}"
    );
    assert_eq!(
        findings.len(),
        1,
        "the fixture's true claim must pass and its false one must fail:\n{}",
        findings.join("")
    );
    assert!(
        findings[0].contains("name = \"unified-archive-that-does-not-exist\""),
        "the wrong finding was reported:\n{}",
        findings[0]
    );
}

// ---------------------------------------------------------------------------
// (b) record ids resolve
// ---------------------------------------------------------------------------

/// Series the manual cites. `R`-prefixed review ids have a different shape
/// (`R0080-0006`, no separator) and are deliberately out of scope here: they
/// live in record bodies, and resolving them costs a 2 MB read for no
/// observed defect.
const CITED_SERIES: &[&str] = &["MADR", "AD", "DCR", "DD", "IG", "OI", "DEF"];

/// A double-quoted occurrence is a *mention* of a retired id, not a citation.
/// This is a documented bundle convention, not a suppression: it is how
/// `manual/explanation/user/en/one-api-many-backends.md` names the id older
/// documents used, and it is why this check needs no exemption list at all.
fn is_quoted_mention(text: &str, offset: usize) -> bool {
    let before = &text[..offset];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let col = offset - line_start;
    let quotes_before = line[..col].matches('"').count();
    quotes_before % 2 == 1
}

fn unresolved_ids(page: &Page) -> Vec<String> {
    let catalogue = records::record_index();
    let ledger = records::ledger_ids();
    let mut out = Vec::new();
    let mut scan = |text: &str, where_: &str| {
        for m in records::scan_ids(text, CITED_SERIES, 0) {
            if catalogue.contains_key(&m.id) || ledger.contains(&m.id) {
                continue;
            }
            if is_quoted_mention(text, m.offset) {
                continue;
            }
            out.push(format!(
                "\n{}  ({where_} line {})\n  cites   : {}\n  \
                 verdict : no record file and no ledger entry carries this id.\n  \
                 fix     : cite the surviving id; or, if you mean the retired one, write it\n  \
                 \x20        in double quotes so it reads as a mention.\n",
                page.rel,
                records::line_of(text, m.offset),
                m.raw
            ));
        }
    };
    scan(&page.canonical_body, "body");
    if let Some(fm) = &page.frontmatter {
        let tagline = fm.tags.join(", ");
        for m in records::scan_ids(&tagline, CITED_SERIES, 0) {
            if !catalogue.contains_key(&m.id) && !ledger.contains(&m.id) {
                out.push(format!(
                    "\n{}  (frontmatter tags)\n  cites   : {}\n  \
                     verdict : no record file and no ledger entry carries this id.\n",
                    page.rel, m.raw
                ));
            }
        }
    }
    out
}

#[test]
fn record_ids_resolve() {
    require_bundle();
    let mut findings = Vec::new();
    for page in concept_pages() {
        findings.extend(unresolved_ids(&page));
    }
    assert!(
        findings.is_empty(),
        "{n} record id(s) cited by the manual do not resolve:\n{b}",
        n = findings.len(),
        b = findings.join("")
    );
}

#[test]
fn record_index_resolves_known_ids() {
    // Pins the id parser against exactly the shapes a naive regex got wrong.
    let idx = records::record_index();
    for id in [
        "IG-004-01",
        "IG-020-003",
        "MADR-0014",
        "AD-0001",
        "MADR-0027",
    ] {
        assert!(idx.contains_key(id), "{id} should resolve");
    }
    for id in ["AD-0027", "AD-9999", "MADR-9999"] {
        assert!(!idx.contains_key(id), "{id} should NOT resolve");
    }
    assert_eq!(
        records::filename_id_prefix("MADR-0014-r0001-sfx-probable-confidence-clamp.md").as_deref(),
        Some("MADR-0014")
    );
    assert_eq!(
        records::filename_id_prefix("IG-0061-0094-0123-tempfile-tempdir.md").as_deref(),
        Some("IG-0061-0094-0123")
    );
    assert_eq!(records::filename_id_prefix("readme.md"), None);
}

#[test]
fn double_quoted_ids_are_mentions_not_citations() {
    let mention = "the record older documents call \"AD 0027\" is now MADR-0027";
    let m = records::scan_ids(mention, &["AD"], 0);
    assert_eq!(m.len(), 1);
    assert!(
        is_quoted_mention(mention, m[0].offset),
        "a double-quoted id must read as a mention"
    );
    let citation = "refused per AD 0027 in the creation path";
    let m = records::scan_ids(citation, &["AD"], 0);
    assert!(
        !is_quoted_mention(citation, m[0].offset),
        "a bare id must read as a citation"
    );
    let backticked = "refused per `AD 0027` in the creation path";
    let m = records::scan_ids(backticked, &["AD"], 0);
    assert!(
        !is_quoted_mention(backticked, m[0].offset),
        "a backticked id is still a citation"
    );
}

// ---------------------------------------------------------------------------
// (c) fence markers
// ---------------------------------------------------------------------------

#[test]
fn rust_fences_carry_a_known_marker() {
    require_bundle();
    let mut findings = Vec::new();
    let mut total = 0usize;
    for page in concept_pages() {
        for f in fences(&page.canonical_body) {
            total += 1;
            if ALLOWED_FENCE_INFO.contains(&f.info.as_str()) {
                continue;
            }
            findings.push(format!(
                "\n{}  (body line {})\n  fence info: ```{}\n  \
                 allowed   : {:?}\n  \
                 verdict   : unknown fence marker; `rust` is compiled by\n  \
                 \x20           tests/manual_snippets.rs, `rust,fragment` is declared\n  \
                 \x20           incomplete and skipped.\n",
                page.rel, f.body_line, f.info, ALLOWED_FENCE_INFO
            ));
        }
    }
    assert!(total >= MIN_RUST_FENCES, "only {total} fenced blocks found");
    assert!(
        findings.is_empty(),
        "{n} fenced block(s) carry an unknown marker:\n{b}",
        n = findings.len(),
        b = findings.join("")
    );
}

// ---------------------------------------------------------------------------
// (d) structure and drift
// ---------------------------------------------------------------------------

#[test]
fn page_frontmatter_and_sources_are_structurally_sound() {
    require_bundle();
    let root = repo_root();
    let mut problems = Vec::new();
    for page in concept_pages() {
        let Some(fm) = &page.frontmatter else {
            problems.push(format!("{}: concept page has no frontmatter", page.rel));
            continue;
        };
        for key in ["type", "title", "audience", "synced_hash"] {
            if fm.get(key).map(|v| v.is_empty()).unwrap_or(true) {
                problems.push(format!("{}: missing frontmatter key `{key}:`", page.rel));
            }
        }
        match fm.generated.get("at") {
            Some(at) if at.len() >= 20 && at.ends_with('Z') && at.contains('T') => {}
            other => problems.push(format!(
                "{}: `generated.at` is missing or unparseable: {other:?}",
                page.rel
            )),
        }
        if fm.sources.is_empty() {
            problems.push(format!("{}: declares no `sources:`", page.rel));
        }
        for (id, resource) in &fm.sources {
            if !root.join(resource).exists() {
                problems.push(format!(
                    "{}: source `{id}` points at `{resource}`, which no longer exists \
                     (the profile calls this `orphaned`)",
                    page.rel
                ));
            }
        }
    }
    for page in reserved_pages() {
        if page.rel == "manual/index.md" {
            continue; // carries only `okf_version`, by profile
        }
        if page.frontmatter.is_some() {
            problems.push(format!(
                "{}: reserved file must not carry concept frontmatter",
                page.rel
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{n} structural problem(s) in the manual bundle:\n  {b}",
        n = problems.len(),
        b = problems.join("\n  ")
    );
}

struct Census {
    lines: Vec<String>,
    counts: BTreeMap<&'static str, usize>,
    stale_or_worse: usize,
    hash_mismatches: usize,
    reason: Option<String>,
}

fn census() -> Census {
    let pages = concept_pages();
    let earliest = pages
        .iter()
        .filter_map(|p| p.frontmatter.as_ref()?.generated.get("at").cloned())
        .min()
        .unwrap_or_else(|| "2020-01-01T00:00:00Z".to_string());

    let dirty = dirty_paths();
    let commits = last_commit_times(&earliest);
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut lines = Vec::new();
    let mut stale_or_worse = 0usize;
    let mut hash_mismatches = 0usize;

    // The mtime fallback is deliberately absent. The profile only ever
    // offered it for non-git checkouts, and in a git worktree it flagged all
    // 27 pages with no signal at all. Outside a worktree we emit exactly one
    // `unknown` line naming the reason and stop; we never guess.
    let (Some(dirty), Some(commits)) = (dirty, commits) else {
        let why = git_failure_reason();
        return Census {
            lines: vec![format!(
                "unknown: source drift is not decidable ({why}); no mtime guess is made"
            )],
            counts,
            stale_or_worse: 0,
            hash_mismatches: 0,
            reason: Some(why),
        };
    };

    for page in &pages {
        let Some(fm) = &page.frontmatter else {
            continue;
        };
        let generated_at = fm.generated.get("at").cloned().unwrap_or_default();
        let hand_edited = page.synced_hash() != Some(page.body_hash().as_str());
        if hand_edited {
            hash_mismatches += 1;
        }
        let mut states = Vec::new();
        let mut detail = Vec::new();
        for (_, resource) in &fm.sources {
            let state = if !repo_root().join(resource).exists() {
                SourceState::Orphaned
            } else if dirty.contains(resource) {
                SourceState::Unknown
            } else if commits
                .get(resource)
                .map(|t| t.as_str() > generated_at.as_str())
                .unwrap_or(false)
            {
                SourceState::Changed
            } else {
                SourceState::Ok
            };
            if state != SourceState::Ok {
                detail.push(format!("{resource}={state:?}"));
            }
            states.push(state);
        }
        let state = classify(hand_edited, &states);
        let label = match state {
            PageState::Ok => "ok",
            PageState::Stale => "stale",
            PageState::Conflicted => "conflicted",
            PageState::Orphaned => "orphaned",
            PageState::Unknown => "unknown",
        };
        *counts.entry(label).or_insert(0) += 1;
        if state != PageState::Ok {
            stale_or_worse += 1;
            lines.push(format!(
                "{label:<11} {}  hand_edited={hand_edited}  {}",
                page.rel,
                detail.join(" ")
            ));
        }
    }
    Census {
        lines,
        counts,
        stale_or_worse,
        hash_mismatches,
        reason: None,
    }
}

#[test]
fn drift_classification_prefers_unknown_over_a_guess() {
    // The regression for the actual complaint in the ticket: an uncommitted
    // edit to a source makes the answer unknowable, and `unknown` is the only
    // honest verdict. Exercised on the classifier directly rather than
    // against a synthetic worktree, so the rule is pinned without a
    // subprocess.
    assert_eq!(
        classify(false, &[SourceState::Ok, SourceState::Ok]),
        PageState::Ok
    );
    assert_eq!(
        classify(false, &[SourceState::Ok, SourceState::Unknown]),
        PageState::Unknown,
        "a dirty source must report unknown, never stale"
    );
    assert_eq!(
        classify(false, &[SourceState::Changed, SourceState::Ok]),
        PageState::Stale
    );
    assert_eq!(
        classify(true, &[SourceState::Changed]),
        PageState::Conflicted,
        "a hand-edited page over a changed source is `conflicted`, not `stale`"
    );
    assert_eq!(classify(true, &[SourceState::Ok]), PageState::Conflicted);
    assert_eq!(
        classify(false, &[SourceState::Unknown, SourceState::Orphaned]),
        PageState::Orphaned,
        "a vanished source outranks an undecidable one"
    );
}

#[test]
fn source_drift_census() {
    require_bundle();
    let c = census();
    let total: usize = c.counts.values().sum();
    eprintln!(
        "manual drift census: {}  (hash mismatches: {})",
        c.counts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" "),
        c.hash_mismatches
    );
    for line in &c.lines {
        eprintln!("  {line}");
    }
    // OI-0056-010: `if c.reason.is_none()` used to be the only thing
    // standing between this lane and asserting nothing — outside a git
    // worktree the census is undecidable and every assertion below was
    // skipped. Tests run from the repository, so "not a git worktree" is a
    // broken environment, and the classification assertion is now
    // unconditional.
    assert!(
        c.reason.is_none(),
        "the drift census could not be computed ({}), so it would classify nothing: {}",
        c.reason.as_deref().unwrap_or("unknown reason"),
        c.lines.join("\n  ")
    );
    assert_eq!(
        total,
        concept_pages().len(),
        "every concept page must be classified exactly once"
    );
    if std::env::var("UA_MANUAL_STRICT").as_deref() == Ok("1") {
        assert_eq!(
            c.stale_or_worse, 0,
            "UA_MANUAL_STRICT=1: {} page(s) are not in sync; run the manual sync",
            c.stale_or_worse
        );
    }
}

/// Strict enforcement of the same census `source_drift_census` reports.
///
/// Kept `#[ignore]`d because it is genuinely red today — the bundle drifted
/// through 0.3.1 and the StreamBound work (see this module's "Why (d) is
/// report-only by default"), and repairing it belongs to the manual sync
/// ritual, not to this suite. OI-0056-010 only added the command, because an
/// `#[ignore]` nobody can run is indistinguishable from a deleted test:
///
/// ```text
/// TMPDIR=/Volumes/Temp/claude cargo test --test manual_conformance --all-features \
///     -- --ignored --test-threads=4 strict_bundle_is_in_sync
/// ```
///
/// `UA_MANUAL_STRICT=1` on `source_drift_census` enforces the same
/// `stale_or_worse == 0` half without the per-page hash check.
#[test]
#[ignore = "red until the next write-diataxis-manual sync run (see manual/log.md); run with \
            `TMPDIR=/Volumes/Temp/claude cargo test --test manual_conformance --all-features \
            -- --ignored --test-threads=4 strict_bundle_is_in_sync`"]
fn strict_bundle_is_in_sync() {
    require_bundle();
    let c = census();
    let mut problems = Vec::new();
    for page in concept_pages() {
        let stored = page.synced_hash().unwrap_or("<none>").to_string();
        let actual = page.body_hash();
        if stored != actual {
            problems.push(format!(
                "{}: synced_hash {stored} != canonical body hash {actual}",
                page.rel
            ));
        }
    }
    assert!(
        problems.is_empty() && c.stale_or_worse == 0,
        "the bundle is not in sync ({} page(s) drifted, {} hash mismatch(es)):\n  {}\n{}",
        c.stale_or_worse,
        problems.len(),
        problems.join("\n  "),
        c.lines.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Exemption hygiene
// ---------------------------------------------------------------------------

#[test]
fn exemption_table_hygiene() {
    require_bundle();
    let exemptions = load_exemptions();
    assert!(
        exemptions.len() <= MAX_EXEMPTIONS,
        "{} exemptions is past MAX_EXEMPTIONS ({MAX_EXEMPTIONS}); if the rule needs this \
         many escapes the rule is wrong and should be narrowed, not padded",
        exemptions.len()
    );
    let root = repo_root();
    for e in &exemptions {
        assert!(
            root.join(&e.page).is_file(),
            "{EXEMPT_FILE}: page {} does not exist",
            e.page
        );
        assert!(
            e.why.len() >= MIN_EXEMPT_REASON,
            "{EXEMPT_FILE}: reason for {:?} is {} chars, minimum {MIN_EXEMPT_REASON}",
            e.candidate,
            e.why.len()
        );
    }

    // Self-pruning: run the two checks that consume exemptions and require
    // every entry to have been needed.
    let corpus = source_text();
    for page in concept_pages() {
        let body = stripped(&page);
        for span in inline_spans(&body) {
            // Both consuming checks retire an exemption the same way, so
            // they share one arm; the `||` keeps the original chain's
            // short-circuit order, which matters because `anchor_message`
            // walks the whole source corpus.
            if (is_quoted_literal(&span.text) && !corpus.contains(&span.text))
                || (is_message_candidate(&span.text)
                    && !matches!(anchor_message(&span.text), Anchor::Matched { .. }))
            {
                exempt(&exemptions, &page.rel, &span.text);
            } else if looks_like_repo_path(&span.text) {
                let candidate = root.join(&span.text);
                if candidate.parent().map(|p| p.is_dir()).unwrap_or(false) && !candidate.exists() {
                    exempt(&exemptions, &page.rel, &span.text);
                }
            }
        }
    }
    let stale: Vec<&Exemption> = exemptions.iter().filter(|e| !e.used.get()).collect();
    assert!(
        stale.is_empty(),
        "stale exemptions in {EXEMPT_FILE} — delete them:\n  {}",
        stale
            .iter()
            .map(|e| format!("{} :: {}", e.page, e.candidate))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
