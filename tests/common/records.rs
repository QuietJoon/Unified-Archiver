//! Shared, read-only view of the decision-record catalogue under
//! `docs/records/`, plus the boundary-anchored id scanner both record-hygiene
//! test targets depend on.
//!
//! This module is pulled in with `#[path = "common/records.rs"] mod records;`
//! rather than through `tests/common/mod.rs`, so that including it does not
//! drag the archive test helpers (and their fixtures) into the link.
//!
//! ## Ownership split
//!
//! Two tickets touch record ids and they must not implement the same check:
//!
//! * **c8d771eb** (`tests/manual_conformance.rs`) owns *resolution* — does a
//!   cited id exist at all — scoped to `manual/**/en/`.
//! * **dd119253** (`tests/record_citations_test.rs`) owns *subject matching*
//!   repo-wide, and resolution everywhere **except** `manual/`.
//!
//! This file is the single index both consume, so the catalogue is parsed,
//! shape-asserted and alias-resolved exactly once.
//!
//! ## Why the scanner is hand-rolled
//!
//! `ADR-NNNN` is a bundle-wide *tag* namespace: `MADR-0027`'s own frontmatter
//! reads `tags: [decision, ADR-0027, ...]`. A substring search for `AD-0027`
//! matches inside both `MADR-0027` and `ADR-0027`. The crate carries no
//! `regex` dependency, and `regex` has no lookbehind anyway, so
//! [`scan_ids`] is a boundary-anchored byte scan.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Repo layout
// ---------------------------------------------------------------------------

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Repo-relative, forward-slashed path.
pub fn rel(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Korean sibling documents and their mirror trees are produced by an
/// external pipeline and are out of scope entirely. Rejected by path, before
/// the file is ever opened.
pub fn is_translation_artifact(rel_path: &str) -> bool {
    rel_path.ends_with(".ko.md")
        || rel_path.contains(".ko.")
        || rel_path.split('/').any(|seg| seg == "ko")
        || rel_path.contains(".translate/")
        || rel_path.contains(".translation-")
}

// ---------------------------------------------------------------------------
// The generated catalogue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// Path relative to `docs/records/`.
    pub file: String,
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Reads `docs/records/index.yaml` against a strict line grammar.
///
/// A hand-rolled reader is used because the crate has no YAML dependency and
/// adding one (`serde_yaml` is unmaintained) for two test targets is not
/// worth it. The shape assertion is the guard: if the 2026-07-23 generator's
/// output format ever changes this fails loudly instead of silently parsing
/// zero records — which would make every check downstream pass vacuously.
pub fn parse_index(text: &str) -> Result<Vec<Record>, String> {
    let mut records: Vec<Record> = Vec::new();
    let mut cur: Option<(String, BTreeMap<String, String>)> = None;
    let mut saw_header = false;

    fn finish(
        cur: Option<(String, BTreeMap<String, String>)>,
        out: &mut Vec<Record>,
    ) -> Result<(), String> {
        let Some((id, fields)) = cur else {
            return Ok(());
        };
        for key in ["type", "title", "status", "file"] {
            if !fields.contains_key(key) {
                return Err(format!("record `{id}` is missing required key `{key}:`"));
            }
        }
        out.push(Record {
            id: id.clone(),
            kind: fields["type"].clone(),
            title: unquote(&fields["title"]),
            file: unquote(&fields["file"]),
        });
        Ok(())
    }

    for (n, line) in text.lines().enumerate() {
        let lineno = n + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "records:" {
            saw_header = true;
            continue;
        }
        if let Some(id) = line.strip_prefix("  - id: ") {
            let id = id.trim();
            // `.` is allowed for the three compound IG range ids such as
            // `IG-0061-0094..0123`.
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            {
                return Err(format!("line {lineno}: unparseable record id `{id}`"));
            }
            finish(cur.take(), &mut records)?;
            cur = Some((id.to_string(), BTreeMap::new()));
            continue;
        }
        if let Some(kv) = line.strip_prefix("    ") {
            if kv.starts_with(' ') {
                return Err(format!("line {lineno}: unexpected indentation: {line:?}"));
            }
            let Some((k, v)) = kv.split_once(": ") else {
                return Err(format!("line {lineno}: not a `key: value` pair: {line:?}"));
            };
            let Some((_, fields)) = cur.as_mut() else {
                return Err(format!("line {lineno}: field outside a record: {line:?}"));
            };
            fields.insert(k.trim().to_string(), v.trim().to_string());
            continue;
        }
        return Err(format!(
            "line {lineno}: does not match the index.yaml line grammar: {line:?}"
        ));
    }
    finish(cur, &mut records)?;
    if !saw_header {
        return Err("index.yaml has no top-level `records:` key".to_string());
    }
    Ok(records)
}

/// Reads `docs/records/redirects.yaml`: the authority for what a
/// pre-consolidation `AD 00NN` used to mean.
pub fn parse_redirects(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut saw_header = false;
    for (n, line) in text.lines().enumerate() {
        let lineno = n + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "redirects:" || line == "legacy_log_split:" {
            saw_header = true;
            continue;
        }
        let Some(body) = line.strip_prefix("  ") else {
            return Err(format!(
                "line {lineno}: does not match the redirects.yaml line grammar: {line:?}"
            ));
        };
        if body.starts_with(' ') {
            return Err(format!("line {lineno}: unexpected indentation: {line:?}"));
        }
        let (key, value) = if let Some(rest) = body.strip_prefix('"') {
            let Some(end) = rest.find('"') else {
                return Err(format!("line {lineno}: unterminated quoted key: {line:?}"));
            };
            let key = &rest[..end];
            let Some(v) = rest[end + 1..].strip_prefix(": ") else {
                return Err(format!("line {lineno}: quoted key without value: {line:?}"));
            };
            (key.to_string(), v.trim().to_string())
        } else {
            let Some((k, v)) = body.split_once(": ") else {
                return Err(format!("line {lineno}: not a `key: value` pair: {line:?}"));
            };
            (k.trim().to_string(), v.trim().to_string())
        };
        out.insert(key, value);
    }
    if !saw_header {
        return Err("redirects.yaml has no top-level `redirects:` key".to_string());
    }
    Ok(out)
}

pub fn index_records() -> &'static [Record] {
    static CELL: OnceLock<Vec<Record>> = OnceLock::new();
    CELL.get_or_init(|| {
        let path = repo_root().join("docs/records/index.yaml");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        parse_index(&text).unwrap_or_else(|e| panic!("docs/records/index.yaml: {e}"))
    })
    .as_slice()
}

pub fn redirects() -> &'static BTreeMap<String, String> {
    static CELL: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    CELL.get_or_init(|| {
        let path = repo_root().join("docs/records/redirects.yaml");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        parse_redirects(&text).unwrap_or_else(|e| panic!("docs/records/redirects.yaml: {e}"))
    })
}

/// Every id that resolves to a record, keyed by id.
///
/// Built from `index.yaml` and then widened with ids derived from the actual
/// filenames in `docs/records/`. The widening matters: the index carries
/// `IG-0061-0094..0123` while prose and filenames write
/// `IG-0061-0094-0123`, and revision-suffixed filenames such as
/// `MADR-0014-r0001-...` are cited both with and without the `-r0001`.
pub fn record_index() -> &'static BTreeMap<String, Record> {
    static CELL: OnceLock<BTreeMap<String, Record>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut map: BTreeMap<String, Record> = BTreeMap::new();
        for r in index_records() {
            map.insert(r.id.clone(), r.clone());
        }
        // Filename-derived aliases. Only *added*; never overrides the index.
        let dir = repo_root().join("docs/records");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".md") || name.ends_with(".ko.md") {
                    continue;
                }
                let Some(prefix) = filename_id_prefix(&name) else {
                    continue;
                };
                let rec = index_records()
                    .iter()
                    .find(|r| r.file == name)
                    .cloned()
                    .unwrap_or_else(|| Record {
                        id: prefix.clone(),
                        kind: prefix.split('-').next().unwrap_or("").to_string(),
                        title: name.trim_end_matches(".md").replace('-', " "),
                        file: name.clone(),
                    });
                map.entry(prefix).or_insert(rec);
            }
        }
        map
    })
}

/// `AD-0019-unrar-...md` -> `AD-0019`; `IG-0061-0094-0123-...md` ->
/// `IG-0061-0094-0123`; `MADR-0014-r0001-...md` -> `MADR-0014`.
pub fn filename_id_prefix(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    if !(2..=5).contains(&i) {
        return None;
    }
    let series_end = i;
    let mut end = series_end;
    loop {
        if i >= bytes.len() || bytes[i] != b'-' {
            break;
        }
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == i + 1 {
            break;
        }
        end = j;
        i = j;
    }
    if end == series_end {
        return None;
    }
    Some(name[..end].to_string())
}

/// Ledger ids that live outside `docs/records/`: `OI-*` open issues and
/// `DEF-*` deferred-work stubs.
pub fn ledger_ids() -> &'static BTreeSet<String> {
    static CELL: OnceLock<BTreeSet<String>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut out = BTreeSet::new();
        let root = repo_root();
        let sources = [
            "docs/project/open-issues.md",
            "docs/project/open-issues-resolved.md",
            "docs/project/stub-manifest.md",
        ];
        for s in sources {
            let Ok(text) = fs::read_to_string(root.join(s)) else {
                continue;
            };
            for m in scan_ids(&text, &["OI", "DEF"], 0) {
                out.insert(m.id);
            }
        }
        // The index itself lists the OI ids a record answers.
        for r in index_records() {
            let _ = r;
        }
        if let Ok(text) = fs::read_to_string(root.join("docs/records/index.yaml")) {
            for m in scan_ids(&text, &["OI", "DEF"], 0) {
                out.insert(m.id);
            }
        }
        out
    })
}

// ---------------------------------------------------------------------------
// The boundary-anchored id scanner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdMatch {
    /// Series as written, e.g. `AD`, `MADR`, `DCR`, `OI`.
    pub series: String,
    /// Canonical id: series and every digit group joined by `-`.
    pub id: String,
    /// The digit groups, in order.
    pub groups: Vec<String>,
    /// Byte offset of the first byte of the series token.
    pub offset: usize,
    /// The separator actually written between series and first group.
    pub separator: char,
    /// Exactly the bytes matched.
    pub raw: String,
}

/// Boundary-anchored scan for `SERIES[- ]DIGITS(-DIGITS)*`.
///
/// The three rejections that matter, in order:
///
/// * the byte before the series must not be ASCII alphanumeric or `_` — this
///   is what stops `MADR-0027` from reading as an `AD-0027` citation;
/// * the byte after the series must be `-` or ` ` — this is what stops the
///   `ADR-NNNN` tag namespace, whose next byte is `R`;
/// * the match must not be followed by an alphanumeric or `_`.
///
/// `from` lets a caller start after a document's YAML frontmatter, which is
/// where the `ADR-NNNN` tag namespace lives.
pub fn scan_ids(text: &str, series_list: &[&str], from: usize) -> Vec<IdMatch> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    // Longest series first, so `MADR` wins over a hypothetical `MA`.
    let mut ordered: Vec<&&str> = series_list.iter().collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));

    let mut i = from;
    'outer: while i < b.len() {
        for series in &ordered {
            let s = series.as_bytes();
            if !b[i..].starts_with(s) {
                continue;
            }
            if i > 0 {
                let prev = b[i - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    continue;
                }
            }
            let mut j = i + s.len();
            if j >= b.len() {
                continue;
            }
            let sep = b[j];
            if sep != b'-' && sep != b' ' {
                continue;
            }
            j += 1;
            let g0 = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == g0 {
                continue;
            }
            let mut groups = vec![text[g0..j].to_string()];
            // Further `-DIGITS` groups (compound ids such as `IG-0061-0094`).
            loop {
                if j + 1 < b.len() && b[j] == b'-' && b[j + 1].is_ascii_digit() {
                    let start = j + 1;
                    let mut k = start;
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    groups.push(text[start..k].to_string());
                    j = k;
                } else {
                    break;
                }
            }
            if j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                // e.g. `AD-0027x`; not an id.
                continue;
            }
            out.push(IdMatch {
                series: (*series).to_string(),
                id: format!("{}-{}", series, groups.join("-")),
                groups,
                offset: i,
                separator: sep as char,
                raw: text[i..j].to_string(),
            });
            i = j;
            continue 'outer;
        }
        i += 1;
    }
    out
}

/// Byte offset where the body begins: after a leading YAML frontmatter block.
pub fn body_start(text: &str) -> usize {
    if !text.as_bytes().starts_with(b"---\n") {
        return 0;
    }
    match text[4..].find("\n---") {
        Some(i) => {
            let after = 4 + i + 4;
            match text.get(after.min(text.len())..).and_then(|t| t.find('\n')) {
                Some(j) => (after + j + 1).min(text.len()),
                None => text.len(),
            }
        }
        None => 0,
    }
}

pub fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|c| *c == b'\n')
        .count()
        + 1
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    if i > s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// `radius` bytes either side of `offset`, snapped to char boundaries.
pub fn window(text: &str, offset: usize, radius: usize) -> &str {
    let lo = floor_boundary(text, offset.saturating_sub(radius));
    let hi = ceil_boundary(text, offset.saturating_add(radius));
    &text[lo..hi]
}
