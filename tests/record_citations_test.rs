//! Record-citation conformance checks (ticgit dd119253).
//!
//! Two defects keep recurring in this tree's prose:
//!
//! 1. A citation names a record id that does not exist at all — `AD 0025` in
//!    `CHANGELOG.md`, where the surviving record is `MADR-0025`.
//! 2. A citation names a record id that *does* exist but whose subject is not
//!    the sentence's subject. The 2026-07-23 renumber migration split one flat
//!    `00NN` numbering into two series, `AD-00NN` and `MADR-00NN`, which
//!    collide over the whole band `0001..=0032`. A bare `AD 0015` written
//!    before the migration usually means today's `MADR-0015`, and an
//!    existence check cannot tell the two apart because both records exist.
//!
//! The band is exactly `0001..=0032`: `MADR` numbering is contiguous over it
//! and `AD` numbering overlaps all of it. Outside the band a bare `AD 00NN`
//! is unambiguous.
//!
//! ## Ownership
//!
//! This target owns **subject matching repo-wide**, and **resolution
//! everywhere except `manual/`**. Resolution inside `manual/**/en/` belongs to
//! `tests/manual_conformance.rs` (ticgit c8d771eb), which applies the
//! bundle's own "a double-quoted id is a *mention* of a retired id, not a
//! citation" convention. Both targets share one catalogue and one scanner,
//! `tests/common/records.rs`, so neither reimplements the other's half.
//!
//! ## The matcher is a byte scanner, not a regex
//!
//! `ADR-NNNN` is a bundle-wide *tag* namespace — `MADR-0027`'s own
//! frontmatter reads `tags: [decision, ADR-0027, ...]` — and every record
//! title, `MADR` records included, is prefixed `"AD: "`. A substring search
//! for `AD-0027` therefore matches inside `MADR-0027` and `ADR-0027` alike.
//! `records::scan_ids` is boundary-anchored on both sides and callers start
//! it after the YAML frontmatter, which is where the tag namespace lives.
//!
//! ## Why a heuristic is acceptable as a hard gate
//!
//! The subject check is biased toward passing: the *cited* record gets a
//! generous +/-500 char neighbourhood to prove itself, while the *rival*
//! record must prove itself within +/-150 chars and must land at least one
//! rare token. A citation that names `MADR-00NN` anywhere in the wide window
//! is skipped outright — it is already disambiguated for the reader, and
//! saying so is the cheapest correct fix. Every finding prints the matched
//! tokens with their weights, so a reader judges the verdict instead of
//! trusting it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "common/records.rs"]
mod records;

use records::{Record, body_start, is_translation_artifact, line_of, rel, repo_root, window};

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// Directories walked for citations, relative to the repo root. `manual/` is
/// further restricted to its `/en/` pages by [`is_in_scope`].
const SCAN_DIRS: &[&str] = &[
    "src", "tests", "examples", "benches", "docs", "specs", "manual",
];

/// Individual non-`.md` files walked in addition to [`SCAN_DIRS`]; root-level
/// `*.md` is picked up separately by [`collect_files`].
const SCAN_FILES: &[&str] = &["build.rs"];

const SCAN_EXTS: &[&str] = &["md", "rs", "toml"];

/// Directories excluded categorically. Each entry is a *reason*, not a name:
///
/// * `docs/records/` — content-immutable. A finding there is unfixable in
///   place (only a dated `## Amendment` section may be appended), so flagging
///   it would produce a permanently red, unactionable test.
/// * `docs/investigation/` — regenerated snapshots; a hand-fix is churn the
///   next generation undoes.
/// * `docs/superpowers/`, `reviews/`, `issues/` — retired plans and archived
///   review artifacts that deliberately preserve pre-consolidation ids as the
///   historical record.
/// * `docs/decisions/`, `docs/architecture/decisions/`,
///   `docs/project/design-change-records/` — the pre-migration record trees
///   that `redirects.yaml` points *away* from.
/// * `tests/fixtures/` — inputs to checks, not prose. A fixture deliberately
///   carries wrong ids so a checker can be shown to catch them.
const EXCLUDED_PREFIXES: &[&str] = &[
    "docs/records/",
    "docs/investigation/",
    "docs/superpowers/",
    "docs/decisions/",
    "docs/architecture/decisions/",
    "docs/project/design-change-records/",
    "tests/fixtures/",
    "reviews/",
    "issues/",
    "target/",
    "node_modules/",
    "extracted/",
];

/// The record-hygiene checkers' own sources. They quote record ids verbatim —
/// in the waiver anchors below, in the scanners' doc comments, and in the
/// fixed-case tests, which deliberately include retired ids such as `AD 0027`
/// and fabricated ones such as `AD-9999` — so they would otherwise flag
/// themselves. Every entry is asserted to exist, to be one of these targets'
/// own `.rs` sources, and to actually be excluded from the scan.
const SELF_EXCLUDED: &[&str] = &[
    "tests/record_citations_test.rs",
    "tests/manual_conformance.rs",
    "tests/manual_snippets.rs",
    "tests/common/records.rs",
    "tests/common/manual_pages.rs",
];

/// Resolution inside the manual bundle is `tests/manual_conformance.rs`'s
/// half of the split; subject matching there is still this target's.
const RESOLUTION_EXCLUDED_PREFIXES: &[&str] = &["manual/"];

/// The ambiguous numbering band: every `AD 00NN` in it has a `MADR-00NN`
/// rival, so a bare citation cannot be resolved by existence alone.
const BAND: std::ops::RangeInclusive<u32> = 1..=32;

/// Series this target scans. `MADR` must be listed for its own sake; the
/// scanner's left-boundary rule is what keeps it from also reading as `AD`.
const KNOWN_SERIES: &[&str] = &["MADR", "AD", "DCR"];

// ---------------------------------------------------------------------------
// Waivers
// ---------------------------------------------------------------------------

struct FileWaiver {
    path: &'static str,
    why: &'static str,
}

struct CiteWaiver {
    path: &'static str,
    cited: &'static str,
    /// A short verbatim snippet of the citing sentence, which must appear in
    /// the matched window. Text-anchored, never line-anchored: line numbers
    /// drift, and a bare `(path, id)` key would silently absolve a *new*
    /// wrong citation of the same id added to the same file later.
    anchor: &'static str,
    why: &'static str,
}

const MAX_HISTORICAL_DOCUMENTS: usize = 3;
/// 2026-08-17: ratcheted down from 22 to 3 once the 2026-08-16 mis-citation
/// sweep landed and nineteen of the twenty-two entries self-pruned. The cap is
/// a ratchet, not a budget: a table that may hold twenty-two rows while holding
/// two lets the exemption list quietly regrow to swallow the finding it exists
/// to surface. Raising it again is a deliberate act that belongs in the same
/// commit as the entries that need the room.
const MAX_SUBJECT_WAIVERS: usize = 3;
const MAX_DANGLING_WAIVERS: usize = 3;

/// Whole-file exemptions. Kept here rather than in [`EXCLUDED_PREFIXES`] so
/// each one carries a visible, counted reason, and so a file that stops
/// tripping the rule fails as a stale waiver instead of staying exempt
/// forever.
const HISTORICAL_DOCUMENTS: &[FileWaiver] = &[
    FileWaiver {
        path: "docs/architecture/decision-review-2026-07-19.md",
        why: "2026-08-16: a dated review OF the pre-renumber record set. Its ids are the \
              subject matter, not references to today's catalogue; rewriting them would \
              falsify the review it records.",
    },
    FileWaiver {
        path: "docs/index.md",
        why: "2026-08-16: a generated mirror of record titles. The ids inside it are \
              quotations of content-immutable record headings, and it is regenerated \
              rather than hand-edited.",
    },
    FileWaiver {
        path: "docs/backlog.md",
        why: "2026-08-16: the persistent backlog register whose entries are literally \
              about the wrong-citation defect; the ids it names are the evidence being \
              tracked, so correcting them in place would erase the finding.",
    },
];

/// Individual citations that this heuristic reads wrongly, or that are real
/// defects in files this agent does not own. Each entry is dated and says
/// which it is.
const SUBJECT_WAIVERS: &[CiteWaiver] = &[
    CiteWaiver {
        path: "manual/explanation/developer/en/sfx-detection-pipeline.md",
        cited: "AD-0015",
        anchor: "confirmed-detection patterns that AD 0015 deferred",
        why: "2026-08-16 (dd119253): REAL DEFECT, not a heuristic miss — the passage is about \
             SFX confirmed detection and its known stub patterns, so it should cite \
             `MADR-0015`. The page's two other `AD 0015` citations are correct (they really \
             are about iterating all candidates and demoting the CD signature), so this is a \
             one-token edit, not a page-wide rename. 2026-08-17: still live. `manual/` is a \
             one-way generated bundle outside this target's file ownership — its pages carry \
             `generated:`/`sources:` provenance and are refreshed by a `write-diataxis-manual` \
             sync run, so the correction must ride that sync rather than be hand-patched here.",
    },
    CiteWaiver {
        path: "docs/project/open-issues-resolved.md",
        cited: "AD-0007",
        anchor: "ZIP creation encryption is silently unsupported (AD 0007 regression)",
        why: "2026-08-16 (dd119253): FALSE POSITIVE, and the seed case for this table. The \
             sentence really does mean AD-0007 \"Dual ZIP Backend Strategy\" — the \
             regression is that the dual-backend choice silently dropped creation encryption \
             — while MADR-0007 happens to be titled \"ZIP AES-256 creation encryption\". \
             Titles are the only subject signal this check has and these two cannot be \
             separated by token overlap; narrowing the rule to catch it would cost real \
             findings. 2026-08-17: re-confirmed still live and still a false positive; this \
             entry is permanent unless the subject heuristic gains a second signal.",
    },
];

/// Citations that resolve to no record at all.
const DANGLING_WAIVERS: &[CiteWaiver] = &[
    CiteWaiver {
        path: "CHANGELOG.md",
        cited: "AD-0025",
        anchor: "mirroring AD 0025's handling of R0058/R0057",
        why: "2026-08-16 (dd119253): REAL DEFECT — the surviving record is MADR-0025 \
             (\"Review 0058 Archived as Duplicate of Review 0057\"), which \
             docs/records/redirects.yaml resolves directly. CHANGELOG.md belongs to another \
             agent this session; delete this entry with the one-token correction. \
             2026-08-17: still live after the docs/ and specs/ citation sweep, which did not \
             reach the root-level CHANGELOG.md. That file is outside this target's file \
             ownership, and it is a released-history document, so the correction is filed \
             here rather than applied.",
    },
    CiteWaiver {
        path: "CHANGELOG.md",
        cited: "AD-0048",
        anchor: "Close OI-0057-007 against v0.1.x",
        why: "2026-08-16 (dd119253): REAL DEFECT of a different class — `AD 0048` resolves to \
             no record in either series and no redirect covers it, yet the line claims it is \
             \"archived under docs/records/\". Needs a decision about what that entry \
             became, not a rename; filed for the record owner rather than guessed at. \
             2026-08-17: still live. The sweep that fixed docs/ and specs/ could not settle \
             this one either — no rename resolves it, so it needs the record owner's \
             decision, and CHANGELOG.md is outside this target's file ownership.",
    },
    CiteWaiver {
        path: "CHANGELOG.md",
        cited: "AD-0043",
        anchor: "Reject R0062-0007 (tempdir-security concern already resolved)",
        why: "2026-08-16 (dd119253): REAL DEFECT, same class as AD 0043's neighbour AD 0048 — \
             no record and no redirect resolves it, while the line claims it is archived \
             under docs/records/. Needs a decision about what the entry became, not a \
             rename. 2026-08-17: still live, for the same two reasons as its neighbour — \
             no rename resolves it, and CHANGELOG.md is outside this target's file \
             ownership.",
    },
];

/// Reasons that do not survive review, because they do not survive this test.
const BANNED_REASONS: &[&str] = &["n/a", "historical", "see above", "todo", "legacy"];
const MIN_REASON_LEN: usize = 40;

// ---------------------------------------------------------------------------
// Vacuity guards
// ---------------------------------------------------------------------------

const MIN_RECORDS: usize = 140;
const MIN_REDIRECTS: usize = 40;
const MIN_FILES_SCANNED: usize = 180;
const MIN_CITATIONS: usize = 400;
const MIN_BAND_CITATIONS: usize = 200;

// ---------------------------------------------------------------------------
// File collection
// ---------------------------------------------------------------------------

fn is_in_scope(rel_path: &str) -> bool {
    if is_translation_artifact(rel_path) {
        return false;
    }
    if EXCLUDED_PREFIXES.iter().any(|p| rel_path.starts_with(p)) {
        return false;
    }
    if SELF_EXCLUDED.contains(&rel_path) {
        return false;
    }
    if rel_path.starts_with("manual/") {
        // Only the English pages plus the bundle's top-level files; every
        // other language tree belongs to the translation pipeline.
        let is_en = rel_path.split('/').any(|seg| seg == "en");
        let is_top_level = rel_path.matches('/').count() == 1;
        if !is_en && !is_top_level {
            return false;
        }
    }
    match Path::new(rel_path).extension().and_then(|e| e.to_str()) {
        Some(ext) => SCAN_EXTS.contains(&ext),
        None => false,
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let r_slash = format!("{}/", rel(&path));
            if EXCLUDED_PREFIXES.iter().any(|p| r_slash.starts_with(p)) {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("ko") {
                continue;
            }
            walk(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn collect_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut raw = Vec::new();
    for dir in SCAN_DIRS {
        walk(&root.join(dir), &mut raw);
    }
    for f in SCAN_FILES {
        let p = root.join(f);
        if p.is_file() {
            raw.push(p);
        }
    }
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
                raw.push(p);
            }
        }
    }
    let mut out: Vec<PathBuf> = raw.into_iter().filter(|p| is_in_scope(&rel(p))).collect();
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Tokenizing and subject scoring
// ---------------------------------------------------------------------------

const STOPWORDS: &[&str] = &[
    "ad",
    "adr",
    "madr",
    "dcr",
    "the",
    "and",
    "for",
    "with",
    "from",
    "into",
    "not",
    "but",
    "are",
    "was",
    "were",
    "has",
    "have",
    "had",
    "its",
    "our",
    "all",
    "any",
    "per",
    "via",
    "record",
    "records",
    "decision",
    "decisions",
    "review",
    "reviews",
    "note",
    "notes",
    "archive",
    "archives",
    "archived",
    "option",
    "options",
    "reject",
    "rejects",
    "rejected",
    "use",
    "used",
    "uses",
    "using",
    "add",
    "added",
    "adds",
    "new",
    "old",
    "this",
    "that",
    "these",
    "those",
    "when",
    "then",
    "than",
    "will",
    "would",
    "should",
    "can",
    "must",
    "may",
    "does",
    "did",
    "doc",
    "docs",
    "src",
    "file",
    "files",
    "line",
    "lines",
    "test",
    "tests",
    "code",
    "one",
    "two",
    "see",
    "also",
    "how",
    "why",
    "what",
    "which",
    "who",
    "each",
    "some",
    "more",
    "most",
    "less",
    "only",
    "just",
    "now",
    "still",
    "such",
    "same",
    "other",
    "over",
    "under",
    "after",
    "before",
    "because",
    "while",
];

fn is_noise_token(t: &str) -> bool {
    if t.len() <= 2 {
        return true;
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    // `r0003`, `oi0057` and friends: one or two letters followed by digits.
    let letters = t.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    if letters > 0 && letters <= 2 && t[letters..].bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    STOPWORDS.contains(&t)
}

/// Splits on non-alphanumerics **and on camelCase boundaries**.
///
/// The camelCase split is load-bearing, not cosmetic: without it
/// `ModificationOptions` never matches `AD-0020`'s
/// `additive-modification-options-extension` slug, and the *correct*
/// `AD 0020` citation in `docs/architecture/bootstrap-config.md` is flagged.
fn tokenize(text: &str) -> BTreeSet<String> {
    fn push(cur: &mut String, out: &mut BTreeSet<String>) {
        if cur.is_empty() {
            return;
        }
        let t = cur.to_lowercase();
        cur.clear();
        if is_noise_token(&t) {
            return;
        }
        if t.len() >= 5 && t.ends_with('s') && !t.ends_with("ss") {
            let sing = t[..t.len() - 1].to_string();
            if !is_noise_token(&sing) {
                out.insert(sing);
            }
        }
        out.insert(t);
    }

    let mut out = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut cur = String::new();
    for (i, c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            push(&mut cur, &mut out);
            continue;
        }
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let boundary = match prev {
            Some(p) if p.is_lowercase() && c.is_uppercase() => true,
            Some(p) if p.is_numeric() != c.is_numeric() => true,
            _ => false,
        };
        if boundary {
            push(&mut cur, &mut out);
        }
        cur.push(*c);
    }
    push(&mut cur, &mut out);
    out
}

/// Token set of a record: its file slug (minus the id prefix and any leading
/// `rNNNN-` review tag) plus its title (minus the `AD:`/`MADR:` label).
fn record_tokens(rec: &Record) -> BTreeSet<String> {
    let mut slug = rec.file.trim_end_matches(".md").to_string();
    if let Some(rest) = slug.strip_prefix(&format!("{}-", rec.id)) {
        slug = rest.to_string();
    }
    let head: String = slug.chars().take_while(|c| *c != '-').collect();
    if head.len() >= 2 && head.starts_with('r') && head[1..].bytes().all(|b| b.is_ascii_digit()) {
        slug = slug[head.len() + 1..].to_string();
    }
    let title = rec
        .title
        .split_once(": ")
        .map(|(_, t)| t.to_string())
        .unwrap_or_else(|| rec.title.clone());
    let mut set = tokenize(&slug);
    set.extend(tokenize(&title));
    set
}

/// A token appearing in at most this many record token sets is decisive
/// (weight 1.0); anything more common is ambient noise (0.4). Computed at
/// runtime from `index.yaml` so it self-tunes as records land.
const RARE_MAX_RECORDS: usize = 3;

fn token_weights(all: &[(String, BTreeSet<String>)]) -> BTreeMap<String, f64> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, set) in all {
        for t in set {
            *counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .map(|(t, n)| (t, if n <= RARE_MAX_RECORDS { 1.0 } else { 0.4 }))
        .collect()
}

fn score(
    rec_tokens: &BTreeSet<String>,
    win_tokens: &BTreeSet<String>,
    weights: &BTreeMap<String, f64>,
) -> (f64, Vec<(String, f64)>) {
    let mut total = 0.0;
    let mut matched = Vec::new();
    for t in rec_tokens.intersection(win_tokens) {
        let w = *weights.get(t).unwrap_or(&1.0);
        total += w;
        matched.push((t.clone(), w));
    }
    matched.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    (total, matched)
}

const TIGHT_RADIUS: usize = 150;
const WIDE_RADIUS: usize = 500;
const MIN_RIVAL_SCORE: f64 = 1.0;
const MIN_MARGIN: f64 = 0.6;

// ---------------------------------------------------------------------------
// The subject verdict, isolated so fixed cases can exercise it directly
// ---------------------------------------------------------------------------

struct Subject {
    weights: BTreeMap<String, f64>,
    tokens_by_id: BTreeMap<String, BTreeSet<String>>,
    by_id: BTreeMap<String, Record>,
}

struct Verdict {
    cited: Record,
    rival: Record,
    s_cited: f64,
    s_rival: f64,
    m_cited: Vec<(String, f64)>,
    m_rival: Vec<(String, f64)>,
}

impl Subject {
    fn new() -> Self {
        let index = records::index_records();
        let all: Vec<(String, BTreeSet<String>)> = index
            .iter()
            .map(|r| (r.id.clone(), record_tokens(r)))
            .collect();
        Subject {
            weights: token_weights(&all),
            tokens_by_id: all.into_iter().collect(),
            by_id: index.iter().map(|r| (r.id.clone(), r.clone())).collect(),
        }
    }

    /// `None` means "no finding": out of band, no rival, self-glossed, or the
    /// evidence did not clear the thresholds.
    fn judge(&self, cited_id: &str, text: &str, offset: usize) -> Option<Verdict> {
        let (series, number) = cited_id.split_once('-')?;
        if series != "AD" {
            return None;
        }
        let num: u32 = number.parse().ok()?;
        if !BAND.contains(&num) {
            return None;
        }
        let rival_id = format!("MADR-{number}");
        let cited = self.by_id.get(cited_id)?;
        let rival = self.by_id.get(&rival_id)?;

        let wide = window(text, offset, WIDE_RADIUS);
        // Self-gloss escape: the passage already names the rival, so the
        // reader is not misled. Cheapest correct fix, offered first in the
        // failure message.
        if wide.contains(&rival_id) {
            return None;
        }
        let tight = window(text, offset, TIGHT_RADIUS);
        let (s_cited, m_cited) =
            score(&self.tokens_by_id[cited_id], &tokenize(wide), &self.weights);
        let (s_rival, m_rival) = score(
            &self.tokens_by_id[&rival_id],
            &tokenize(tight),
            &self.weights,
        );
        let has_rare = m_rival.iter().any(|(_, w)| *w >= 1.0);
        if !(has_rare && s_rival >= MIN_RIVAL_SCORE && s_rival - s_cited >= MIN_MARGIN) {
            return None;
        }
        Some(Verdict {
            cited: cited.clone(),
            rival: rival.clone(),
            s_cited,
            s_rival,
            m_cited,
            m_rival,
        })
    }
}

// ---------------------------------------------------------------------------
// Waiver bookkeeping
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WaiverUse {
    files: BTreeSet<usize>,
    subjects: BTreeSet<usize>,
    danglings: BTreeSet<usize>,
}

/// Marks a whole-file waiver as *used* only when the file actually produced a
/// finding, so an exemption that has stopped covering anything goes stale.
fn file_waived(rel_path: &str, used: &mut WaiverUse) -> bool {
    for (i, w) in HISTORICAL_DOCUMENTS.iter().enumerate() {
        if w.path == rel_path {
            used.files.insert(i);
            return true;
        }
    }
    false
}

fn cite_waived(
    table: &'static [CiteWaiver],
    rel_path: &str,
    id: &str,
    win: &str,
    used: &mut BTreeSet<usize>,
) -> bool {
    for (i, w) in table.iter().enumerate() {
        if w.path == rel_path && w.cited == id && win.contains(w.anchor) {
            used.insert(i);
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// The scan
// ---------------------------------------------------------------------------

struct Scan {
    files: Vec<PathBuf>,
    total_citations: usize,
    band_citations: usize,
    subject: Vec<String>,
    dangling: Vec<String>,
    used: WaiverUse,
}

fn scan_all() -> Scan {
    let subject_model = Subject::new();
    let redirects = records::redirects();
    let index = records::index_records();
    let catalogue = records::record_index();

    let files = collect_files();
    let mut used = WaiverUse::default();
    let mut subject = Vec::new();
    let mut dangling = Vec::new();
    let mut total_citations = 0usize;
    let mut band_citations = 0usize;

    for path in &files {
        let rel_path = rel(path);
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let start = body_start(&text);
        let resolution_in_scope = !RESOLUTION_EXCLUDED_PREFIXES
            .iter()
            .any(|p| rel_path.starts_with(p));

        for c in records::scan_ids(&text, KNOWN_SERIES, start) {
            // Compound ids (`AD-0061-0094`) are not band citations.
            if c.groups.len() != 1 {
                continue;
            }
            total_citations += 1;
            let num: u32 = c.groups[0].parse().unwrap_or(0);
            let in_band = c.series == "AD" && c.groups[0].len() == 4 && BAND.contains(&num);
            if in_band {
                band_citations += 1;
            }

            // --- resolution -------------------------------------------------
            if resolution_in_scope && !catalogue.contains_key(&c.id) {
                let win = window(&text, c.offset, WIDE_RADIUS);
                if file_waived(&rel_path, &mut used)
                    || cite_waived(DANGLING_WAIVERS, &rel_path, &c.id, win, &mut used.danglings)
                {
                    continue;
                }
                let mut msg = format!(
                    "\n{rel_path}  (line {ln})\n\
                     \x20 cites    : {raw}\n\
                     \x20 {id:<9} <no record with this id>\n",
                    ln = line_of(&text, c.offset),
                    raw = c.raw,
                    id = c.id,
                );
                let hits: Vec<_> = redirects
                    .iter()
                    .filter(|(k, _)| k.contains(&format!("/{}-", c.groups[0])) || *k == &c.id)
                    .collect();
                if hits.is_empty() {
                    msg.push_str(
                        "  redirects: nothing in docs/records/redirects.yaml resolves this id\n",
                    );
                } else {
                    for (k, v) in hits {
                        let resolved = index
                            .iter()
                            .find(|r| &r.file == v)
                            .map(|r| format!("{} \"{}\"", r.id, r.title))
                            .unwrap_or_else(|| v.clone());
                        msg.push_str(&format!("  redirects: {k} -> {resolved}\n"));
                    }
                }
                msg.push_str(
                    "  fix      : cite the surviving record id; or, if the gap is real, file it\n\
                     \x20            as a ticket and add a dated DANGLING_WAIVERS entry naming it.\n",
                );
                dangling.push(msg);
                continue;
            }

            // --- subject ----------------------------------------------------
            let Some(v) = subject_model.judge(&c.id, &text, c.offset) else {
                continue;
            };
            let wide = window(&text, c.offset, WIDE_RADIUS);
            if file_waived(&rel_path, &mut used)
                || cite_waived(SUBJECT_WAIVERS, &rel_path, &c.id, wide, &mut used.subjects)
            {
                continue;
            }
            subject.push(format_verdict(
                &rel_path,
                line_of(&text, c.offset),
                &c.raw,
                &v,
                {
                    window(&text, c.offset, 90)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            ));
        }
    }

    Scan {
        files,
        total_citations,
        band_citations,
        subject,
        dangling,
        used,
    }
}

fn fmt_matched(m: &[(String, f64)]) -> String {
    if m.is_empty() {
        "(none)".to_string()
    } else {
        m.iter()
            .map(|(t, w)| format!("{t}({w:.1})"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn format_verdict(rel_path: &str, ln: usize, raw: &str, v: &Verdict, quote: String) -> String {
    format!(
        "\n{rel_path}  (line {ln})\n\
         \x20 cites    : {raw}\n\
         \x20 reads    : \"...{quote}...\"\n\
         \x20 {cid:<9} \"{ctitle}\"\n\
         \x20             {cfile}   [score {sc:.1} / +-{WIDE_RADIUS}]\n\
         \x20 {rid:<9} \"{rtitle}\"\n\
         \x20             {rfile}   [score {sr:.1} / +-{TIGHT_RADIUS}]\n\
         \x20 cited hit: {mc}\n\
         \x20 rival hit: {mr}\n\
         \x20 verdict  : the surrounding text is about {rid}'s subject, not {cid}'s.\n\
         \x20 fix      : write `{rid}`; or, if {cid} really is meant, name {rid} in the same\n\
         \x20            paragraph (that alone satisfies this check); or add a dated\n\
         \x20            SUBJECT_WAIVERS entry with an anchor and a reason.\n",
        cid = v.cited.id,
        ctitle = v.cited.title,
        cfile = v.cited.file,
        sc = v.s_cited,
        rid = v.rival.id,
        rtitle = v.rival.title,
        rfile = v.rival.file,
        sr = v.s_rival,
        mc = fmt_matched(&v.m_cited),
        mr = fmt_matched(&v.m_rival),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn record_index_and_redirects_parse_and_agree() {
    let index = records::index_records();
    let redirects = records::redirects();
    let root = repo_root();

    assert!(
        index.len() >= MIN_RECORDS,
        "docs/records/index.yaml parsed only {} records (expected >= {MIN_RECORDS}); \
         the reader has probably drifted from the generator's format",
        index.len()
    );
    assert!(
        redirects.len() >= MIN_REDIRECTS,
        "docs/records/redirects.yaml parsed only {} entries (expected >= {MIN_REDIRECTS})",
        redirects.len()
    );

    let known_files: BTreeSet<&str> = index.iter().map(|r| r.file.as_str()).collect();
    let mut bad = Vec::new();
    for (key, target) in redirects {
        if known_files.contains(target.as_str()) {
            continue;
        }
        // Targets outside docs/records/ (e.g. `../project/open-issues.md`)
        // are legitimate; they must still exist on disk.
        if root.join("docs/records").join(target).exists() {
            continue;
        }
        bad.push(format!("  {key} -> {target} (target does not exist)"));
    }
    assert!(
        bad.is_empty(),
        "docs/records/redirects.yaml points at targets that are neither a record `file:` \
         nor an existing path:\n{}",
        bad.join("\n")
    );

    for rec in index {
        assert!(
            root.join("docs/records").join(&rec.file).exists(),
            "index.yaml record {} names a missing file: docs/records/{}",
            rec.id,
            rec.file
        );
    }
}

#[test]
fn index_shape_assertion_rejects_malformed_yaml() {
    // An unrecognised line must error, not be skipped.
    let bad_line = "records:\n  - id: AD-0001\n    type: AD\n    title: \"x\"\n    \
                    status: active\n    file: a.md\nrogue line\n";
    assert!(
        records::parse_index(bad_line).is_err(),
        "rogue line was accepted"
    );

    // A record missing `file:` must error, not yield a half-built record.
    let missing = "records:\n  - id: AD-0001\n    type: AD\n    title: \"x\"\n    status: active\n";
    let err = records::parse_index(missing).unwrap_err();
    assert!(err.contains("file"), "unexpected error: {err}");

    // No `records:` header at all must error rather than yield zero records.
    assert!(records::parse_index("# only a comment\n").is_err());

    // A well-formed document still parses, including the `..` range ids.
    let good = "# comment\nrecords:\n  - id: IG-0061-0094..0123\n    type: IG\n    \
                title: \"AD: x\"\n    status: active\n    file: IG-0061.md\n";
    assert_eq!(records::parse_index(good).unwrap().len(), 1);

    assert!(records::parse_redirects("redirects:\n nope\n").is_err());
    assert_eq!(
        records::parse_redirects("redirects:\n  \"a/b.md\": AD-0001-x.md\n")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn matcher_is_boundary_anchored() {
    // The exact trap this ticket was filed over: a substring search for
    // `AD-0027` matches inside `MADR-0027`, and `ADR-NNNN` is a legitimate
    // bundle-wide tag namespace.
    let negatives = [
        "MADR-0027",
        "ADR-0027",
        "xAD-0027",
        "AD-00271",
        "AD_0027",
        "READ-0027",
        "3AD-0027",
        "AD0027",
        "AD--0027",
        "AD-0027x",
    ];
    for s in negatives {
        let hits: Vec<_> = records::scan_ids(s, &["AD"], 0)
            .into_iter()
            .filter(|m| m.groups.len() == 1 && m.groups[0].len() == 4)
            .collect();
        assert!(
            hits.is_empty(),
            "{s:?} must not read as an AD citation, got {hits:?}"
        );
    }
    let positives = [
        "AD-0027",
        "AD 0027",
        "(AD 0027)",
        "\"AD 0027\"",
        "per AD 0027.",
        "`AD-0027`",
    ];
    for s in positives {
        let hits = records::scan_ids(s, &["AD"], 0);
        assert_eq!(hits.len(), 1, "{s:?} must yield exactly one AD citation");
        assert_eq!(hits[0].id, "AD-0027");
    }
    // `MADR-0027` is a MADR citation, and only that.
    let hits = records::scan_ids("see MADR-0027 and ADR-0027", KNOWN_SERIES, 0);
    assert_eq!(hits.len(), 1, "got {hits:?}");
    assert_eq!(hits[0].id, "MADR-0027");
    // A compound id keeps all of its groups rather than truncating to the
    // first, so `IG-0061-0094` never masquerades as `IG-0061`.
    let hits = records::scan_ids("IG-0061-0094", &["IG"], 0);
    assert_eq!(hits[0].id, "IG-0061-0094");
    assert_eq!(hits[0].groups.len(), 2);
}

#[test]
fn frontmatter_tags_are_not_citations() {
    let doc = "---\n\
               title: Something\n\
               tags: [decision, ADR-0027, AD-0006]\n\
               ---\n\
               \n\
               Body text with no citation at all.\n";
    let start = body_start(doc);
    assert!(start > 0, "frontmatter was not detected");
    let hits = records::scan_ids(doc, KNOWN_SERIES, start);
    assert!(
        hits.is_empty(),
        "frontmatter must not yield citations: {hits:?}"
    );
    // Without the frontmatter skip the `AD-0006` tag would be picked up; the
    // `ADR-0027` tag is rejected by shape alone, in either position.
    let all = records::scan_ids(doc, KNOWN_SERIES, 0);
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].id, "AD-0006");
}

#[test]
fn verdict_flags_the_known_wrong_citations() {
    let s = Subject::new();
    // Three passages lifted from the tree at the time this check landed.
    let cases: &[(&str, &str, &str)] = &[
        (
            "AD-0015",
            "`Confirmed` is reserved for the deferred confirmed-detection patterns (AD 0015). \
             Constructors and helpers cover the SFX confidence ladder.",
            "MADR-0015",
        ),
        (
            "AD-0019",
            "Standalone compressed files are supported for read/extract via libarchive's \
             `format_raw` binding (AD 0019); the single entry is renamed to the archive's \
             file stem.",
            "MADR-0019",
        ),
        (
            "AD-0010",
            "`ResultWithWarnings` dispatch was restored so that per-entry warnings survive \
             the unified return path (AD 0010).",
            "MADR-0010",
        ),
    ];
    for (cited, text, expect_rival) in cases {
        let offset = text
            .find("AD 0")
            .expect("fixture must contain the citation");
        let v = s
            .judge(cited, text, offset)
            .unwrap_or_else(|| panic!("{cited} in {text:?} should have been flagged"));
        assert_eq!(&v.rival.id, expect_rival);
        assert!(v.s_rival > v.s_cited);
    }
}

#[test]
fn verdict_passes_the_known_correct_citations() {
    let s = Subject::new();
    let cases: &[(&str, &str)] = &[
        // `src/ffi/wrapper.rs`: the process-wide UnRAR mutex really is AD-0019.
        (
            "AD-0019",
            "All UnRAR FFI calls are serialized behind a process-wide mutex because the \
             library is not reentrant (AD 0019). The mutex is held for the whole open \
             and read sequence.",
        ),
        // `tests/password_handling_test.rs`: deferred password validation is AD-0014.
        (
            "AD-0014",
            "Password validation is rejected as a deferred check; the wrong password must \
             surface at open time, not at extraction time (AD 0014).",
        ),
        // `docs/architecture/bootstrap-config.md`: this one only passes because
        // the tokenizer splits camelCase, so `ModificationOptions` reaches
        // AD-0020's `additive-modification-options-extension` slug.
        (
            "AD-0020",
            "`Archive::modify_with_options` takes `ModificationOptions` as an additive \
             extension of the existing entry point rather than replacing it (AD 0020).",
        ),
    ];
    for (cited, text) in cases {
        let offset = text
            .find("AD 0")
            .expect("fixture must contain the citation");
        let v = s.judge(cited, text, offset);
        assert!(
            v.is_none(),
            "{cited} in {text:?} must NOT be flagged; rival scored {:.1} vs cited {:.1}",
            v.as_ref().map(|v| v.s_rival).unwrap_or(0.0),
            v.as_ref().map(|v| v.s_cited).unwrap_or(0.0),
        );
    }
}

#[test]
fn camel_case_tokenization_is_load_bearing() {
    // The regression guard for the tokenizer split: without it the correct
    // AD-0020 citation above is flagged.
    let t = tokenize("ModificationOptions and format_raw and list_files_metadata_only");
    for want in ["modification", "format", "raw", "metadata"] {
        assert!(t.contains(want), "tokenizer lost {want:?}: {t:?}");
    }
}

#[test]
fn self_gloss_suppresses_the_finding() {
    let s = Subject::new();
    let glossed = "Encrypted creation is refused per MADR-0020 — the record older documents \
                   cite as \"AD 0020\" — and `ModificationOptions` is unaffected.";
    let offset = glossed.find("AD 0020").unwrap();
    assert!(
        s.judge("AD-0020", glossed, offset).is_none(),
        "a passage that names MADR-0020 alongside the bare id must not be flagged"
    );

    // Remove the gloss and the same text must be judged on its merits: the
    // rival's subject words now stand alone.
    let bare = "The confirmed-detection patterns for SFX stubs are deferred (AD 0015).";
    let offset = bare.find("AD 0015").unwrap();
    assert!(
        s.judge("AD-0015", bare, offset).is_some(),
        "the ungloss control must flag"
    );
    let with_gloss = "The confirmed-detection patterns for SFX stubs are deferred \
                      (AD 0015, today MADR-0015).";
    let offset = with_gloss.find("AD 0015").unwrap();
    assert!(
        s.judge("AD-0015", with_gloss, offset).is_none(),
        "naming MADR-0015 in the same sentence must suppress the finding"
    );
}

#[test]
fn translation_mirrors_are_never_scanned() {
    let scan = scan_all();
    for p in &scan.files {
        let r = rel(p);
        assert!(
            !is_translation_artifact(&r),
            "translation artifact leaked into the scan: {r}"
        );
    }
    assert!(
        SELF_EXCLUDED.contains(&"tests/record_citations_test.rs"),
        "this file must be in the self-exclusion list"
    );
    for own in SELF_EXCLUDED {
        assert!(
            own.starts_with("tests/") && own.ends_with(".rs"),
            "self-exclusion is only for the checkers' own Rust sources, not documents: {own}"
        );
        assert!(
            repo_root().join(own).exists(),
            "self-exclusion names a file that does not exist: {own}"
        );
        assert!(
            !scan.files.iter().any(|p| &rel(p) == own),
            "{own} must exclude itself; it quotes record ids verbatim"
        );
    }
}

#[test]
fn scan_is_not_vacuous() {
    let scan = scan_all();
    assert!(
        scan.files.len() >= MIN_FILES_SCANNED,
        "only {} files scanned (expected >= {MIN_FILES_SCANNED}); a scope regression would \
         make every other check pass silently",
        scan.files.len()
    );
    assert!(
        scan.total_citations >= MIN_CITATIONS,
        "only {} record citations found (expected >= {MIN_CITATIONS})",
        scan.total_citations
    );
    assert!(
        scan.band_citations >= MIN_BAND_CITATIONS,
        "only {} citations in the ambiguous band {BAND:?} (expected >= {MIN_BAND_CITATIONS})",
        scan.band_citations,
    );
}

#[test]
fn no_citation_resolves_to_a_missing_record() {
    let scan = scan_all();
    assert!(
        scan.dangling.is_empty(),
        "{n} citation(s) name a record id that does not exist:\n{body}\n({n} finding(s) total.)",
        n = scan.dangling.len(),
        body = scan.dangling.join(""),
    );
}

#[test]
fn no_citation_resolves_to_the_wrong_record() {
    let scan = scan_all();
    assert!(
        scan.subject.is_empty(),
        "{n} citation(s) name a record whose subject is not the sentence's subject:\n{body}\n\
         ({n} finding(s) total.)",
        n = scan.subject.len(),
        body = scan.subject.join(""),
    );
}

#[test]
fn waiver_table_hygiene() {
    assert!(
        HISTORICAL_DOCUMENTS.len() <= MAX_HISTORICAL_DOCUMENTS,
        "HISTORICAL_DOCUMENTS has {} entries; raise MAX_HISTORICAL_DOCUMENTS deliberately \
         if that is really intended",
        HISTORICAL_DOCUMENTS.len()
    );
    assert!(
        SUBJECT_WAIVERS.len() <= MAX_SUBJECT_WAIVERS,
        "SUBJECT_WAIVERS has {} entries; raise MAX_SUBJECT_WAIVERS deliberately",
        SUBJECT_WAIVERS.len()
    );
    assert!(
        DANGLING_WAIVERS.len() <= MAX_DANGLING_WAIVERS,
        "DANGLING_WAIVERS has {} entries; raise MAX_DANGLING_WAIVERS deliberately",
        DANGLING_WAIVERS.len()
    );

    fn check_reason(what: &str, why: &str, problems: &mut Vec<String>) {
        if why.len() < MIN_REASON_LEN {
            problems.push(format!(
                "{what}: reason is {} chars, minimum is {MIN_REASON_LEN}",
                why.len()
            ));
        }
        let lower = why.to_lowercase();
        if BANNED_REASONS.iter().any(|b| lower.trim() == *b) {
            problems.push(format!("{what}: reason {why:?} is not a reason"));
        }
    }

    let root = repo_root();
    let mut problems = Vec::new();
    for w in HISTORICAL_DOCUMENTS {
        if !root.join(w.path).exists() {
            problems.push(format!("HISTORICAL_DOCUMENTS: {} does not exist", w.path));
        }
        check_reason(
            &format!("HISTORICAL_DOCUMENTS {}", w.path),
            w.why,
            &mut problems,
        );
    }
    for w in SUBJECT_WAIVERS.iter().chain(DANGLING_WAIVERS) {
        if !root.join(w.path).exists() {
            problems.push(format!("citation waiver: {} does not exist", w.path));
        }
        if w.anchor.trim().is_empty() {
            problems.push(format!(
                "citation waiver {} {}: empty anchor",
                w.path, w.cited
            ));
        }
        check_reason(
            &format!("citation waiver {} {}", w.path, w.cited),
            w.why,
            &mut problems,
        );
    }
    assert!(
        problems.is_empty(),
        "waiver table problems:\n  {}",
        problems.join("\n  ")
    );

    // Self-pruning: a waiver whose citation was fixed, or whose file no longer
    // trips the rule, must be deleted. The tables can only shrink on their
    // own, and padding them with speculative entries fails immediately.
    let used = scan_all().used;
    let mut stale = Vec::new();
    for (i, w) in HISTORICAL_DOCUMENTS.iter().enumerate() {
        if !used.files.contains(&i) {
            stale.push(format!(
                "HISTORICAL_DOCUMENTS[{i}] {} — the file no longer trips the rule",
                w.path
            ));
        }
    }
    for (i, w) in SUBJECT_WAIVERS.iter().enumerate() {
        if !used.subjects.contains(&i) {
            stale.push(format!(
                "SUBJECT_WAIVERS[{i}] {} {} — no longer matches anything",
                w.path, w.cited
            ));
        }
    }
    for (i, w) in DANGLING_WAIVERS.iter().enumerate() {
        if !used.danglings.contains(&i) {
            stale.push(format!(
                "DANGLING_WAIVERS[{i}] {} {} — no longer matches anything",
                w.path, w.cited
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "stale waivers — delete them:\n  {}",
        stale.join("\n  ")
    );
}
