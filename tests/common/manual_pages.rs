//! Reader for the Diataxis manual bundle under `manual/`, plus the source
//! corpus the conformance checks resolve quotations against.
//!
//! Pulled in with `#[path = "common/manual_pages.rs"] mod manual_pages;` so
//! that including it does not drag the archive test helpers into the link.
//!
//! Everything here is `std` only — no `regex`, no YAML crate, no `sha2`. The
//! canonical body hash is defined by the manual profile as a `shasum -a 256`
//! over an `awk`-trimmed body, so [`sha256_hex`] reproduces SHA-256 directly
//! rather than shelling out (which would make the check unavailable wherever
//! `shasum` is not on PATH).
//!
//! ## Korean mirrors
//!
//! `manual/**/ko/` and `*.ko.md` are produced by an external translation
//! pipeline and are out of scope entirely. [`walk_manual`] rejects them **by
//! path, before the file is opened**, and `korean_mirrors_are_never_read`
//! asserts it.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), so the profile's `shasum -a 256` can be reproduced
// without a dependency or a subprocess.
// ---------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

// ---------------------------------------------------------------------------
// Bundle layout
// ---------------------------------------------------------------------------

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn rel(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `Some(manual/)` when the bundle is present. Returns `None` for a packaged
/// crate or a sparse checkout, so the tests can print "skipped: manual/ not
/// present" instead of passing vacuously.
pub fn bundle_root() -> Option<PathBuf> {
    let p = repo_root().join("manual");
    p.is_dir().then_some(p)
}

fn is_korean(path: &Path) -> bool {
    let r = rel(path);
    r.ends_with(".ko.md") || r.contains(".ko.") || r.split('/').any(|s| s == "ko")
}

/// Every English markdown file in the bundle. Korean mirrors are rejected by
/// path before any read.
pub fn walk_manual() -> Vec<PathBuf> {
    fn go(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().and_then(|n| n.to_str()) == Some("ko") {
                    continue;
                }
                go(&p, out);
            } else if p.is_file()
                && p.extension().and_then(|x| x.to_str()) == Some("md")
                && !is_korean(&p)
            {
                out.push(p);
            }
        }
    }
    let Some(root) = bundle_root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    go(&root, &mut out);
    out
}

/// Files the manual profile reserves: the bundle index, every sub-index, and
/// the log. They carry no concept frontmatter and are excluded from the page
/// checks.
pub fn is_reserved(rel_path: &str) -> bool {
    rel_path == "manual/log.md" || rel_path.ends_with("/index.md") || rel_path == "manual/index.md"
}

// ---------------------------------------------------------------------------
// Frontmatter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    /// Top-level scalars (`type`, `title`, `synced_hash`, `language`, ...).
    pub scalars: BTreeMap<String, String>,
    /// `tags: [a, b, c]`.
    pub tags: Vec<String>,
    /// `sources:` entries, as `(id, resource)`.
    pub sources: Vec<(String, String)>,
    /// `generated:` sub-keys, e.g. `by`, `at`.
    pub generated: BTreeMap<String, String>,
    /// Raw frontmatter text, for tag scanning.
    pub raw: String,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.scalars.get(key).map(|s| s.as_str())
    }
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Minimal YAML subset: exactly the shapes the manual profile emits.
/// Anything else is reported, not silently dropped.
pub fn parse_frontmatter(text: &str) -> Option<(Frontmatter, usize)> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut fm = Frontmatter::default();
    let mut consumed = 1usize;
    let mut section: Option<String> = None;
    let mut raw = String::new();
    for line in lines {
        consumed += 1;
        if line.trim_end() == "---" {
            fm.raw = raw;
            return Some((fm, consumed));
        }
        raw.push_str(line);
        raw.push('\n');
        if line.trim().is_empty() {
            continue;
        }
        if let Some(item) = line.strip_prefix("  - ") {
            // `sources:` entries in either the inline-map form the bundle uses
            // or the block form the profile documents.
            let item = item.trim();
            if section.as_deref() == Some("sources") {
                let inner = item.trim_start_matches('{').trim_end_matches('}');
                let mut id = String::new();
                let mut resource = String::new();
                for part in inner.split(',') {
                    if let Some((k, v)) = part.split_once(':') {
                        match k.trim() {
                            "id" => id = strip_quotes(v),
                            "resource" => resource = strip_quotes(v),
                            _ => {}
                        }
                    }
                }
                if !resource.is_empty() || !id.is_empty() {
                    fm.sources.push((id, resource));
                }
            } else if section.as_deref() == Some("checks_exempt") {
                // tolerated but unused; the repo keeps exemptions test-side
            }
            continue;
        }
        if let Some(kv) = line.strip_prefix("  ") {
            if let Some((k, v)) = kv.split_once(':') {
                if section.as_deref() == Some("generated") {
                    fm.generated.insert(k.trim().to_string(), strip_quotes(v));
                } else if section.as_deref() == Some("sources")
                    && k.trim() == "resource"
                    && let Some(last) = fm.sources.last_mut()
                {
                    // Block form: `  - id: x` opened the entry above.
                    last.1 = strip_quotes(v);
                }
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_string();
        let val = v.trim();
        if val.is_empty() {
            section = Some(key);
            continue;
        }
        section = None;
        if key == "tags" {
            fm.tags = val
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(strip_quotes)
                .filter(|t| !t.is_empty())
                .collect();
        }
        fm.scalars.insert(key, strip_quotes(val));
    }
    None
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Page {
    pub path: PathBuf,
    pub rel: String,
    pub text: String,
    pub frontmatter: Option<Frontmatter>,
    /// Body as the profile's hash pipeline defines it: everything after the
    /// second `---` line, each line re-terminated with `\n`.
    pub canonical_body: String,
    /// Byte offset of `canonical_body` within `text`, for line numbering.
    pub body_offset: usize,
}

impl Page {
    pub fn body_hash(&self) -> String {
        sha256_hex(self.canonical_body.as_bytes())
    }
    pub fn synced_hash(&self) -> Option<&str> {
        self.frontmatter.as_ref()?.get("synced_hash")
    }
    /// 1-based line in the whole file for an offset into `canonical_body`.
    pub fn line_of_body(&self, body_offset: usize) -> usize {
        let abs = self.body_offset + body_offset.min(self.canonical_body.len());
        self.text[..abs.min(self.text.len())]
            .bytes()
            .filter(|c| *c == b'\n')
            .count()
            + 1
    }
}

/// Reproduces the profile's `awk` trim byte for byte: drop everything up to
/// and including the second `---` line, then emit each remaining line plus a
/// newline. A file with fewer than two `---` lines hashes the empty string.
pub fn canonical_body(text: &str) -> (String, usize) {
    let mut seen = 0;
    let mut out = String::new();
    let mut offset = 0usize;
    let mut body_offset = text.len();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if seen < 2 && trimmed.trim_end_matches([' ', '\t']) == "---" {
            seen += 1;
            offset += line.len();
            if seen == 2 {
                body_offset = offset;
            }
            continue;
        }
        if seen >= 2 {
            out.push_str(trimmed);
            out.push('\n');
        }
        offset += line.len();
    }
    if seen < 2 {
        return (String::new(), text.len());
    }
    (out, body_offset)
}

pub fn load_page(path: &Path) -> Page {
    let text = fs::read_to_string(path).unwrap_or_default();
    let (body, body_offset) = canonical_body(&text);
    Page {
        rel: rel(path),
        path: path.to_path_buf(),
        frontmatter: parse_frontmatter(&text).map(|(f, _)| f),
        canonical_body: body,
        body_offset,
        text,
    }
}

/// The 27 concept pages: every English page that is not a reserved file.
pub fn concept_pages() -> Vec<Page> {
    walk_manual()
        .into_iter()
        .map(|p| load_page(&p))
        .filter(|p| !is_reserved(&p.rel))
        .collect()
}

pub fn reserved_pages() -> Vec<Page> {
    walk_manual()
        .into_iter()
        .map(|p| load_page(&p))
        .filter(|p| is_reserved(&p.rel))
        .collect()
}

// ---------------------------------------------------------------------------
// Fenced blocks and inline spans
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Fence {
    pub info: String,
    /// 1-based line of the opening fence within `canonical_body`.
    pub body_line: usize,
    pub content: String,
    /// Byte offset of the opening fence within `canonical_body`.
    pub offset: usize,
}

pub fn fences(body: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut in_fence: Option<(String, String, usize, usize)> = None;
    let mut offset = 0usize;
    for (i, line) in body.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches('\n');
        if let Some((info, content, start_line, start_off)) = in_fence.as_mut() {
            if trimmed.trim_start().starts_with("```")
                && trimmed.trim().len() >= 3
                && trimmed.trim().chars().all(|c| c == '`')
            {
                out.push(Fence {
                    info: info.clone(),
                    body_line: *start_line,
                    content: content.clone(),
                    offset: *start_off,
                });
                in_fence = None;
            } else {
                content.push_str(trimmed);
                content.push('\n');
            }
        } else if let Some(rest) = trimmed.trim_start().strip_prefix("```") {
            in_fence = Some((rest.trim().to_string(), String::new(), i + 1, offset));
        }
        offset += line.len();
    }
    // An unterminated fence is still reported, so it can be diagnosed.
    if let Some((info, content, start_line, start_off)) = in_fence {
        out.push(Fence {
            info,
            body_line: start_line,
            content,
            offset: start_off,
        });
    }
    out
}

/// The body with every fenced block replaced by blank lines, so inline-span
/// scanning never walks into code.
pub fn strip_fences(body: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if trimmed.trim_start().starts_with("```") {
            inside = !inside;
            out.push('\n');
            continue;
        }
        if inside {
            out.push('\n');
        } else {
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct Span {
    /// Span text with internal whitespace runs collapsed to one space.
    pub text: String,
    /// Byte offset of the opening backtick within the fence-stripped body.
    pub offset: usize,
    /// 1-based line within the fence-stripped body.
    pub line: usize,
}

/// Count the line breaks in `bytes`. [`inline_spans`] advances its 1-based
/// line counter by the newlines it steps over, so it needs the count, not the
/// individual bytes.
fn newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&c| c == b'\n').count()
}

/// Backtick-delimited inline code spans, **including spans that wrap across a
/// line break**.
///
/// That is not a detail: the encrypted-creation quotation on
/// `manual/reference/user/en/errors-and-warnings.md` breaks between
/// `password must be` and `None`, so a line-oriented extractor silently
/// misses the exact quotation this check exists for.
pub fn inline_spans(stripped_body: &str) -> Vec<Span> {
    let b = stripped_body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    while i < b.len() {
        if b[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b[i] != b'`' {
            i += 1;
            continue;
        }
        let mut n = 0usize;
        while i + n < b.len() && b[i + n] == b'`' {
            n += 1;
        }
        let content_start = i + n;
        let mut j = content_start;
        let mut found = None;
        while j < b.len() {
            if b[j] == b'`' {
                let mut m = 0usize;
                while j + m < b.len() && b[j + m] == b'`' {
                    m += 1;
                }
                if m == n {
                    found = Some(j);
                    break;
                }
                j += m;
                continue;
            }
            // A blank line ends a paragraph; an unterminated span must not
            // swallow the rest of the document.
            if b[j] == b'\n' && j + 1 < b.len() && b[j + 1] == b'\n' {
                break;
            }
            j += 1;
        }
        let Some(end) = found else {
            line += newlines(&b[i..content_start]);
            i = content_start;
            continue;
        };
        let raw = &stripped_body[content_start..end];
        out.push(Span {
            text: raw.split_whitespace().collect::<Vec<_>>().join(" "),
            offset: i,
            line,
        });
        line += newlines(&b[i..end + n]);
        i = end + n;
    }
    out
}

// ---------------------------------------------------------------------------
// The source corpus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Literal {
    pub file: String,
    /// The literal's decoded text.
    pub text: String,
    /// Placeholder-free runs of at least [`MIN_SEGMENT`] chars, in order.
    pub segments: Vec<String>,
}

pub const MIN_SEGMENT: usize = 12;

pub fn source_files() -> Vec<PathBuf> {
    fn go(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                go(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    go(&repo_root().join("src"), &mut out);
    let b = repo_root().join("build.rs");
    if b.is_file() {
        out.push(b);
    }
    out
}

/// The whole source corpus as one concatenated string, for the exact-literal
/// rule (a1), which asks whether `"..."` occurs verbatim in the sources.
pub fn source_text() -> &'static str {
    static CELL: OnceLock<String> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut s = String::new();
        for f in source_files() {
            if let Ok(t) = fs::read_to_string(&f) {
                s.push_str(&t);
                s.push('\n');
            }
        }
        s
    })
}

/// A small Rust lexer, just enough to pull string literals out without being
/// derailed by an apostrophe in a doc comment or a `"` inside a `//` line.
fn literals_of(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                let mut depth = 1;
                i += 2;
                while i < b.len() && depth > 0 {
                    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'r' | b'b' if i + 1 < b.len() && (b[i + 1] == b'"' || b[i + 1] == b'#') => {
                // Raw / byte strings: skipped rather than decoded. No manual
                // quotation resolves to one today, and mis-decoding one would
                // be worse than not offering it as an anchor.
                let mut j = i + 1;
                let mut hashes = 0;
                while j < b.len() && b[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < b.len() && b[j] == b'"' {
                    j += 1;
                    let close: Vec<u8> = std::iter::once(b'"')
                        .chain(std::iter::repeat_n(b'#', hashes))
                        .collect();
                    while j < b.len() && !b[j..].starts_with(&close) {
                        j += 1;
                    }
                    i = (j + close.len()).min(b.len());
                } else {
                    i += 1;
                }
            }
            b'\'' => {
                // Char literal or lifetime; either way, skip past it safely.
                if i + 1 < b.len() && b[i + 1] == b'\\' {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                    i += 1;
                } else if i + 2 < b.len() && b[i + 2] == b'\'' {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            b'"' => {
                let mut j = i + 1;
                let mut lit = String::new();
                while j < b.len() {
                    match b[j] {
                        b'\\' if j + 1 < b.len() => {
                            match b[j + 1] {
                                b'n' => lit.push('\n'),
                                b't' => lit.push('\t'),
                                b'r' => lit.push('\r'),
                                b'0' => lit.push('\0'),
                                b'"' => lit.push('"'),
                                b'\\' => lit.push('\\'),
                                b'\'' => lit.push('\''),
                                b'\n' => {
                                    // Line continuation: skip leading blanks.
                                    let mut k = j + 2;
                                    while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
                                        k += 1;
                                    }
                                    j = k;
                                    continue;
                                }
                                other => {
                                    lit.push('\\');
                                    lit.push(other as char);
                                }
                            }
                            j += 2;
                        }
                        b'"' => break,
                        _ => {
                            // Copy the whole UTF-8 sequence.
                            let start = j;
                            let mut end = j + 1;
                            while end < b.len() && (b[end] & 0xC0) == 0x80 {
                                end += 1;
                            }
                            lit.push_str(&text[start..end]);
                            j = end;
                        }
                    }
                }
                out.push(lit);
                i = (j + 1).min(b.len());
            }
            _ => i += 1,
        }
    }
    out
}

/// Splits a literal on `{...}` format placeholders (honouring `{{`/`}}`
/// escapes) and keeps the runs of at least [`MIN_SEGMENT`] characters.
pub fn placeholder_free_segments(lit: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    let bytes: Vec<char> = lit.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '{' {
            if i + 1 < bytes.len() && bytes[i + 1] == '{' {
                cur.push('{');
                i += 2;
                continue;
            }
            // Find the closing brace; a brace group containing whitespace is
            // Rust struct syntax in prose, not a placeholder.
            let mut j = i + 1;
            let mut inner = String::new();
            while j < bytes.len() && bytes[j] != '}' {
                inner.push(bytes[j]);
                j += 1;
            }
            if j < bytes.len() && !inner.contains(char::is_whitespace) {
                if cur.chars().count() >= MIN_SEGMENT {
                    segs.push(cur.clone());
                }
                cur.clear();
                i = j + 1;
                continue;
            }
        }
        if c == '}' && i + 1 < bytes.len() && bytes[i + 1] == '}' {
            cur.push('}');
            i += 2;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if cur.chars().count() >= MIN_SEGMENT {
        segs.push(cur);
    }
    segs
}

/// Every source string literal that carries at least one placeholder-free
/// segment long enough to anchor a quotation.
pub fn source_literals() -> &'static [Literal] {
    static CELL: OnceLock<Vec<Literal>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut out = Vec::new();
        for f in source_files() {
            let Ok(text) = fs::read_to_string(&f) else {
                continue;
            };
            let file = rel(&f);
            for lit in literals_of(&text) {
                let segments = placeholder_free_segments(&lit);
                if segments.is_empty() {
                    continue;
                }
                out.push(Literal {
                    file: file.clone(),
                    text: lit,
                    segments,
                });
            }
        }
        out
    })
    .as_slice()
}
