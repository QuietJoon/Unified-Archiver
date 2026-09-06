//! OI-0001-002: a read handle is bound to the archive **file**, not to the
//! entry names it happened to list.
//!
//! Every read backend memoises its listing once (AD 0065) and then re-opens
//! the archive **by pathname** for each later operation. The drift guards at
//! those re-opens compare the normalised entry name and nothing else, so an
//! archive rewritten on disk between the listing and the extraction — same
//! entry names, different payloads, sizes, CRCs, entry kinds or encryption
//! status — passed every guard, while the safety-gate decisions (ratio,
//! size, entry kind) had been made against the stale cached listing.
//!
//! The fix is not a wider per-entry comparison: per-backend normalisation of
//! size/CRC/type/encryption does not agree across backends (directory sizes
//! are `None` on ZIP/7z/UnRAR but `Some(0)` on libarchive tar; CRC is `None`
//! for the whole tar/ISO/raw family and a placeholder for AE-2 ZIP), so a
//! widened comparison would fail closed on healthy archives. Instead the
//! handle is bound to the archive file's identity — `(dev, ino, len)` on
//! Unix, `len` alone off Unix — and that binding is re-checked at every
//! by-path re-open.
//!
//! What this file pins, at the public facade:
//!
//! 1. after the archive is replaced by a *different file* carrying the same
//!    entry name, every read operation is refused with
//!    `OperationBlocked` + "identity changed" — `extract_all`,
//!    `extract_file`, `extract_to_memory`, `extract_to_stream` and
//!    `validate_integrity`, on tar (libarchive), 7z and RAR;
//! 2. `len` earns its place in the identity: appending a single byte to a
//!    tar leaves the inode alone and is still refused;
//! 3. ZIP's window is closed by a different mechanism, and that is asserted
//!    rather than left as a silent hole (see the ZIP test's comment).
//!
//! The refusal vocabulary is deliberately disjoint from name/cardinality
//! drift, which stays `ArchiveError::Format` carrying "listing drift".
//! Every one of those name and cardinality guards is still in place; this
//! binding is additive, and catches only what a name comparison cannot.

mod common;

use std::path::Path;

use unified_archive::{
    Archive, ArchiveError, CompressionOptions, ExtractionOptions, StreamBound, WritableFormat,
};

/// The entry name both the bound archive and its replacement carry, so the
/// name-drift guards would pass the swap through and the identity binding
/// is provably the thing doing the refusing.
const ENTRY: &str = "payload.bin";

/// Deterministic, poorly-compressible payload bytes.
///
/// Two properties are wanted, and a run of one repeated byte gives
/// neither. Poor compressibility keeps two archives built from different
/// payload lengths at clearly different *file* lengths, which is the only
/// half of the identity that survives off Unix; it also keeps the
/// compression ratio near 1:1, so the facade's zip-bomb gate cannot fire
/// ahead of the identity check and turn a passing assertion into an
/// accident. Determinism lets the ZIP test compare the extracted bytes
/// against the payload it wrote.
fn payload(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut state: u32 = 0x9E37_79B9;
    for _ in 0..len {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        out.push((state >> 24) as u8);
    }
    out
}

/// Single-entry archive of `format` at `path`, whose only member is
/// [`ENTRY`] carrying `len` bytes.
fn write_archive(path: &Path, format: WritableFormat, len: usize) {
    let mut archive =
        Archive::create(path, CompressionOptions::for_writable(format)).expect("create archive");
    archive
        .add_file_from_data(ENTRY, &payload(len))
        .expect("add entry");
    archive.finish().expect("finish archive");
}

/// Name and declared size of the archive's single file entry, taken from
/// the AD 0065 snapshot every later operation reuses.
fn only_file_entry(archive: &Archive) -> (String, Option<u64>) {
    let entries = archive.list_files().expect("list");
    let file = entries
        .iter()
        .find(|e| e.is_file())
        .expect("archive has a file entry");
    (file.path.clone(), file.size)
}

/// Install `replacement` at `target` as a genuinely different file, and
/// assert the precondition that makes the swap detectable on every
/// platform: a different length, so the check binds off Unix too, where
/// there is no inode half to the identity.
fn rename_over(target: &Path, replacement: &Path) {
    let bound_len = std::fs::metadata(target)
        .expect("stat the bound archive")
        .len();
    let replacement_len = std::fs::metadata(replacement)
        .expect("stat the replacement archive")
        .len();
    assert_ne!(
        bound_len, replacement_len,
        "the two archives must differ in length, or this test proves nothing off Unix"
    );
    std::fs::rename(replacement, target).expect("rename the replacement over the bound archive");
}

/// `result` must be the identity refusal, in its own vocabulary.
///
/// Generic over the success type because `StreamingExtractor` is not
/// `Debug`, so the `Ok` arm can never name its value.
///
/// The operation label is asserted non-empty but not pinned to a specific
/// string: the label a backend threads through is the caller's op
/// (`extract`, `extract_file`, `validate_integrity`, ...) and differs by
/// backend and by altitude. What the public contract promises here is the
/// variant and the phrasing.
#[track_caller]
fn assert_identity_refusal<T>(label: &str, result: Result<T, ArchiveError>) {
    match result {
        Ok(_) => panic!(
            "{label}: the archive on disk is no longer the file this handle was bound to, so \
             the operation must be refused rather than run against bytes the safety gate \
             never saw"
        ),
        Err(ArchiveError::OperationBlocked { operation, reason }) => {
            assert!(
                reason.contains("identity changed"),
                "{label}: identity drift has its own vocabulary — name/cardinality drift is \
                 `Format` + \"listing drift\", this is `OperationBlocked` + \"identity \
                 changed\". Got: {reason}"
            );
            assert!(
                !operation.is_empty(),
                "{label}: the refusal must name the operation it blocked"
            );
        }
        Err(other) => panic!("{label}: expected OperationBlocked, got: {other}"),
    }
}

/// The whole read surface, against a handle whose archive has been
/// replaced. Every one of these re-opens the archive by pathname, so every
/// one of them must re-check the binding.
#[cfg_attr(not(feature = "libarchive"), allow(dead_code))]
fn assert_every_read_operation_is_refused(
    archive: &Archive,
    entry: &str,
    dest: &Path,
    backend: &str,
) {
    assert_identity_refusal(
        &format!("{backend}/extract_all"),
        archive.extract_all(ExtractionOptions::new(dest)),
    );
    assert_identity_refusal(
        &format!("{backend}/extract_file"),
        archive.extract_file(entry, ExtractionOptions::new(dest)),
    );
    assert_identity_refusal(
        &format!("{backend}/extract_to_memory"),
        archive.extract_to_memory(entry),
    );
    assert_identity_refusal(
        &format!("{backend}/extract_to_stream"),
        archive.extract_to_stream(entry, StreamBound::DeclaredSize),
    );
    assert_identity_refusal(
        &format!("{backend}/validate_integrity"),
        archive.validate_integrity(),
    );
}

/// libarchive (TAR): the backend with the widest by-path re-open surface —
/// it cannot rewind its read handle, so it re-opens the file once per
/// operation. Every one of those re-opens is a place the cached listing
/// could start describing a different file.
#[cfg(feature = "libarchive")]
#[test]
fn tar_read_surface_is_refused_after_a_same_name_archive_is_swapped_in() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    let replacement = temp.join("replacement.tar");
    let dest = temp.join("out");

    write_archive(&target, WritableFormat::TAR, 4096);
    write_archive(&replacement, WritableFormat::TAR, 512);

    let archive = Archive::open(&target).expect("open tar");
    let (entry, declared) = only_file_entry(&archive);
    assert_eq!(declared, Some(4096), "TAR declares its entry size");
    assert_eq!(entry, ENTRY);

    rename_over(&target, &replacement);

    assert_every_read_operation_is_refused(&archive, &entry, &dest, "tar");

    common::cleanup(&temp);
}

/// 7z: `open_reader` is re-run per operation, so the listing snapshot and
/// every later extraction can straddle a replacement exactly as the tar
/// case does.
///
/// The two payload lengths are far apart (64 KiB vs 1 KiB) and the payload
/// barely compresses, so the resulting archives cannot coincidentally share
/// a length — which is the only half of the identity that survives off
/// Unix.
#[cfg(all(feature = "sevenzip", feature = "libarchive"))]
#[test]
fn sevenz_read_surface_is_refused_after_a_same_name_archive_is_swapped_in() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.7z");
    let replacement = temp.join("replacement.7z");
    let dest = temp.join("out");

    write_archive(&target, WritableFormat::SEVEN_ZIP, 64 * 1024);
    write_archive(&replacement, WritableFormat::SEVEN_ZIP, 1024);

    let archive = Archive::open(&target).expect("open 7z");
    let (entry, declared) = only_file_entry(&archive);
    assert_eq!(declared, Some(64 * 1024), "7z declares its entry size");
    assert_eq!(entry, ENTRY);

    rename_over(&target, &replacement);

    assert_every_read_operation_is_refused(&archive, &entry, &dest, "7z");

    common::cleanup(&temp);
}

/// UnRAR: `fresh_handle` re-opens the archive by pathname for the listing
/// walk, both extract paths and the integrity walk.
///
/// The fixtures are copied out of `tests/fixtures/` and swapped **in the
/// temp directory** — the repository's fixtures are never renamed, and no
/// RAR is created here. RAR fixtures come from the standalone
/// `scripts/generate-rar-fixtures.sh`; this test only reads them.
///
/// This pair is the sharpest case in the file. `test.rar` and
/// `test_recovery.rar` hold the *same* member — `test_file.txt`, 18 bytes,
/// CRC32 `0x054607BC` — and differ only in that the second carries a 5%
/// recovery record, so it is a different file (97 vs 295 bytes). A
/// per-entry comparison of name, size, CRC, type and encryption status
/// would find nothing to object to; only the file binding sees the swap.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn rar_read_surface_is_refused_after_a_fixture_is_swapped_in() {
    let temp = common::temp_test_dir();
    let target = temp.join("bound.rar");
    let replacement = temp.join("replacement.rar");
    let dest = temp.join("out");

    // test.rar and test_rar5.rar are byte-for-byte identical, so the pair
    // must be test.rar / test_recovery.rar for the length half of the
    // identity to bind off Unix.
    std::fs::copy(common::fixture("test.rar"), &target).expect("stage the bound RAR fixture");
    std::fs::copy(common::fixture("test_recovery.rar"), &replacement)
        .expect("stage the replacement RAR fixture");

    let archive = Archive::open(&target).expect("open rar");
    let (entry, declared) = only_file_entry(&archive);
    assert_eq!(entry, "test_file.txt");
    assert_eq!(
        declared,
        Some(18),
        "both fixtures carry the same 18-byte member; the swap is invisible per-entry"
    );

    rename_over(&target, &replacement);

    assert_every_read_operation_is_refused(&archive, &entry, &dest, "rar");

    common::cleanup(&temp);
}

/// Why `len` is part of the identity, stated as a test.
///
/// Appending to a tar keeps `(dev, ino)` — the file is the same file — but
/// adds entries the safety gate never saw. The bulk-walk cardinality guard
/// would catch extra *live* entries, but a single-entry seek stops at its
/// target and would not, so `(dev, ino)` alone would bless the archive.
/// A one-byte append is the minimal version of that: nothing about the
/// inode moves, and the operation is still refused.
///
/// `#[cfg(unix)]` because the assertion that makes this test mean anything
/// — "the inode did not move" — is only expressible on Unix. Off Unix the
/// identity is the length alone and an append is caught trivially.
#[cfg(unix)]
#[cfg(feature = "libarchive")]
#[test]
fn appending_one_byte_is_refused_even_though_the_inode_is_unchanged() {
    use std::io::Write as _;
    use std::os::unix::fs::MetadataExt as _;

    let temp = common::temp_test_dir();
    let target = temp.join("payload.tar");
    let dest = temp.join("out");

    write_archive(&target, WritableFormat::TAR, 4096);

    let archive = Archive::open(&target).expect("open tar");
    let (entry, _) = only_file_entry(&archive);

    let before = std::fs::metadata(&target).expect("stat before the append");
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&target)
            .expect("open the tar for append");
        file.write_all(&[0u8]).expect("append one byte");
        file.sync_all().expect("flush the append");
    }
    let after = std::fs::metadata(&target).expect("stat after the append");

    assert_eq!(
        (after.dev(), after.ino()),
        (before.dev(), before.ino()),
        "an append must leave the inode alone, or this test would be proving the inode check \
         rather than the length check"
    );
    assert_eq!(
        after.len(),
        before.len() + 1,
        "the append must move the length"
    );

    assert_every_read_operation_is_refused(&archive, &entry, &dest, "tar/append");

    common::cleanup(&temp);
}

/// ZIP refuses too — but from the facade, not from the backend, and the
/// difference is the whole reason this test exists.
///
/// ZIP reads through **one cached descriptor** (D4 / R0068-0033, widened by
/// OI-0001-003): the first operation opens the file, and every later
/// listing, extraction and integrity call reuses that same open descriptor
/// rather than re-resolving the pathname. So there is no by-path re-open for
/// the backend's own binding to guard — its identity check fires only on
/// cache (re)population, which this sequence never reaches. Left there, ZIP
/// would have been the one backend where a post-listing swap could not be
/// refused at all.
///
/// That mattered more than it first looked, because the facade re-resolves
/// the pathname once on its own account: the compression-ratio gate stats
/// the file to get its denominator while the numerator comes from the
/// cached listing. A *larger* replacement therefore talked the zip-bomb
/// gate **down** — and on ZIP nothing downstream could object, precisely
/// because the backend never re-opens. That gate now revalidates the
/// handle's binding first, which is what this test pins.
///
/// Non-vacuity: delete the revalidation in `Archive::payload_size_for_ratio`
/// and this test fails by *succeeding* — `extract_to_memory` returns the
/// bound archive's 4096-byte payload through the cached descriptor, with no
/// refusal anywhere, which is exactly the state the bypass lived in.
///
/// `#[cfg(unix)]` because the construction itself is Unix-shaped: Windows
/// refuses to rename over a file that is open without `FILE_SHARE_DELETE`,
/// and ZIP is holding exactly such a descriptor here.
#[cfg(unix)]
#[test]
fn zip_is_refused_at_the_facade_even_though_its_backend_never_reopens() {
    let temp = common::temp_test_dir();
    let target = temp.join("payload.zip");
    let replacement = temp.join("replacement.zip");

    write_archive(&target, WritableFormat::ZIP, 4096);
    write_archive(&replacement, WritableFormat::ZIP, 512);

    let archive = Archive::open(&target).expect("open zip");
    // This is what warms the cached descriptor.
    let (entry, declared) = only_file_entry(&archive);
    assert_eq!(declared, Some(4096), "ZIP declares its entry size");

    rename_over(&target, &replacement);

    // The facade's ratio gate revalidates the binding before dispatch, so ZIP
    // is refused even though its backend would happily have served the swap
    // from the descriptor it already holds.
    assert_identity_refusal("zip/facade-ratio-gate", archive.extract_to_memory(&entry));

    common::cleanup(&temp);
}
