//! Public-API tests for the typed multipart volume parser (OI-0080-004).
//!
//! The parser's own unit tests live beside it in `src/format/multipart/`. These
//! exercise it as a downstream crate does: through
//! `unified_archive::format::multipart`, with the `#[non_exhaustive]` public
//! types, and — for the RAR case that is supported end-to-end — over a volume
//! list that `Archive::detect_multipart` actually produced from disk.
//!
//! Scope note: `parse_volume_set` recognises the ZIP-split and numeric naming
//! schemes so it can tell a mixed-scheme set from a single-scheme one, but ZIP
//! `.z01` and 7z `.001` split *extraction* remains unsupported. Nothing here
//! implies otherwise.

use std::path::PathBuf;

use unified_archive::MultipartLayout;
use unified_archive::format::multipart::{
    VolumeScheme, VolumeSetDefect, VolumeSetReport, parse_volume_set,
};

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// A complete RAR split set reports complete, in volume order, and bridges to
/// the existing typed [`MultipartLayout`].
#[test]
fn complete_rar_set_bridges_to_multipart_layout() {
    let report = parse_volume_set(&paths(&[
        "backup.part3.rar",
        "backup.part1.rar",
        "backup.part2.rar",
    ]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    let set = report.set().expect("complete report carries a set");
    assert_eq!(set.scheme, VolumeScheme::RarPart);
    assert_eq!(
        set.to_layout(),
        MultipartLayout::Multi {
            parts: paths(&["backup.part1.rar", "backup.part2.rar", "backup.part3.rar"]),
        }
    );
}

/// Failure mode 1 of 4 — a missing middle volume, named in the diagnostic.
#[test]
fn missing_middle_volume_is_named() {
    let report = parse_volume_set(&paths(&["backup.part1.rar", "backup.part3.rar"]));
    let [defect] = report.defects() else {
        panic!("expected exactly one defect, got {:?}", report.defects());
    };
    assert_eq!(
        defect,
        &VolumeSetDefect::MissingVolumes {
            first: 2,
            last: 2,
            expected_first: "backup.part2.rar".to_string(),
        }
    );
    assert!(defect.to_string().contains("backup.part2.rar"));
}

/// Failure mode 2 of 4 — one volume number claimed twice.
#[test]
fn duplicate_volume_index_is_reported() {
    let report = parse_volume_set(&paths(&[
        "backup.part1.rar",
        "backup.part01.rar",
        "backup.part2.rar",
    ]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::DuplicateVolume {
            number: 1,
            paths: paths(&["backup.part01.rar", "backup.part1.rar"]),
        }],
        "a set that looks complete by count is still ambiguous in order"
    );
}

/// Failure mode 3 of 4 — a set that starts at `part2`.
#[test]
fn off_by_one_first_volume_is_reported_as_its_own_failure() {
    let report = parse_volume_set(&paths(&["backup.part2.rar", "backup.part3.rar"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::FirstVolumeMissing {
            first_present: 2,
            expected_first: "backup.part1.rar".to_string(),
        }],
        "a leading gap must not be reported as an interior hole"
    );
}

/// Two naming conventions in one candidate list.
///
/// Not a *failure* mode: cf5109 separated "a volume is missing" from "this
/// neighbouring file is not part of your set". `backup.part1` + `part2` are
/// contiguous, so the set is complete and the `.r00` is merely reported.
#[test]
fn mixed_naming_scheme_is_reported_without_making_the_set_incomplete() {
    let report = parse_volume_set(&paths(&[
        "backup.part1.rar",
        "backup.part2.rar",
        "backup.r00",
    ]));
    assert!(report.is_complete());
    assert!(report.defects().is_empty());
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::MixedScheme {
            path: PathBuf::from("backup.r00"),
            found: VolumeScheme::RarOldStyle,
            expected: VolumeScheme::RarPart,
        }]
    );
}

/// The three continuity failure modes stay distinguishable when they occur
/// together: a single "the set is broken" signal would collapse them. The
/// foreign sibling rides alongside in `unrelated()`, which cf5109 split out
/// of `defects()` precisely so it cannot be mistaken for a missing volume.
#[test]
fn the_three_continuity_failure_modes_are_distinguishable_in_one_report() {
    let report = parse_volume_set(&paths(&[
        "backup.part2.rar",
        "backup.part02.rar",
        "backup.part4.rar",
        "backup.r00",
    ]));
    let defects = report.defects();
    assert_eq!(defects.len(), 3, "defects: {defects:?}");
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, VolumeSetDefect::FirstVolumeMissing { .. })),
        "{defects:?}"
    );
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, VolumeSetDefect::MissingVolumes { first: 3, .. })),
        "{defects:?}"
    );
    assert!(
        defects
            .iter()
            .any(|d| matches!(d, VolumeSetDefect::DuplicateVolume { number: 2, .. })),
        "{defects:?}"
    );
    assert!(
        report
            .unrelated()
            .iter()
            .any(|d| matches!(d, VolumeSetDefect::MixedScheme { .. })),
        "the foreign sibling is reported, just not as a continuity defect: {:?}",
        report.unrelated()
    );
}

/// A lone archive is not a one-volume set.
#[test]
fn lone_archive_is_unvolumed() {
    let report = parse_volume_set(&paths(&["backup.rar"]));
    assert!(matches!(report, VolumeSetReport::Unvolumed { .. }));
    assert!(report.set().is_none());
    assert!(report.defects().is_empty());
}

// ── over a volume list discovered on disk ──

#[cfg(feature = "rar-support")]
mod on_disk {
    use super::*;
    use std::path::Path;
    use unified_archive::Archive;
    use unified_archive::format::multipart::parse_volume_set_for;

    /// Every volume of the set is a byte-copy of a real single-volume RAR, so
    /// `Archive::open` succeeds on whichever one the test opens. Sibling
    /// discovery is name-level and never reads the siblings' contents.
    fn stage_set(dir_name: &str, volume_names: &[&str]) -> PathBuf {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join("test_rar5.rar");
        let dir = std::env::temp_dir().join(dir_name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create staging dir");
        for name in volume_names {
            std::fs::copy(&fixture, dir.join(name)).expect("stage volume");
        }
        dir
    }

    /// The parser turns the volume list `detect_multipart` already discovers
    /// into an actionable "volume 2 is missing" diagnostic. Deliberately does
    /// not assert what `detect_multipart` itself concludes about the set, so
    /// wiring the parser into `Archive::multipart_layout` (a follow-up that
    /// owns `src/inspection.rs`) does not invalidate this test.
    #[test]
    fn discovered_volume_list_with_a_hole_reports_the_missing_volume() {
        let dir = stage_set(
            "ua_multipart_continuity_hole",
            &["set.part1.rar", "set.part3.rar"],
        );
        let source = dir.join("set.part1.rar");

        let archive = Archive::open(&source).expect("staged RAR volume must open");
        let (_, discovered) = archive
            .detect_multipart()
            .expect("sibling discovery must succeed");
        assert!(
            discovered.len() >= 2,
            "discovery should have found both staged volumes: {discovered:?}"
        );

        let report = parse_volume_set_for(&source, &discovered);
        assert_eq!(
            report.defects(),
            [VolumeSetDefect::MissingVolumes {
                first: 2,
                last: 2,
                expected_first: "set.part2.rar".to_string(),
            }],
            "set: {:?}",
            report.set()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same discovery, over a set with no hole, reports complete — so the
    /// diagnostic above is not a false positive from staging.
    #[test]
    fn discovered_volume_list_without_a_hole_reports_complete() {
        let dir = stage_set(
            "ua_multipart_continuity_whole",
            &["set.part1.rar", "set.part2.rar"],
        );
        let source = dir.join("set.part1.rar");

        let archive = Archive::open(&source).expect("staged RAR volume must open");
        let (_, discovered) = archive
            .detect_multipart()
            .expect("sibling discovery must succeed");

        let report = parse_volume_set_for(&source, &discovered);
        assert!(
            report.is_complete(),
            "defects: {:?} over {discovered:?}",
            report.defects()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Ruling 8 (OI-0080-004): the facade can report a *defective* set, not only
/// which files are in it.
///
/// This is the gap the ruling closes. `detect_multipart` and
/// `multipart_layout` both reduce a set to a list of paths, and a list cannot
/// express "and a fourth volume is missing between these three" — a hole
/// arrives as a shorter list, indistinguishable from a smaller set. The report
/// was already being computed on every call and discarded; now it is reachable.
///
/// Non-vacuity: the assertion is not merely that the report exists. It is that
/// `multipart_layout` reports the same three parts for the SAME archive, so the
/// defect is information the older shape provably cannot carry.
#[test]
#[cfg(feature = "rar-support")]
#[serial_test::file_serial(rar)]
fn the_facade_reports_a_hole_that_the_layout_cannot_express() {
    // This suite does not pull in tests/common, so use tempfile directly.
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let temp = temp_dir.path();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

    // A set with its middle volume absent: part1 and part3, no part2.
    for part in ["test_multivol.part1.rar", "test_multivol.part3.rar"] {
        std::fs::copy(fixtures.join(part), temp.join(part)).expect("stage a volume");
    }

    let archive = unified_archive::Archive::open(temp.join("test_multivol.part1.rar"))
        .expect("open the first volume");

    let report = archive
        .volume_set_report()
        .expect("a readable directory yields a report");
    assert!(
        !report.is_complete(),
        "a set missing its middle volume is not complete"
    );
    assert!(
        !report.defects().is_empty(),
        "the hole must arrive as a defect, which is the whole point of this method"
    );

    // The control: the older shape sees two files and has nowhere to say that
    // a third belongs between them.
    let (is_multi, parts) = archive
        .detect_multipart()
        .expect("legacy shape still works");
    assert!(is_multi, "two volumes of one set are still a set");
    assert_eq!(
        parts.len(),
        2,
        "the legacy tuple reports the files present and cannot report the one that is not"
    );
}
