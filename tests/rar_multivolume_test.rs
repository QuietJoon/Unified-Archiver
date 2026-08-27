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
/// guards reject.
#[test]
#[serial_test::file_serial(rar)]
fn complete_volume_set_lists_one_entry_per_volume() {
    let (dir, first) = stage_volumes(&PARTS);

    let archive = Archive::open(&first).expect("open the first volume of a complete set");
    let entries = archive.list_files().expect("list a complete volume set");

    assert_eq!(
        entries.len(),
        PARTS.len(),
        "one entry per volume is today's shape, not one per logical file: {entries:?}"
    );

    let declared = entries[0].size.expect("the entry declares a size");
    assert!(
        declared > 20_480,
        "the payload must exceed one 20k volume or the set would not be split: {declared}"
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry.path == entries[0].path && entry.size == Some(declared)),
        "every per-volume entry describes the same logical file: {entries:?}"
    );

    let mut checksums: Vec<_> = entries.iter().map(|entry| entry.crc32).collect();
    let before = checksums.len();
    checksums.sort_unstable();
    checksums.dedup();
    assert_eq!(
        checksums.len(),
        before,
        "each volume carries its own fragment checksum, so they must differ: {entries:?}"
    );

    common::cleanup(&dir);
}

/// Every public extraction path is closed for a volume set, including the one
/// the other errors tell the caller to use.
///
/// This is asserted as one test because the value is the *set* of outcomes: any
/// single failure could be read as "use a different call", and the point is
/// that there is no different call. If one of these starts succeeding, this
/// test fails and the capability value in `src/format.rs` should be revisited
/// in the same change.
#[test]
#[serial_test::file_serial(rar)]
fn complete_volume_set_cannot_be_extracted_by_any_path() {
    let (dir, first) = stage_volumes(&PARTS);
    let archive = Archive::open(&first).expect("open the first volume of a complete set");
    let entry_path = "volume_payload.bin";

    // `extract_all`: the duplicate-output-path guard fires, because three
    // entries name one output file.
    let err = archive
        .extract_all(common::default_extraction_options(subdir(&dir, "all")))
        .expect_err("extract_all must not silently write one volume's fragment");
    let text = err.to_string();
    assert!(
        text.contains("same output path") || text.contains("Multiple entries"),
        "expected the duplicate-output-path refusal; got: {text}"
    );

    // `extract_file` and `extract_to_memory`: both refuse to pick one of the
    // three same-named entries, and both point at `extract_by_ids`.
    for text in [
        archive
            .extract_file(
                entry_path,
                common::default_extraction_options(subdir(&dir, "file")),
            )
            .expect_err("extract_file must not disambiguate on its own")
            .to_string(),
        archive
            .extract_to_memory(entry_path)
            .expect_err("extract_to_memory must not disambiguate on its own")
            .to_string(),
    ] {
        assert!(
            text.contains("extract_by_ids"),
            "the refusal must name the call it recommends; got: {text}"
        );
    }

    // And that recommended call fails too — the listing holds three entries
    // but the archive walk ends after one, so the crate's own drift guard
    // rejects it. A caller following the advice in the two errors above
    // arrives here.
    let err = archive
        .extract_by_ids(
            &[0],
            common::default_extraction_options(subdir(&dir, "ids")),
        )
        .expect_err("extract_by_ids cannot reassemble a split entry either");
    let text = err.to_string();
    assert!(
        text.contains("listing drift"),
        "expected the listing-drift refusal from the recommended call; got: {text}"
    );

    common::cleanup(&dir);
}

/// The capability report must not promise what the tests above disprove.
#[test]
fn rar_multipart_read_is_partial_not_full() {
    for format in [ArchiveFormat::Rar, ArchiveFormat::Rar5] {
        assert_eq!(
            format.capabilities().multipart_read,
            Support::Partial,
            "{format:?}: UnRAR lists a volume set but no extraction path can \
             reassemble one, so this cannot be Full"
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
