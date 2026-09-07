//! The libarchive header-status policy must stay in one place.
//!
//! OI-0080-006 / ticgit 12d431: `src/ffi/libarchive_wrapper/reader.rs` had
//! eight `archive_read_next_header` call sites that each decided for
//! themselves what to do with `ARCHIVE_WARN`. Three refused it and five
//! accepted it, so the same archive could pass `validate_integrity()` and be
//! rejected by `list_files()` — the answer depended only on which method the
//! caller reached for. Nothing in the type system prevented that, and nothing
//! prevented it drifting apart again after it was fixed.
//!
//! This is that missing guard. It reads the source rather than the behaviour,
//! because the defect *is* a source-shape defect: a ninth call site added
//! tomorrow with its own inline status check would reintroduce exactly the
//! divergence, while every behavioural test still passed.
//!
//! `ARCHIVE_WARN` means libarchive recovered and the header is usable. The
//! single policy is: accept it, and keep the message
//! (`ArchiveWarning::BackendAdvisory`) rather than discarding it.

use std::path::PathBuf;

fn reader_source() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/ffi/libarchive_wrapper/reader.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Lines of `reader.rs` that are actual code, paired with their 1-based
/// number. Doc comments mention the FFI function by name on purpose.
fn code_lines(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.starts_with("//"))
        .collect()
}

#[test]
fn every_header_read_goes_through_the_single_policy_helper() {
    let source = reader_source();
    let calls: Vec<_> = code_lines(&source)
        .into_iter()
        .filter(|(_, line)| line.contains("archive_read_next_header("))
        .collect();

    assert_eq!(
        calls.len(),
        1,
        "every `archive_read_next_header` call must go through \
         `next_header_status`, which is the one place the ARCHIVE_WARN policy \
         is decided. Found {} call sites: {:?}. If you are adding a read walk, \
         call the helper; if you genuinely need a different policy, change it \
         there so every walk moves together.",
        calls.len(),
        calls
    );

    // And that one call has to be inside the helper, not merely alone.
    let helper_at = source
        .find("unsafe fn next_header_status(")
        .expect("next_header_status must exist");
    let helper_line = source[..helper_at].lines().count() + 1;
    let (call_line, _) = calls[0];
    assert!(
        call_line > helper_line && call_line - helper_line < 40,
        "the sole `archive_read_next_header` call (line {call_line}) must sit \
         inside `next_header_status` (line {helper_line}), not somewhere else \
         that merely happens to be unique"
    );
}

#[test]
fn no_read_walk_compares_the_warn_status_on_its_own() {
    let source = reader_source();
    let comparisons: Vec<_> = code_lines(&source)
        .into_iter()
        .filter(|(_, line)| line.contains("ARCHIVE_WARN"))
        .collect();

    // Two are legitimate: `next_header_status` classifies the header status,
    // and `checked_data_skip` applies the same accept-on-warn rule to
    // `archive_read_data_skip`, which is a different libarchive call with its
    // own return.
    assert!(
        comparisons.len() <= 2,
        "ARCHIVE_WARN is compared in {} places: {:?}. The header policy lives \
         in `next_header_status` and the skip policy in `checked_data_skip`; a \
         third site is a walk deciding for itself again.",
        comparisons.len(),
        comparisons
    );
}

// ---------------------------------------------------------------------------
// The behavioural half: the policy, exercised against an archive that really
// makes libarchive return ARCHIVE_WARN.
// ---------------------------------------------------------------------------

#[path = "common/mod.rs"]
mod common;

#[cfg_attr(not(feature = "libarchive"), allow(unused_imports))]
use unified_archive::{Archive, ArchiveWarning};

#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn block(payload: &[u8]) -> Vec<u8> {
    let mut b = payload.to_vec();
    b.resize(512, 0);
    b
}

/// One ustar header block. The checksum field counts as spaces while summing,
/// which is the format's own rule.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn ustar_header(name: &[u8], size: usize, typeflag: u8) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..name.len()].copy_from_slice(name);
    h[100..108].copy_from_slice(b"0000644\0");
    h[108..116].copy_from_slice(b"0000000\0");
    h[116..124].copy_from_slice(b"0000000\0");
    h[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    h[136..148].copy_from_slice(b"00000000000\0");
    h[148..156].copy_from_slice(b"        ");
    h[156] = typeflag;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    h
}

/// A tar whose pax extended header holds a record that is not
/// `LEN key=value\n`. libarchive reports "Ignoring malformed pax extended
/// attribute", **recovers**, and hands back the following entry — i.e. it
/// returns `ARCHIVE_WARN` from `archive_read_next_header`, which is exactly
/// the status the eight read walks used to disagree about.
///
/// Verified against the system `bsdtar` (libarchive's own CLI), which lists
/// `f.txt` and prints that warning.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn malformed_pax_tar() -> Vec<u8> {
    let bad_record = b"this is not a pax attribute record at all\n";
    let payload = b"hello\n";
    let mut tar = Vec::new();
    tar.extend(ustar_header(b"PaxHeaders/f.txt", bad_record.len(), b'x'));
    tar.extend(block(bad_record));
    tar.extend(ustar_header(b"f.txt", payload.len(), b'0'));
    tar.extend(block(payload));
    tar.extend(block(b""));
    tar.extend(block(b""));
    tar
}

#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn staged_malformed_pax_tar() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = common::temp_test_dir();
    let archive = dir.join("malformed_pax.tar");
    std::fs::write(&archive, malformed_pax_tar()).expect("write fixture");
    (dir, archive)
}

#[cfg(feature = "integrity")]
/// The divergence itself: listing and integrity must agree about one archive.
///
/// Before this fix `list_files()` refused `ARCHIVE_WARN` while
/// `validate_integrity()` accepted it, so these two calls returned opposite
/// verdicts about the same bytes.
#[cfg(feature = "libarchive")]
#[test]
fn listing_and_integrity_agree_on_an_archive_libarchive_warns_about() {
    let (dir, archive_path) = staged_malformed_pax_tar();

    let archive = Archive::open(&archive_path).expect("a recovered header must still open");

    let entries = archive
        .list_files()
        .expect("list_files must accept a header libarchive recovered from");
    assert_eq!(
        entries.len(),
        1,
        "the recovered entry must still be listed: {entries:?}"
    );
    assert_eq!(entries[0].path, "f.txt");

    archive
        .validate_integrity()
        .expect("validate_integrity accepted this before the fix, and must still");

    common::cleanup(&dir);
}

/// Accepting the status is only half of it. The message libarchive attached
/// was discarded at every accepting site, so a caller was told the archive was
/// fine with no way to learn what had been objected to.
#[cfg(feature = "libarchive")]
#[test]
fn the_warning_text_reaches_the_caller_instead_of_being_discarded() {
    let (dir, archive_path) = staged_malformed_pax_tar();
    let dest = dir.join("out");
    std::fs::create_dir_all(&dest).expect("create destination");

    let archive = Archive::open(&archive_path).expect("open");
    let outcome = archive
        .extract_all(common::default_extraction_options(&dest))
        .expect("extraction must succeed on a header libarchive recovered from");

    let advisories: Vec<_> = outcome
        .warnings
        .iter()
        .filter_map(|warning| match warning {
            ArchiveWarning::BackendAdvisory {
                backend, message, ..
            } => Some((*backend, message.clone())),
            _ => None,
        })
        .collect();

    assert!(
        !advisories.is_empty(),
        "libarchive objected to this archive; extract_all must carry what it \
         said, not swallow it. warnings: {:?}",
        outcome.warnings
    );
    assert!(
        advisories
            .iter()
            .all(|(backend, _)| *backend == "libarchive"),
        "advisories must name their source: {advisories:?}"
    );
    assert!(
        advisories
            .iter()
            .any(|(_, message)| message.to_lowercase().contains("pax")),
        "the advisory must be libarchive's own text about this archive, \
         verbatim: {advisories:?}"
    );

    // And the entry really was extracted — a warning is not a refusal.
    assert!(
        dest.join("f.txt").is_file(),
        "the recovered entry must still be written"
    );

    common::cleanup(&dir);
}

/// `extract_all` has a warning channel; the other reads do not, and that is
/// the whole reason the sink exists. A caller who lists an archive must still
/// be able to find out that libarchive objected to it.
#[cfg(feature = "libarchive")]
#[test]
fn a_listing_caller_can_still_reach_what_the_backend_said() {
    let (dir, archive_path) = staged_malformed_pax_tar();
    let archive = Archive::open(&archive_path).expect("open");

    // Nothing has been read past the eager open probe yet.
    let before = archive.take_backend_warnings();

    let entries = archive.list_files().expect("list_files");
    assert_eq!(entries.len(), 1);

    let after = archive.take_backend_warnings();
    assert!(
        !before.is_empty() || !after.is_empty(),
        "the listing walk hit a header libarchive warned about; \
         take_backend_warnings must surface it"
    );
    assert!(
        before.iter().chain(after.iter()).any(|warning| matches!(
            warning,
            ArchiveWarning::BackendAdvisory { message, .. }
                if message.to_lowercase().contains("pax")
        )),
        "expected libarchive's own pax text; got before={before:?} after={after:?}"
    );

    // Taking empties the buffer, so a caller polling in a loop does not see
    // the same advisory twice.
    assert!(
        archive.take_backend_warnings().is_empty(),
        "take must drain"
    );

    common::cleanup(&dir);
}

/// Backends with no recoverable-status channel must answer the same call
/// harmlessly rather than making the accessor libarchive-only in practice.
#[test]
fn a_backend_without_advisories_returns_an_empty_list() {
    let dir = common::temp_test_dir();
    let zip_path = dir.join("plain.zip");
    common::build_zip(
        &zip_path,
        &[common::ZipMember::File("a.txt", b"hello\n")],
        true,
    );

    let archive = Archive::open(&zip_path).expect("open zip");
    archive.list_files().expect("list zip");
    assert!(
        archive.take_backend_warnings().is_empty(),
        "the ZIP reader has no ARCHIVE_WARN equivalent, so this must be empty"
    );

    common::cleanup(&dir);
}
