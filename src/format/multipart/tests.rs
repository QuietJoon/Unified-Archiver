//! Unit tests for the typed multipart volume parser (OI-0080-004).
//!
//! Each continuity failure mode named by the ticket gets its own test, so a
//! regression that collapses two of them into one diagnostic fails loudly
//! instead of passing a single "detects a broken set" assertion.

use super::*;

/// Build a candidate list from bare file names.
fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// Build a candidate list under a directory, so path-carrying defects are
/// exercised with realistic multi-component paths.
fn dir_paths(dir: &str, names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|n| Path::new(dir).join(n)).collect()
}

fn expect_set(report: &VolumeSetReport) -> &VolumeSet {
    report.set().expect("report should carry a parsed set")
}

// ── parse_volume_name: scheme recognition ──

#[test]
fn parses_rar_new_style_volume() {
    let v = parse_volume_name("backup.part2.rar").unwrap();
    assert_eq!(v.scheme, VolumeScheme::RarPart);
    assert_eq!(v.base, "backup");
    assert_eq!(v.number, 2);
    assert!(v.numbered);
    assert_eq!(v.width, 1);
}

#[test]
fn parses_rar_new_style_anchored_at_terminal_part_suffix() {
    // A base name that itself contains `.part` must not steal the volume
    // number from the terminal suffix.
    let v = parse_volume_name("my.part9.data.part1.rar").unwrap();
    assert_eq!(v.scheme, VolumeScheme::RarPart);
    assert_eq!(v.base, "my.part9.data");
    assert_eq!(v.number, 1);
}

#[test]
fn parses_rar_old_style_main_and_continuations() {
    let main = parse_volume_name("backup.rar").unwrap();
    assert_eq!(main.scheme, VolumeScheme::RarOldStyle);
    assert_eq!(main.base, "backup");
    assert_eq!(main.number, 1);
    assert!(!main.numbered, "`.rar` carries no explicit volume number");

    assert_eq!(parse_volume_name("backup.r00").unwrap().number, 2);
    assert_eq!(parse_volume_name("backup.r01").unwrap().number, 3);
    // WinRAR rolls `.r99` into `.s00`; the normalised numbering stays
    // contiguous across the roll-over so continuity checks work through it.
    assert_eq!(parse_volume_name("backup.r99").unwrap().number, 101);
    assert_eq!(parse_volume_name("backup.s00").unwrap().number, 102);
    assert_eq!(parse_volume_name("backup.s01").unwrap().number, 103);
}

#[test]
fn parses_zip_split_main_and_segments() {
    let main = parse_volume_name("backup.zip").unwrap();
    assert_eq!(main.scheme, VolumeScheme::ZipSplit);
    assert_eq!(main.number, 1);
    assert!(!main.numbered);

    assert_eq!(parse_volume_name("backup.z01").unwrap().number, 2);
    assert_eq!(parse_volume_name("backup.z02").unwrap().number, 3);
}

#[test]
fn parses_numeric_volume_keeping_inner_extension_in_base() {
    let v = parse_volume_name("backup.tar.001").unwrap();
    assert_eq!(v.scheme, VolumeScheme::Numeric);
    assert_eq!(v.base, "backup.tar");
    assert_eq!(v.number, 1);
    assert_eq!(v.width, 3);
}

#[test]
fn volume_name_matching_is_case_insensitive_but_base_keeps_its_case() {
    let v = parse_volume_name("Backup.PART03.RAR").unwrap();
    assert_eq!(v.scheme, VolumeScheme::RarPart);
    assert_eq!(v.base, "Backup", "base preserves the on-disk casing");
    assert_eq!(v.number, 3);
    assert_eq!(v.width, 2);

    let z = parse_volume_name("Backup.Z01").unwrap();
    assert_eq!(z.scheme, VolumeScheme::ZipSplit);
    assert_eq!(z.number, 2);
}

// ── parse_volume_name: rejections ──

#[test]
fn rejects_non_volume_names() {
    for name in ["notes.txt", "backup", "backup.7z", "backup.tar.gz"] {
        assert!(
            parse_volume_name(name).is_none(),
            "{name} should not parse as a volume name"
        );
    }
}

#[test]
fn rejects_volume_number_zero() {
    // Volume 0 sits outside every convention; admitting it would make the
    // 1-based continuity arithmetic ambiguous.
    assert!(parse_volume_name("backup.000").is_none());

    // A `.rar` name whose digit run is rejected does not become a numbered
    // volume: it degrades to the old-style main volume of a set whose base
    // happens to end in `part0`, which is exactly what the file is.
    for name in ["backup.part0.rar", "backup.part00.rar"] {
        let v = parse_volume_name(name).expect("still a .rar main volume");
        assert_eq!(v.scheme, VolumeScheme::RarOldStyle, "{name}");
        assert!(!v.numbered, "{name} must not claim a volume number");
    }

    // `.r00` is the one legitimate zero: it is the *second* old-style volume,
    // not a zeroth one.
    assert_eq!(parse_volume_name("backup.r00").unwrap().number, 2);
}

#[test]
fn rejects_old_style_runs_that_are_not_exactly_two_digits() {
    for name in ["backup.r0", "backup.r000", "backup.s1"] {
        assert!(
            parse_volume_name(name).is_none(),
            "{name} is outside the WinRAR old-style convention"
        );
    }
}

#[test]
fn rejects_digit_runs_wider_than_the_u32_bound() {
    let ok = format!("backup.part{}.rar", "1".repeat(MAX_VOL_DIGITS));
    let parsed = parse_volume_name(&ok).unwrap();
    assert_eq!(parsed.scheme, VolumeScheme::RarPart);
    assert_eq!(parsed.number, 111_111_111);

    // One digit wider overflows the u32 volume number, so it must not become
    // a numbered volume — matching`detect_multipart`'s bound, where a wider
    // run would pass matching and then mis-sort.
    let too_wide = format!("backup.part{}.rar", "1".repeat(MAX_VOL_DIGITS + 1));
    let parsed = parse_volume_name(&too_wide).unwrap();
    assert_ne!(parsed.scheme, VolumeScheme::RarPart);
    assert!(!parsed.numbered);

    let too_wide_numeric = format!("backup.{}", "1".repeat(MAX_VOL_DIGITS + 1));
    assert!(parse_volume_name(&too_wide_numeric).is_none());
}

#[test]
fn rejects_empty_or_non_numeric_suffixes() {
    for name in ["backup.z", "backup.zz", "backup.z1x"] {
        assert!(
            parse_volume_name(name).is_none(),
            "{name} should not parse as a volume name"
        );
    }
}

#[test]
fn part_suffix_without_digits_is_an_old_style_main_volume() {
    // `.part.rar` / `.partx.rar` carry no volume number, so the whole stem is
    // the base name — the same anchoring `detect_multipart` uses to keep
    // `my.part9.data.part.rar` out of a `.partN.rar` set.
    for (name, base) in [
        ("backup.part.rar", "backup.part"),
        ("backup.partx.rar", "backup.partx"),
    ] {
        let v = parse_volume_name(name).unwrap();
        assert_eq!(v.scheme, VolumeScheme::RarOldStyle, "{name}");
        assert_eq!(v.base, base);
        assert!(!v.numbered);
    }
}

// ── complete sets ──

#[test]
fn complete_rar_new_style_set_reports_complete() {
    let report = parse_volume_set(&paths(&["set.part2.rar", "set.part1.rar", "set.part3.rar"]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    let set = expect_set(&report);
    assert_eq!(set.scheme, VolumeScheme::RarPart);
    assert_eq!(set.base, "set");
    assert_eq!(
        set.paths(),
        paths(&["set.part1.rar", "set.part2.rar", "set.part3.rar"]),
        "volumes come back in volume order regardless of input order"
    );
}

#[test]
fn complete_rar_old_style_set_reports_complete() {
    let report = parse_volume_set(&paths(&["set.r01", "set.rar", "set.r00"]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    let set = expect_set(&report);
    assert_eq!(set.scheme, VolumeScheme::RarOldStyle);
    assert_eq!(set.paths(), paths(&["set.rar", "set.r00", "set.r01"]));
}

#[test]
fn complete_zip_split_set_reports_complete_with_main_first() {
    let report = parse_volume_set(&paths(&["set.z02", "set.zip", "set.z01"]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    assert_eq!(
        expect_set(&report).paths(),
        paths(&["set.zip", "set.z01", "set.z02"]),
        "matches the main-archive-first order detect_multipart already returns"
    );
}

#[test]
fn complete_numeric_set_reports_complete() {
    let report = parse_volume_set(&paths(&["set.7z.002", "set.7z.001"]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    let set = expect_set(&report);
    assert_eq!(set.scheme, VolumeScheme::Numeric);
    assert_eq!(set.base, "set.7z");
}

#[test]
fn single_numbered_volume_is_a_complete_one_volume_set() {
    // No naming convention records the total volume count, so a set truncated
    // at the end is indistinguishable from a short complete one by name. This
    // is a documented limit, asserted so a future change is deliberate.
    let report = parse_volume_set(&paths(&["set.part1.rar"]));
    assert!(report.is_complete());
    assert_eq!(expect_set(&report).volumes.len(), 1);
}

#[test]
fn lone_unnumbered_archive_is_unvolumed() {
    for name in ["set.rar", "set.zip"] {
        let report = parse_volume_set(&paths(&[name]));
        assert!(
            matches!(report, VolumeSetReport::Unvolumed { .. }),
            "{name} alone is a single-volume archive, not a set: {report:?}"
        );
        assert!(report.set().is_none());
        assert!(!report.is_complete());
    }
}

#[test]
fn empty_candidate_list_is_unvolumed() {
    let report = parse_volume_set(&[]);
    assert!(matches!(report, VolumeSetReport::Unvolumed { paths, .. } if paths.is_empty()));
}

// ── continuity failure 1: a missing middle volume ──

#[test]
fn detects_missing_middle_volume() {
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part3.rar"]));
    assert!(!report.is_complete());
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 2,
            last: 2,
            expected_first: "set.part2.rar".to_string(),
        }],
        "the diagnostic must name which volume to go and find"
    );
    // The volumes actually present are still returned, so a caller can list
    // what it has alongside what it needs.
    assert_eq!(
        expect_set(&report).paths(),
        paths(&["set.part1.rar", "set.part3.rar"])
    );
}

#[test]
fn detects_missing_middle_volume_in_old_style_set() {
    // `.r00` present, `.r01` absent, `.r02` present.
    let report = parse_volume_set(&paths(&["set.rar", "set.r00", "set.r02"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 3,
            last: 3,
            expected_first: "set.r01".to_string(),
        }]
    );
}

#[test]
fn detects_missing_middle_volume_across_the_r99_to_s00_rollover() {
    let report = parse_volume_set(&paths(&["set.r98", "set.s00"]));
    let defects = report.defects();
    assert!(
        defects.contains(&VolumeSetDefect::MissingVolumes {
            first: 101,
            last: 101,
            expected_first: "set.r99".to_string(),
        }),
        "the roll-over must not read as a gap of its own: {defects:?}"
    );
}

#[test]
fn reports_one_defect_per_contiguous_missing_run() {
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part4.rar", "set.part7.rar"]));
    assert_eq!(
        report.defects(),
        [
            VolumeSetDefect::MissingVolumes {
                first: 2,
                last: 3,
                expected_first: "set.part2.rar".to_string(),
            },
            VolumeSetDefect::MissingVolumes {
                first: 5,
                last: 6,
                expected_first: "set.part5.rar".to_string(),
            },
        ]
    );
}

#[test]
fn a_huge_gap_collapses_into_one_defect() {
    // The gap is described as a range, so a nine-digit volume number cannot
    // make the parser enumerate a billion missing volumes.
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part999999999.rar"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 2,
            last: 999_999_998,
            expected_first: "set.part2.rar".to_string(),
        }]
    );
}

// ── continuity failure 2: a duplicate index ──

#[test]
fn detects_duplicate_volume_index_from_two_paddings() {
    // `part1` and `part01` are the same volume spelled two ways: the set
    // looks complete by count while the volume order is genuinely ambiguous.
    let report = parse_volume_set(&paths(&[
        "set.part1.rar",
        "set.part01.rar",
        "set.part2.rar",
    ]));
    assert!(!report.is_complete());
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::DuplicateVolume {
            number: 1,
            paths: paths(&["set.part01.rar", "set.part1.rar"]),
        }]
    );
}

#[test]
fn detects_duplicate_volume_index_in_numeric_set() {
    let report = parse_volume_set(&paths(&["set.1", "set.001", "set.002"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::DuplicateVolume {
            number: 1,
            paths: paths(&["set.001", "set.1"]),
        }]
    );
}

#[test]
fn duplicate_volume_is_reported_without_being_dropped_from_the_set() {
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part01.rar"]));
    assert_eq!(
        expect_set(&report).volumes.len(),
        2,
        "both files exist on disk; the set must reflect that"
    );
}

// ── continuity failure 3: an off-by-one first index ──

#[test]
fn detects_set_starting_at_part2() {
    let report = parse_volume_set(&paths(&["set.part2.rar", "set.part3.rar"]));
    assert!(!report.is_complete());
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::FirstVolumeMissing {
            first_present: 2,
            expected_first: "set.part1.rar".to_string(),
        }],
        "a leading gap is its own failure mode, not an interior hole"
    );
}

#[test]
fn detects_old_style_set_missing_its_main_rar_volume() {
    // `set.rar` was not copied, so the set begins at `.r00` (volume 2).
    let report = parse_volume_set(&paths(&["set.r00", "set.r01"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::FirstVolumeMissing {
            first_present: 2,
            expected_first: "set.rar".to_string(),
        }]
    );
}

#[test]
fn detects_zip_split_set_missing_its_main_zip_volume() {
    let report = parse_volume_set(&paths(&["set.z01", "set.z02"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::FirstVolumeMissing {
            first_present: 2,
            expected_first: "set.zip".to_string(),
        }]
    );
}

#[test]
fn leading_gap_and_interior_gap_are_reported_separately() {
    let report = parse_volume_set(&paths(&["set.part2.rar", "set.part4.rar"]));
    assert_eq!(
        report.defects(),
        [
            VolumeSetDefect::FirstVolumeMissing {
                first_present: 2,
                expected_first: "set.part1.rar".to_string(),
            },
            VolumeSetDefect::MissingVolumes {
                first: 3,
                last: 3,
                expected_first: "set.part3.rar".to_string(),
            },
        ]
    );
}

// ── continuity failure 4: a mixed naming scheme ──

#[test]
fn detects_mixed_naming_scheme_within_one_set() {
    let report = parse_volume_set(&dir_paths(
        "/vol",
        &["set.part1.rar", "set.part2.rar", "set.r00"],
    ));
    // cf5109: part1+part2 are contiguous, so the set is complete. The `.r00`
    // is a different scheme and belongs to no set here — reported, but not a
    // continuity defect.
    assert!(report.is_complete());
    assert!(report.defects().is_empty());
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::MixedScheme {
            path: Path::new("/vol/set.r00").to_path_buf(),
            found: VolumeScheme::RarOldStyle,
            expected: VolumeScheme::RarPart,
        }]
    );
    assert_eq!(
        expect_set(&report).volumes.len(),
        2,
        "the foreign volume is reported, not folded into the set"
    );
}

#[test]
fn detects_mixed_numeric_volume_in_a_rar_set() {
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part2.rar", "set.001"]));
    assert!(report.is_complete(), "cf5109: part1+part2 are contiguous");
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::MixedScheme {
            path: PathBuf::from("set.001"),
            found: VolumeScheme::Numeric,
            expected: VolumeScheme::RarPart,
        }]
    );
}

#[test]
fn mixed_scheme_picks_the_convention_with_more_numbered_volumes() {
    // Two old-style continuations outvote a single new-style volume.
    let report = parse_volume_set(&paths(&["set.rar", "set.r00", "set.r01", "set.part1.rar"]));
    assert_eq!(expect_set(&report).scheme, VolumeScheme::RarOldStyle);
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::MixedScheme {
            path: PathBuf::from("set.part1.rar"),
            found: VolumeScheme::RarPart,
            expected: VolumeScheme::RarOldStyle,
        }]
    );
}

#[test]
fn mixed_scheme_defects_are_ordered_deterministically() {
    // Input order must not leak into the reported order.
    let forward = parse_volume_set(&paths(&[
        "set.part1.rar",
        "set.part2.rar",
        "set.r00",
        "set.001",
    ]));
    let reversed = parse_volume_set(&paths(&[
        "set.001",
        "set.r00",
        "set.part2.rar",
        "set.part1.rar",
    ]));
    // cf5109: the determinism this pins is about the *unrelated* list now —
    // the foreign siblings — since neither report has a continuity defect.
    assert_eq!(forward.unrelated(), reversed.unrelated());
    assert_eq!(forward.unrelated().len(), 2);
    assert!(forward.defects().is_empty() && reversed.defects().is_empty());
}

// ── set grouping ──

#[test]
fn detects_unrelated_base_name_in_the_candidate_list() {
    let report = parse_volume_set(&paths(&[
        "set.part1.rar",
        "set.part2.rar",
        "other.part1.rar",
    ]));
    assert!(report.is_complete(), "cf5109: part1+part2 are contiguous");
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::UnrelatedBase {
            path: PathBuf::from("other.part1.rar"),
            found: "other".to_string(),
            expected: "set".to_string(),
        }]
    );
}

#[test]
fn non_volume_names_in_the_candidate_list_are_ignored() {
    let report = parse_volume_set(&paths(&[
        "set.part1.rar",
        "set.part2.rar",
        "readme.txt",
        "set.part2.rar.bak",
    ]));
    assert!(
        report.is_complete(),
        "files that are not volume names are not the set's problem: {:?}",
        report.defects()
    );
}

#[test]
fn mixed_case_set_groups_as_one_set() {
    let report = parse_volume_set(&paths(&["Set.PART1.RAR", "set.part2.rar"]));
    assert!(report.is_complete(), "defects: {:?}", report.defects());
    assert_eq!(expect_set(&report).volumes.len(), 2);
}

// ── anchored parsing ──

#[test]
fn anchored_parse_reports_the_same_set_whichever_member_is_opened() {
    let candidates = paths(&["set.rar", "set.r00", "set.r01"]);
    let reports: Vec<VolumeSetReport> = candidates
        .iter()
        .map(|source| parse_volume_set_for(source, &candidates))
        .collect();
    assert!(reports.iter().all(|r| r.is_complete()));
    assert!(
        reports.windows(2).all(|w| w[0] == w[1]),
        "opening any volume of one set must describe the same set: {reports:?}"
    );
}

#[test]
fn anchored_parse_detects_the_gap_from_a_later_volume() {
    // Opening `.r02` of a set whose `.r01` is absent must still report the
    // hole, not just "you have three files".
    let candidates = paths(&["set.rar", "set.r00", "set.r02"]);
    let report = parse_volume_set_for(Path::new("set.r02"), &candidates);
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 3,
            last: 3,
            expected_first: "set.r01".to_string(),
        }]
    );
}

#[test]
fn anchored_parse_follows_the_anchor_not_the_majority() {
    // The numeric group is larger, but the caller opened a RAR volume, so the
    // answer is about the RAR set.
    let candidates = paths(&["set.part1.rar", "set.001", "set.002", "set.003"]);
    let report = parse_volume_set_for(Path::new("set.part1.rar"), &candidates);
    assert_eq!(expect_set(&report).scheme, VolumeScheme::RarPart);
    assert!(
        report.defects().is_empty(),
        "cf5109: a foreign sibling is not a continuity defect: {:?}",
        report.defects()
    );
    assert_eq!(
        report.unrelated().len(),
        3,
        "the three numeric siblings are foreign to the anchored set: {:?}",
        report.unrelated()
    );
}

#[test]
fn anchored_parse_on_a_lone_main_volume_is_unvolumed() {
    let candidates = paths(&["set.rar"]);
    let report = parse_volume_set_for(Path::new("set.rar"), &candidates);
    assert!(matches!(report, VolumeSetReport::Unvolumed { .. }));
}

#[test]
fn anchored_parse_on_a_main_volume_still_reports_a_foreign_sibling() {
    // `set.rar` alone is not a set, but a `.001` sibling the caller passed in
    // is still worth reporting rather than dropping.
    let candidates = paths(&["set.rar", "set.001"]);
    let report = parse_volume_set_for(Path::new("set.rar"), &candidates);
    // cf5109: `set.rar` has no volumes, so it cannot be *missing* any. It is
    // `Unvolumed`, and the sibling rides along as unrelated rather than being
    // reported as a defect of a set that does not exist.
    assert!(
        matches!(report, VolumeSetReport::Unvolumed { .. }),
        "a lone main volume describes no set: {report:?}"
    );
    assert!(report.defects().is_empty());
    assert_eq!(
        report.unrelated(),
        [VolumeSetDefect::MixedScheme {
            path: PathBuf::from("set.001"),
            found: VolumeScheme::Numeric,
            expected: VolumeScheme::RarOldStyle,
        }]
    );
}

#[test]
fn anchored_parse_falls_back_to_the_majority_for_a_non_volume_source() {
    let candidates = paths(&["set.part1.rar", "set.part2.rar"]);
    let report = parse_volume_set_for(Path::new("notes.txt"), &candidates);
    assert!(report.is_complete());
    assert_eq!(expect_set(&report).scheme, VolumeScheme::RarPart);
}

// ── expected-name rendering ──

#[test]
fn expected_name_reuses_the_padding_the_set_already_uses() {
    let report = parse_volume_set(&paths(&["set.part01.rar", "set.part03.rar"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 2,
            last: 2,
            expected_first: "set.part02.rar".to_string(),
        }],
        "a zero-padded set must be told to look for a zero-padded name"
    );
}

#[test]
fn expected_name_renders_each_scheme() {
    let rar_part = expect_set(&parse_volume_set(&paths(&["set.part1.rar"]))).clone();
    assert_eq!(rar_part.expected_name(1), "set.part1.rar");
    assert_eq!(rar_part.expected_name(12), "set.part12.rar");

    let old_style = expect_set(&parse_volume_set(&paths(&["set.rar", "set.r00"]))).clone();
    assert_eq!(old_style.expected_name(1), "set.rar");
    assert_eq!(old_style.expected_name(2), "set.r00");
    assert_eq!(old_style.expected_name(101), "set.r99");
    assert_eq!(old_style.expected_name(102), "set.s00");

    let zip = expect_set(&parse_volume_set(&paths(&["set.zip", "set.z01"]))).clone();
    assert_eq!(zip.expected_name(1), "set.zip");
    assert_eq!(zip.expected_name(3), "set.z02");

    let numeric = expect_set(&parse_volume_set(&paths(&["set.001"]))).clone();
    assert_eq!(numeric.expected_name(2), "set.002");
    assert_eq!(numeric.expected_name(1234), "set.1234");
}

#[test]
fn expected_name_keeps_the_directory_out_of_the_rendered_name() {
    // Defect names are file names, not paths: the caller knows the directory.
    let report = parse_volume_set(&dir_paths("/vol/sub", &["set.part1.rar", "set.part3.rar"]));
    assert_eq!(
        report.defects(),
        [VolumeSetDefect::MissingVolumes {
            first: 2,
            last: 2,
            expected_first: "set.part2.rar".to_string(),
        }]
    );
}

// ── MultipartLayout bridge ──

#[test]
fn complete_multi_volume_set_maps_to_multipart_layout_multi() {
    let report = parse_volume_set(&paths(&["set.part1.rar", "set.part2.rar"]));
    assert_eq!(
        expect_set(&report).to_layout(),
        MultipartLayout::Multi {
            parts: paths(&["set.part1.rar", "set.part2.rar"]),
        }
    );
}

#[test]
fn one_volume_set_maps_to_multipart_layout_single() {
    let report = parse_volume_set(&paths(&["set.part1.rar"]));
    assert_eq!(
        expect_set(&report).to_layout(),
        MultipartLayout::Single {
            path: PathBuf::from("set.part1.rar"),
        }
    );
}

// ── diagnostics ──

#[test]
fn defect_display_names_the_offending_volume() {
    let missing_one = VolumeSetDefect::MissingVolumes {
        first: 2,
        last: 2,
        expected_first: "set.part2.rar".to_string(),
    };
    assert_eq!(
        missing_one.to_string(),
        "volume 2 is missing (expected `set.part2.rar`)"
    );

    let missing_run = VolumeSetDefect::MissingVolumes {
        first: 2,
        last: 4,
        expected_first: "set.part2.rar".to_string(),
    };
    assert_eq!(
        missing_run.to_string(),
        "volumes 2..=4 are missing (expected `set.part2.rar` first)"
    );

    let leading = VolumeSetDefect::FirstVolumeMissing {
        first_present: 2,
        expected_first: "set.part1.rar".to_string(),
    };
    assert_eq!(
        leading.to_string(),
        "set starts at volume 2, not volume 1 (expected `set.part1.rar`)"
    );

    let duplicate = VolumeSetDefect::DuplicateVolume {
        number: 1,
        paths: paths(&["set.part01.rar", "set.part1.rar"]),
    };
    assert_eq!(
        duplicate.to_string(),
        "volume 1 is claimed by 2 files: `set.part01.rar` `set.part1.rar`"
    );

    let mixed = VolumeSetDefect::MixedScheme {
        path: PathBuf::from("set.r00"),
        found: VolumeScheme::RarOldStyle,
        expected: VolumeScheme::RarPart,
    };
    assert_eq!(
        mixed.to_string(),
        "`set.r00` uses RAR old-style (.rar/.rNN/.sNN) naming but the set uses \
         RAR new-style (.partN.rar)"
    );

    let unrelated = VolumeSetDefect::UnrelatedBase {
        path: PathBuf::from("other.part1.rar"),
        found: "other".to_string(),
        expected: "set".to_string(),
    };
    assert_eq!(
        unrelated.to_string(),
        "`other.part1.rar` has base name `other` but the set's base name is `set`"
    );
}

#[test]
fn scheme_display_matches_its_label() {
    for scheme in [
        VolumeScheme::RarPart,
        VolumeScheme::RarOldStyle,
        VolumeScheme::ZipSplit,
        VolumeScheme::Numeric,
    ] {
        assert_eq!(scheme.to_string(), scheme.label());
    }
}

// ---------------------------------------------------------------------------
// ticgit cf5109: an unrelated neighbour must not make a complete set
// incomplete. `continuity_defects` answers "is a volume missing"; the foreign
// list answers "what else is in this directory". Only the first bears on the
// verdict, and merging them made the normal case — a directory holding more
// than one archive — report a false negative on the one question this API
// exists to answer.
// ---------------------------------------------------------------------------

/// The reported bug, at its smallest.
#[test]
fn a_complete_set_stays_complete_when_a_foreign_archive_shares_the_directory() {
    let paths = vec![
        PathBuf::from("/d/show.part1.rar"),
        PathBuf::from("/d/show.part2.rar"),
        PathBuf::from("/d/show.part3.rar"),
        // Neither a member nor a defect of the set above: a different base,
        // and a different scheme.
        PathBuf::from("/d/unrelated.z01"),
        PathBuf::from("/d/other.part1.rar"),
    ];
    let report = parse_volume_set(&paths);

    assert!(
        report.is_complete(),
        "three contiguous volumes are a complete set whatever else is nearby: {report:?}"
    );
    assert!(
        report.defects().is_empty(),
        "a neighbouring archive is not a continuity defect: {:?}",
        report.defects()
    );
    assert_eq!(
        report.unrelated().len(),
        2,
        "the neighbours are still reported, just not as defects: {:?}",
        report.unrelated()
    );
    assert_eq!(report.set().expect("a set").volumes.len(), 3);
}

/// The control: a genuine gap still makes it incomplete, and the gap is the
/// only thing in `defects()` even with neighbours present.
#[test]
fn a_missing_volume_still_reports_incomplete_with_only_the_gap_as_a_defect() {
    let paths = vec![
        PathBuf::from("/d/show.part1.rar"),
        // part2 absent
        PathBuf::from("/d/show.part3.rar"),
        PathBuf::from("/d/unrelated.z01"),
    ];
    let report = parse_volume_set(&paths);

    assert!(!report.is_complete(), "a gap must still be incomplete");
    assert_eq!(
        report.defects().len(),
        1,
        "exactly the gap, with the neighbour kept out of it: {:?}",
        report.defects()
    );
    assert_eq!(report.unrelated().len(), 1);
}
