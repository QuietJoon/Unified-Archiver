//! `StreamBound` contract tests (R0001-0011 / OI-0001-001 / DEF-004).
//!
//! Two properties are pinned here:
//!
//! 1. `StreamBound::DeclaredSize` is an *exact-length* contract when the
//!    preflight listing declares a size — over-production is
//!    `InvalidData` (DCR-006) and under-production is `UnexpectedEof`
//!    instead of a short read that reads as success.
//! 2. The backend materialization budget is derived from the bound —
//!    `min(bound, max_file_size, max_total_size)` — so a caller-chosen
//!    `Cap(n)` binds *before* a staging backend buffers the entry, and
//!    the caller's limits stay a ceiling the bound can only tighten.

mod common;

use std::io::Read;
use unified_archive::{
    Archive, Cap, CompressionOptions, ExtractionLimits, ExtractionOptions, StreamBound,
    WritableFormat,
};

/// Build a single-entry TAR (libarchive-backed: the only genuinely
/// incremental stream path) whose payload is `len` bytes.
fn write_tar(path: &std::path::Path, entry: &str, len: usize) {
    let mut archive = Archive::create(path, CompressionOptions::for_writable(WritableFormat::TAR))
        .expect("create tar");
    archive
        .add_file_from_data(entry, &vec![b'A'; len])
        .expect("add entry");
    archive.finish().expect("finish tar");
}

fn only_entry(archive: &Archive) -> (String, Option<u64>) {
    let entries = archive.list_files().expect("list");
    let file = entries
        .iter()
        .find(|e| e.is_file())
        .expect("archive has a file entry");
    (file.path.clone(), file.size)
}

fn limits(max_file: Cap, max_total: Cap) -> ExtractionLimits {
    ExtractionLimits::builder()
        .max_file_size(max_file)
        .max_total_size(max_total)
        .build()
}

/// R0001-0011 / OI-0001-001, the case the ticket names: an entry whose
/// authoritative listing declares more bytes than the live stream
/// delivers must not read as a short-but-valid payload.
///
/// The archive is a plain TAR — no per-entry checksum — and the shortfall
/// is produced the way it happens in practice: the AD 0065 listing
/// snapshot is taken, then the file on disk is replaced by one carrying
/// the same entry name with a shorter payload. The name-only drift guards
/// (OI-0001-002) pass it through, so the stream itself is the last line of
/// defence. Before this change `read_to_end` returned `Ok(512)`.
#[test]
fn declared_size_truncated_entry_surfaces_unexpected_eof() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    let short = temp.join("short.tar");

    write_tar(&target, "payload.bin", 4096);
    write_tar(&short, "payload.bin", 512);

    let archive = Archive::open(&target).expect("open tar");
    // Take the snapshot the extraction path will reuse.
    let (entry, declared) = only_entry(&archive);
    assert_eq!(declared, Some(4096), "TAR declares its entry size");

    std::fs::copy(&short, &target).expect("swap in the shorter archive");

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("stream opens against the shortened archive");

    let mut buf = Vec::new();
    let err = stream
        .read_to_end(&mut buf)
        .expect_err("a 512-byte payload cannot satisfy a 4096-byte declaration");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::UnexpectedEof,
        "truncation must be distinguishable from over-production ({err})"
    );

    // Sticky: retrying must not fall through to a clean EOF.
    let again = stream
        .read(&mut [0u8; 16])
        .expect_err("the verdict must repeat");
    assert_eq!(again.kind(), std::io::ErrorKind::UnexpectedEof);

    common::cleanup(&temp);
}

/// The same shortfall under a ceiling-only bound is *not* an error:
/// `Cap(n)` is a budget, not an assertion about the entry's size
/// (unchanged contract, pinned so the exactness work cannot leak into it).
#[test]
fn cap_bound_tolerates_a_short_entry() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    let short = temp.join("short.tar");

    write_tar(&target, "payload.bin", 4096);
    write_tar(&short, "payload.bin", 512);

    let archive = Archive::open(&target).expect("open tar");
    let (entry, _) = only_entry(&archive);
    std::fs::copy(&short, &target).expect("swap in the shorter archive");

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::Cap(4096))
        .expect("stream opens");
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .expect("ceiling-only bound accepts a short entry");
    assert_eq!(buf.len(), 512);

    common::cleanup(&temp);
}

/// Regression guard for the exactness path: a well-formed entry on every
/// backend still reads to exactly its declared size and then EOFs.
#[test]
fn declared_size_healthy_entries_reach_clean_eof() {
    for fixture in ["test.zip", "test.7z", "test.tar", "test.tar.gz"] {
        let archive = Archive::open(common::fixture(fixture)).expect("open fixture");
        let (entry, declared) = only_entry(&archive);
        let declared = declared.expect("these formats declare entry sizes");

        let mut stream = archive
            .extract_to_stream(&entry, StreamBound::DeclaredSize)
            .expect("stream opens");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).expect("read_to_end");
        assert_eq!(buf.len() as u64, declared, "{fixture}: exact delivery");
        assert_eq!(
            stream.read(&mut [0u8; 8]).expect("post-EOF read"),
            0,
            "{fixture}: clean EOF after the declaration"
        );
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn declared_size_healthy_rar_entry_reaches_clean_eof() {
    let archive = Archive::open(common::fixture("test.rar")).expect("open rar");
    let (entry, declared) = only_entry(&archive);
    let declared = declared.expect("RAR declares entry sizes");

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("stream opens");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(buf.len() as u64, declared);

    // Ceiling-only regression on the same handle: a generous `Cap` must not
    // turn into an assertion about the entry's size.
    let mut generous = archive
        .extract_to_stream(&entry, StreamBound::Cap(1 << 20))
        .expect("generous cap");
    let mut buf = Vec::new();
    generous.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(buf.len() as u64, declared);
    assert_eq!(generous.read(&mut [0u8; 4]).expect("clean EOF"), 0);
}

/// DEF-004 first step: a `Cap(n)` below the declared size now reaches the
/// backend as the materialization budget, so a backend that must buffer
/// the entry before returning a reader refuses it at call time instead of
/// materializing all of it and handing back a reader capped at `n`.
#[test]
fn cap_below_declared_size_is_refused_by_staging_backends() {
    for fixture in ["test.zip", "test.7z"] {
        let archive = Archive::open(common::fixture(fixture)).expect("open fixture");
        let (entry, declared) = only_entry(&archive);
        let declared = declared.expect("declared size");
        assert!(declared > 4, "{fixture}: fixture must exceed the budget");

        // `StreamingExtractor` is not `Debug`, so match rather than
        // `expect_err`.
        let text = match archive.extract_to_stream(&entry, StreamBound::Cap(4)) {
            Ok(_) => panic!("{fixture}: a staging backend cannot serve a prefix of a larger entry"),
            Err(e) => e.to_string(),
        };
        // Match the budget in its full phrase, not the bare digit: a lone '4'
        // also appears in any declared size containing it ("14", "4096"), so
        // `contains('4')` could not fail for the reason it claims.
        assert!(
            text.contains("limit of 4 bytes"),
            "{fixture}: the rejection should name the 4-byte budget, got: {text}"
        );
    }
}

/// R2 (DCR-006 Amendment 4): UnRAR's `Cap(n)` trigger is now the same as
/// ZIP's and 7z's — the *declared* size, judged before any decode. It used
/// to abort only once decoded bytes crossed the budget inside
/// `UCM_PROCESSDATA`, which made the refusal both later and conditional on
/// the payload rather than on the header.
///
/// The assertions pin all three properties the unification claims: the
/// error is `OperationBlocked`, it is labelled with the *stream* operation
/// (R5 / ti-581bcda4), and it names the declaration and the budget in the
/// shared `read_entry_to_memory_capped` phrasing.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn cap_below_declared_size_is_refused_by_rar() {
    use unified_archive::ArchiveError;

    let archive = Archive::open(common::fixture("test.rar")).expect("open rar");
    let (entry, declared) = only_entry(&archive);
    assert!(
        declared.expect("RAR declares entry sizes") > 4,
        "fixture must exceed the budget"
    );

    match archive.extract_to_stream(&entry, StreamBound::Cap(4)) {
        Ok(_) => panic!("UnRAR stages the entry, so the budget must bind before the SDK runs"),
        Err(ArchiveError::OperationBlocked { operation, reason }) => {
            assert_eq!(
                operation, "extract_to_stream",
                "the label must name the public operation the caller invoked, got: {operation}"
            );
            assert!(
                reason.contains("declares"),
                "the refusal must be triggered by the declaration, got: {reason}"
            );
            // Full phrase, not the bare digit: a lone '4' also occurs inside
            // any declared size containing it.
            assert!(
                reason.contains("limit of 4 bytes"),
                "the refusal should name the 4-byte budget, got: {reason}"
            );
        }
        Err(other) => panic!("expected OperationBlocked, got: {other}"),
    }
}

/// R5 (ti-581bcda4): the operation label matrix. The same refusal reached
/// through `extract_to_stream` and through the memory path must name the
/// operation the caller actually invoked — the capped memory helper the
/// stream path reuses internally is an implementation detail, not the
/// caller's operation (R0071-0010). Before this the stream path reported
/// `extract_to_memory` on all three staging backends.
#[test]
fn cap_refusal_label_names_the_caller_operation_on_staging_backends() {
    for fixture in ["test.zip", "test.7z"] {
        let archive = Archive::open(common::fixture(fixture)).expect("open fixture");
        let (entry, declared) = only_entry(&archive);
        assert!(
            declared.expect("declared size") > 4,
            "{fixture}: fixture must exceed the budget"
        );
        assert_stream_and_memory_labels(&archive, &entry, fixture);
    }
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn cap_refusal_label_names_the_caller_operation_on_rar() {
    let archive = Archive::open(common::fixture("test.rar")).expect("open rar");
    let (entry, declared) = only_entry(&archive);
    // The zip/7z arm of this matrix carries the same guard: if the fixture
    // ever shrank to 4 bytes or fewer the Cap(4) refusal would stop firing and
    // the label assertion would pass by never being reached.
    assert!(
        declared.expect("RAR declares entry sizes") > 4,
        "test.rar must exceed the 4-byte budget for the refusal to fire"
    );
    assert_stream_and_memory_labels(&archive, &entry, "test.rar");
}

/// Shared body of the R5 label matrix: the stream entry point must report
/// `extract_to_stream`, the memory entry point `extract_to_memory`, for the
/// same over-budget entry on the same handle.
fn assert_stream_and_memory_labels(archive: &Archive, entry: &str, fixture: &str) {
    use unified_archive::ArchiveError;

    match archive.extract_to_stream(entry, StreamBound::Cap(4)) {
        Ok(_) => panic!("{fixture}: a staging backend cannot serve a prefix of a larger entry"),
        Err(ArchiveError::OperationBlocked { operation, .. }) => assert_eq!(
            operation, "extract_to_stream",
            "{fixture}: stream refusals must be labelled extract_to_stream"
        ),
        Err(other) => panic!("{fixture}: expected OperationBlocked from the stream path: {other}"),
    }

    // The memory arm proves the public contract "a memory refusal names
    // extract_to_memory". On this path the facade's metadata gate is what
    // refuses (the same `limits` feed both the gate and the backend budget,
    // and every staging format declares its entry size), so the *backend*
    // label is the one pinned by the stream arm above — which is exactly the
    // call site R5 was mislabelling.
    let options = ExtractionOptions::default().limits(limits(Cap::Limited(4), Cap::Unlimited));
    match archive.extract_to_memory_with_options(entry, &options) {
        Ok(_) => panic!("{fixture}: the memory path must refuse the over-budget entry too"),
        Err(ArchiveError::OperationBlocked { operation, .. }) => assert_eq!(
            operation, "extract_to_memory",
            "{fixture}: memory refusals keep the extract_to_memory label"
        ),
        Err(other) => panic!("{fixture}: expected OperationBlocked from the memory path: {other}"),
    }
}

/// R1 (DCR-006 Amendment 4): the RAR staged payload is now length-checked
/// against the *listing's* declaration, not only against the staged file's
/// own metadata. This guards the hardening against false positives — a
/// healthy RAR entry must still stream to a clean EOF under every bound,
/// and must still extract to memory unchanged.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn rar_staged_length_check_accepts_a_healthy_entry() {
    let archive = Archive::open(common::fixture("test.rar")).expect("open rar");
    let (entry, declared) = only_entry(&archive);
    let declared = declared.expect("RAR declares entry sizes");

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("healthy RAR entry must not trip the staged-length check");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(buf.len() as u64, declared);
    assert_eq!(stream.read(&mut [0u8; 8]).expect("clean EOF"), 0);

    let bytes = archive
        .extract_to_memory(&entry)
        .expect("the memory path is unaffected");
    assert_eq!(bytes.len() as u64, declared);
    assert_eq!(bytes, buf, "both paths deliver the same payload");
}

/// The incremental backend keeps the prefix read a ceiling-only budget
/// implies: libarchive hands back a reader, the caller gets `n` bytes,
/// and only reading *past* `n` is an error.
#[test]
fn cap_below_declared_size_serves_a_prefix_on_libarchive() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    write_tar(&target, "payload.bin", 4096);

    let archive = Archive::open(&target).expect("open tar");
    let (entry, _) = only_entry(&archive);

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::Cap(16))
        .expect("the incremental backend serves a prefix");

    let mut prefix = [0u8; 16];
    let mut filled = 0usize;
    while filled < prefix.len() {
        let n = stream.read(&mut prefix[filled..]).expect("prefix read");
        assert_ne!(n, 0, "prefix must be available");
        filled += n;
    }
    assert!(prefix.iter().all(|b| *b == b'A'));

    let err = stream
        .read(&mut [0u8; 8])
        .expect_err("reading past the cap is a violation, not an EOF");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

    common::cleanup(&temp);
}

/// Constraint check for `Cap`: a generous ceiling must not turn into an
/// assertion — a smaller entry streams to its natural EOF on every
/// backend.
#[test]
fn cap_above_entry_size_reads_the_whole_entry() {
    for fixture in ["test.zip", "test.7z", "test.tar"] {
        let archive = Archive::open(common::fixture(fixture)).expect("open fixture");
        let (entry, declared) = only_entry(&archive);
        let declared = declared.expect("declared size");

        let mut stream = archive
            .extract_to_stream(&entry, StreamBound::Cap(1 << 20))
            .expect("stream opens");
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).expect("read_to_end");
        assert_eq!(buf.len() as u64, declared, "{fixture}");
        assert_eq!(
            stream.read(&mut [0u8; 4]).expect("clean EOF"),
            0,
            "{fixture}"
        );
    }
}

/// Hard constraint: an entry with no declared size gets no invented
/// declaration. A raw gzip stream (libarchive's single-file reader leaves
/// the size field unset) reads to its natural EOF under `DeclaredSize`.
#[test]
fn declared_size_degrades_for_unknown_size_entries() {
    let archive = Archive::open(common::fixture("test.gz")).expect("open gz");
    let (entry, declared) = only_entry(&archive);
    assert_eq!(
        declared, None,
        "the raw gzip reader declares no uncompressed size"
    );

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("stream opens");
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .expect("an unknown-size entry EOFs cleanly");
    assert!(!buf.is_empty());
}

/// R0001-0011: the stream path no longer ignores a tighter
/// `max_total_size`. With `max_file_size` unlimited and `max_total_size`
/// at 4 bytes the backend budget is 4, so an unknown-size entry that
/// decodes past it fails — previously the backend received
/// `max_file_size` alone (unlimited) and produced the whole payload.
#[test]
fn max_total_size_participates_in_the_backend_budget() {
    let archive = Archive::open(common::fixture("test.gz")).expect("open gz");
    let (entry, _) = only_entry(&archive);

    let options = ExtractionOptions::default().limits(limits(Cap::Unlimited, Cap::Limited(4)));
    let mut stream = archive
        .extract_to_stream_with_options(&entry, &options, StreamBound::Unbounded)
        .expect("unknown-size entry passes the metadata gate");

    let mut buf = Vec::new();
    let err = stream
        .read_to_end(&mut buf)
        .expect_err("the effective entry ceiling must bind");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(buf.len() <= 4, "at most the ceiling is delivered");
}

/// The documented way to read a window of a larger entry: an exact
/// `DeclaredSize` stream wrapped in `Read::take`. The inner stream never
/// observes EOF, so the exactness check does not fire.
#[test]
fn declared_size_window_via_take_is_not_a_truncation() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    write_tar(&target, "payload.bin", 4096);

    let archive = Archive::open(&target).expect("open tar");
    let (entry, _) = only_entry(&archive);
    let stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("stream opens");

    let mut window = stream.take(64);
    let mut buf = Vec::new();
    window.read_to_end(&mut buf).expect("window read");
    assert_eq!(buf.len(), 64);

    common::cleanup(&temp);
}

/// `DeclaredSize` normalizes `total_size` to the listing's declaration
/// (R0001-0008 direction), so `progress()` reaches exactly 1.0 when — and
/// only when — the entry completes. On the staging backends this used to
/// report the materialized buffer length instead.
#[test]
fn declared_size_reports_the_listing_size_as_total() {
    let archive = Archive::open(common::fixture("test.zip")).expect("open zip");
    let (entry, declared) = only_entry(&archive);

    let mut stream = archive
        .extract_to_stream(&entry, StreamBound::DeclaredSize)
        .expect("stream opens");
    assert_eq!(stream.total_size(), declared);

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(stream.progress(), Some(1.0));
}
