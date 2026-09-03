//! End-to-end behaviour of a real multi-volume RAR set.
//!
//! Two tickets meet here, and neither could be closed by argument alone —
//! both needed a genuine volume set, which this crate cannot create (RAR
//! creation exists only behind
//! `cfg(all(target_os = "windows", feature = "external-rar-create"))`). The
//! fixtures come from `scripts/generate-rar-fixtures.sh` (`rar a -m0 -v20k`),
//! run by hand and committed — never from the build.
//!
//! * ticgit d3cfce fixed the UnRAR volume-change callback: `RAR_VOL_ASK` used
//!   to fall through to the default return, which the SDK reads as "retry", so
//!   a set with a missing volume retried the same absent path forever while
//!   holding the process-wide UnRAR lock. Five direct callback tests pin the
//!   trampoline (`src/ffi/wrapper/tests.rs`); this file pins what a caller
//!   observes.
//! * ticgit 8c29f8 tracked the absent fixture that blocked the above.
//!
//! What the fixture then revealed is the larger finding: a complete, valid,
//! `unrar`-verified volume set cannot be extracted by **any** public path.
//! These tests state that precisely rather than leaving it to be discovered
//! again, and they are the regression surface for ticgit 3b4d15.

#![cfg(feature = "rar-support")]

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::{Path, PathBuf};

use unified_archive::format::multipart::parse_volume_set;
use unified_archive::{Archive, ArchiveFormat, Support};

const PARTS: [&str; 3] = [
    "test_multivol.part1.rar",
    "test_multivol.part2.rar",
    "test_multivol.part3.rar",
];

/// Copy the named volumes into a fresh directory and return its path plus the
/// path of the first volume, which is where a reader must start.
fn stage_volumes(names: &[&str]) -> (PathBuf, PathBuf) {
    let dir = common::temp_test_dir();
    for name in names {
        fs::copy(common::fixture(name), dir.join(name))
            .unwrap_or_else(|e| panic!("staging {name}: {e}"));
    }
    (dir.clone(), dir.join(PARTS[0]))
}

fn subdir(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(&path).expect("create destination");
    path
}

/// A file split across three volumes is listed as **three** entries, one per
/// volume: same `path`, same declared `size`, different `compressed_size` and
/// different `crc32` (each volume's own fragment). That is UnRAR surfacing the
/// per-volume file header rather than one logical entry, and it is the root of
/// every extraction failure below — the duplicate path is what the extraction
/// A complete set lists as **one logical entry**, not one per volume.
///
/// UnRAR surfaces a per-volume file header for a split file: three headers
/// sharing one name, each declaring the whole file's unpacked size and
/// carrying its own fragment checksum. Left in that shape they read as three
/// distinct files at one path, which is what closed every extraction route
/// and made the content total count the file three times.
///
/// `walk_entries` now folds continuation headers (`RHDF_SPLITBEFORE`) into
/// their predecessor, which is the information UnRAR was already handing us
/// and the crate was discarding.
#[test]
#[serial_test::file_serial(rar)]
fn complete_volume_set_lists_one_logical_entry() {
    let (dir, first) = stage_volumes(&PARTS);

    let archive = Archive::open(&first).expect("open the first volume of a complete set");
    let entries = archive.list_files().expect("list a complete volume set");

    assert_eq!(
        entries.len(),
        1,
        "three volume headers describe one file: {entries:?}"
    );
    let entry = &entries[0];
    assert_eq!(entry.path, "volume_payload.bin");

    let declared = entry.size.expect("the entry declares a size");
    assert!(
        declared > 20_480,
        "the payload must exceed one 20k volume or the set would not be split: {declared}"
    );
    assert_eq!(
        entry.crc32, None,
        "each volume carries a checksum over its own fragment, not over the \
         file; surfacing one of them would verify nothing: {entry:?}"
    );

    // The total that used to be three times the truth, and marked exact.
    let (_digest, total) = archive
        .calculate_content_multiset_digest_and_size()
        .expect("digest a complete set");
    assert_eq!(total.sized_bytes(), declared);
    assert_eq!(total.sized_entries(), 1);

    common::cleanup(&dir);
}

/// Every public read route reaches the payload, including the two that used
/// to refuse by recommending a call which also failed.
///
/// Asserted as one test because the value is the *set* of outcomes: the
/// defect was that no route worked, so the fix is that every route does.
/// Extraction across volumes needed no volume-change callback — UnRAR opens
/// the continuation volumes itself once it is asked for one logical entry.
#[test]
#[serial_test::file_serial(rar)]
fn complete_volume_set_extracts_through_every_public_route() {
    let (dir, first) = stage_volumes(&PARTS);
    let archive = Archive::open(&first).expect("open the first volume of a complete set");
    let entry_path = "volume_payload.bin";
    let size = archive.list_files().expect("list")[0]
        .size
        .expect("declared size");

    let all_dest = subdir(&dir, "all");
    archive
        .extract_all(common::default_extraction_options(all_dest.clone()))
        .expect("extract_all used to trip the duplicate-output-path guard");
    assert_eq!(
        fs::metadata(all_dest.join(entry_path)).expect("stat").len(),
        size,
        "a short file here means the continuation volumes were not followed"
    );

    let file_dest = subdir(&dir, "file");
    archive
        .extract_file(
            entry_path,
            common::default_extraction_options(file_dest.clone()),
        )
        .expect("extract_file used to refuse to disambiguate");
    assert_eq!(
        fs::metadata(file_dest.join(entry_path))
            .expect("stat")
            .len(),
        size
    );

    let in_memory = archive
        .extract_to_memory(entry_path)
        .expect("extract_to_memory used to refuse to disambiguate");
    assert_eq!(in_memory.len() as u64, size);

    let ids_dest = subdir(&dir, "ids");
    archive
        .extract_by_ids(&[0], common::default_extraction_options(ids_dest.clone()))
        .expect("extract_by_ids used to fail with a listing-drift error");
    assert_eq!(
        fs::metadata(ids_dest.join(entry_path)).expect("stat").len(),
        size
    );

    let report = archive
        .validate_integrity()
        .expect("validate a complete set");
    assert_eq!(
        (report.total_entries, report.validated, report.failed.len()),
        (1, 1, 0),
        "it used to report three healthy entries for an archive nothing could read"
    );

    common::cleanup(&dir);
}

/// The capability report must match what the tests above demonstrate.
///
/// This was `Support::Partial` while no extraction path could reassemble a
/// set. The tests above now extract one end to end through four routes, so
/// `Partial` understates the crate and this assertion moved with the
/// behaviour rather than being deleted.
#[test]
fn rar_multipart_read_is_full() {
    for format in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
        assert_eq!(
            format.capabilities().multipart_read,
            Support::Full,
            "{format:?}: a complete volume set lists as one entry and extracts \
             through every public route, so this is no longer Partial"
        );
    }
}

/// The regression ticgit d3cfce exists for: a set with its middle volume absent
/// must fail *bounded*, and say something useful. Reaching the end of this test
/// at all is the first assertion — the pre-fix behaviour was to retry the
/// absent volume name forever while holding the process-wide UnRAR lock.
///
/// Getting the diagnostic out took a second fix. The vendored SDK's
/// `DllVolChange` (`volume.cpp`) ends with:
///
/// ```text
/// if (DllVolAborted || Cmd->Callback==NULL && Cmd->ChangeVolProc==NULL)
/// {
///   Cmd->DllError=ERAR_EOPEN;
///   return false;
/// }
/// ```
///
/// so "our callback returned -1" and "no callback was installed" are
/// indistinguishable from outside, and in the second case the callback is never
/// invoked at all — verified by instrumenting the trampoline, which logged
/// nothing for this scenario. The extract path installs its callback
/// immediately before `RARProcessFile` and cleared it after, so the *listing*
/// walk that runs first, and its `RAR_SKIP` past a split entry, met the SDK's
/// own no-callback branch. Boundedness came from that branch — which exists in
/// its author's words "to prevent an infinite loop if no callback is defined" —
/// and not from d3cfce's abort at all.
///
/// ticgit 03ddc6 moved the callback to the handle's lifetime: it is registered
/// through `RAROpenArchiveDataEx` before the main header is read and restored
/// after any operation that installs its own, so every SDK call on the handle
/// can answer a volume request. The typed `MissingVolume` diagnostic now
/// reaches the caller, which is what this asserts.
#[test]
#[serial_test::file_serial(rar)]
fn volume_set_missing_its_middle_volume_fails_bounded() {
    let (dir, first) = stage_volumes(&[PARTS[0], PARTS[2]]);

    let archive = Archive::open(&first).expect("the first volume alone still opens");

    let err = archive
        .extract_all(common::default_extraction_options(subdir(&dir, "out")))
        .expect_err("a set missing its middle volume must not extract successfully");
    let text = err.to_string();

    assert!(
        text.contains(PARTS[0]),
        "the error must name the archive the caller asked about; got: {text}"
    );
    // The discriminator against the complete-set failure above. A complete set
    // fails at our duplicate-output-path guard; a hole in the set must fail as
    // a missing volume, and must say which call answers "which volume".
    // Without this the test would pass for the same reason a complete set
    // fails.
    assert!(
        text.contains("next volume") && text.contains("VolumeSetReport"),
        "a missing volume must report itself as one and point at the call that \
         names the gap, distinctly from the duplicate-path refusal a complete \
         set produces; got: {text}"
    );
    assert!(
        !text.contains("ERAR_EOPEN"),
        "ERAR_EOPEN is what the SDK reports when no callback was installed. \
         Seeing it again means the handle-lifetime callback stopped being \
         registered for this walk; got: {text}"
    );

    common::cleanup(&dir);
}

/// The typed parser answers "which volume is missing" for these exact paths,
/// which is what the `MissingVolume` diagnostic points callers at.
#[test]
fn the_advertised_recovery_call_names_the_missing_volume() {
    let present: Vec<PathBuf> = [PARTS[0], PARTS[2]]
        .iter()
        .map(|name| common::fixture(name))
        .collect();

    let report = parse_volume_set(&present);
    assert!(
        !report.is_complete(),
        "a set missing part2 must not report complete"
    );
    let defects = report.defects();
    assert!(
        !defects.is_empty(),
        "the gap must be reported as a defect, not silently tolerated"
    );
    let described = format!("{defects:?}");
    assert!(
        described.contains('2'),
        "the defect must identify volume 2 as the gap; got: {described}"
    );
}
