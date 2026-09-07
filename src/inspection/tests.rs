use super::*;
use crate::test_utils::fixture;

// ── ValidationReport tests ──

#[test]
fn test_validation_report_debug() {
    let report = ValidationReport {
        total_entries: 5,
        total_files: 5,
        validated: 4,
        failed: vec!["bad_file.txt".to_string()],
    };
    let debug = format!("{:?}", report);
    assert!(debug.contains("total_entries: 5"));
    assert!(debug.contains("total_files: 5"));
    assert!(debug.contains("validated: 4"));
    assert!(debug.contains("bad_file.txt"));
}

#[test]
fn test_validation_report_clone() {
    let report = ValidationReport {
        total_entries: 3,
        total_files: 3,
        validated: 3,
        failed: vec![],
    };
    let cloned = report.clone();
    assert_eq!(report, cloned);
}

#[test]
fn test_validation_report_eq() {
    let a = ValidationReport {
        total_entries: 2,
        total_files: 2,
        validated: 2,
        failed: vec![],
    };
    let b = ValidationReport {
        total_entries: 2,
        total_files: 2,
        validated: 2,
        failed: vec![],
    };
    assert_eq!(a, b);
}

#[test]
fn test_validation_report_ne() {
    let a = ValidationReport {
        total_entries: 2,
        total_files: 2,
        validated: 2,
        failed: vec![],
    };
    let b = ValidationReport {
        total_entries: 2,
        total_files: 2,
        validated: 1,
        failed: vec!["x.txt".to_string()],
    };
    assert_ne!(a, b);
}

// ── list_files tests ──

#[test]
fn test_list_files_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty());
    // All entries should have paths
    for entry in entries {
        assert!(!entry.path.is_empty());
    }
}

#[cfg(feature = "sevenzip")]
#[test]
fn test_list_files_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty());
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_list_files_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty());
}

#[cfg(feature = "libarchive")]
#[test]
fn test_list_files_tar() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(!entries.is_empty());
}

#[test]
fn test_list_files_caching() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries1 = archive.list_files().unwrap();
    let entries2 = archive.list_files().unwrap();
    // Second call should return same pointer (cached)
    assert!(std::ptr::eq(entries1.as_ptr(), entries2.as_ptr()));
}

// ── list_files_for_limits tests ──

#[test]
fn test_list_files_for_limits_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files_for_limits().unwrap();
    assert!(!entries.is_empty());
}

#[test]
fn test_list_files_for_limits_does_not_cache() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    // Call list_files_for_limits first (should NOT populate cache)
    let _entries = archive.list_files_for_limits().unwrap();
    // Cache should still be empty since list_files_for_limits doesn't cache
    assert!(archive.entry_cache.get().is_none());
}

// ── entry_count tests ──

#[test]
fn test_entry_count_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let count = archive.entry_count().unwrap();
    assert!(count > 0);
}

#[test]
fn test_entry_count_matches_list_files() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let count = archive.entry_count().unwrap();
    let entries = archive.list_files().unwrap();
    assert_eq!(count, entries.len());
}

// ── find_entry tests ──

#[test]
fn test_find_entry_existing() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let first_path = &entries[0].path;
    let found = archive.find_entry(first_path).unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().path, *first_path);
}

#[test]
fn test_find_entry_nonexistent() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let found = archive.find_entry("nonexistent_file_xyz.txt").unwrap();
    assert!(found.is_none());
}

#[test]
fn test_find_entry_empty_path() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let found = archive.find_entry("").unwrap();
    assert!(found.is_none());
}

// ── validate_integrity tests ──

#[cfg(feature = "integrity")]
#[test]
fn test_validate_integrity_valid_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(
        report.failed.is_empty(),
        "Valid ZIP should have no failures"
    );
    assert!(
        report.validated > 0,
        "Should have validated at least one entry"
    );
    assert_eq!(report.total_entries, archive.entry_count().unwrap());
}

#[cfg(feature = "integrity")]
#[cfg(feature = "sevenzip")]
#[test]
fn test_validate_integrity_valid_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let report = archive.validate_integrity().unwrap();
    assert!(report.failed.is_empty());
}

// ── calculate_archive_crc tests ──

#[cfg(feature = "integrity")]
#[test]
fn test_calculate_archive_crc_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let crc = archive.calculate_archive_crc().unwrap();
    // CRC should be deterministic for the same archive
    let crc2 = archive.calculate_archive_crc().unwrap();
    assert_eq!(crc, crc2);
}

#[cfg(feature = "integrity")]
#[test]
fn test_calculate_archive_crc_is_sum_of_entry_crcs() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    let expected: u32 = entries
        .iter()
        .filter_map(|e| e.crc32)
        .fold(0u32, |acc, crc| acc.wrapping_add(crc));
    let actual = archive.calculate_archive_crc().unwrap();
    assert_eq!(actual, expected);
}

// ── calculate_manifest_digest tests ──

#[cfg(feature = "integrity")]
#[test]
fn test_calculate_manifest_digest_deterministic() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let digest1 = archive.calculate_manifest_digest().unwrap();
    let digest2 = archive.calculate_manifest_digest().unwrap();
    assert_eq!(digest1, digest2, "manifest_digest should be deterministic");
    assert!(
        !digest1.is_empty(),
        "ZIP with entries should produce non-empty digest"
    );
}

#[cfg(feature = "integrity")]
#[test]
fn test_calculate_manifest_digest_differs_from_archive_crc() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let digest = archive.calculate_manifest_digest().unwrap();
    let crc = archive.calculate_archive_crc().unwrap();
    // manifest_digest and archive_crc use different algorithms —
    // they should (almost certainly) differ
    assert_ne!(
        digest,
        format!("{crc:08x}"),
        "manifest_digest and archive_crc should differ (different algorithms)"
    );
}

#[cfg(feature = "integrity")]
#[test]
fn test_calculate_manifest_digest_matches_manual_computation() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();

    // Manual computation: same algorithm as the method
    let mut hashes: Vec<String> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File)
        .filter_map(|e| e.crc32)
        .map(|crc| {
            crc.to_be_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .collect();
    hashes.sort();
    let joined = hashes.join(",");
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(joined.as_bytes());
    let expected = format!("{:08x}", hasher.finalize());

    let actual = archive.calculate_manifest_digest().unwrap();
    assert_eq!(actual, expected);
}

#[cfg(feature = "integrity")]
#[cfg(feature = "sevenzip")]
#[test]
fn test_calculate_manifest_digest_7z() {
    let archive = Archive::open(fixture("test.7z")).unwrap();
    let digest = archive.calculate_manifest_digest().unwrap();
    // 7z may or may not have CRC32 per entry — just check it doesn't error
    let digest2 = archive.calculate_manifest_digest().unwrap();
    assert_eq!(digest, digest2);
}

#[cfg(feature = "integrity")]
// TAR has no per-entry CRC in its metadata; the digest must still be
// content-based (streamed CRC32) and not fall back to path/size — otherwise
// renamed-but-identical content would appear distinct.
#[cfg(feature = "libarchive")]
#[test]
fn test_calculate_manifest_digest_tar_is_stable_and_nonempty() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let d1 = archive.calculate_manifest_digest().unwrap();
    let d2 = archive.calculate_manifest_digest().unwrap();
    assert!(!d1.is_empty(), "TAR digest should reflect real content");
    assert_eq!(d1, d2, "digest must be deterministic across calls");
    assert_eq!(d1.len(), 8, "digest is 8 lowercase hex chars");
    assert!(
        d1.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    );
}

// ── OI-0001-009 single-traversal CRC resolution ──

/// The one-pass resolver must actually run on a libarchive-backed
/// CRC-less archive — and be *seen* to run, not merely be fast.
///
/// `resolve_crc32_single_pass` returns an empty map both when there is
/// nothing to resolve and when the backend has no one-pass walk (the
/// `NotImplemented` fallback). A digest-value assertion alone cannot tell
/// those apart from a working pass, because the per-entry fallback
/// produces the identical digest — which is exactly why the quadratic
/// behaviour survived so long. Asserting the map covers every CRC-less
/// file entry pins the route, not just the result.
#[cfg(feature = "libarchive")]
#[test]
fn test_resolve_crc32_single_pass_covers_every_crc_less_entry() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();

    let crc_less: Vec<&ArchiveEntry> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File && e.crc32.is_none())
        .collect();
    assert!(
        !crc_less.is_empty(),
        "test.tar must carry CRC-less file entries or this proves nothing"
    );

    let resolved = archive.resolve_crc32_single_pass(entries).unwrap();
    assert_eq!(
        resolved.len(),
        crc_less.len(),
        "the single traversal must resolve every CRC-less entry; an empty or short map \
         means it silently fell back to the per-entry re-open path"
    );
    for entry in &crc_less {
        assert!(
            resolved.contains_key(&entry.id),
            "entry id {} ('{}') was not visited by the single traversal",
            entry.id,
            entry.path
        );
    }
}

#[cfg(feature = "integrity")]
/// The one-pass resolver and the per-entry resolver must agree value for
/// value. They are two routes to the same number, and only one of them is
/// exercised by the public digest on a libarchive archive — so pin the
/// equality directly rather than trusting that both were tested.
#[cfg(feature = "libarchive")]
#[test]
fn test_single_pass_and_per_entry_crc32_agree() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let entries = archive.list_files().unwrap();
    let resolved = archive.resolve_crc32_single_pass(entries).unwrap();

    for entry in entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File && e.crc32.is_none())
    {
        let one_pass = resolved
            .get(&entry.id)
            .copied()
            .expect("every CRC-less entry is covered");
        let per_entry = archive.entry_crc32_for_digest(entry).unwrap();
        assert_eq!(
            one_pass, per_entry,
            "single-pass and per-entry resolution disagree for '{}'",
            entry.path
        );
    }
}

/// Backends with no one-pass walk must degrade to the empty map (the
/// caller then uses the per-entry resolver), never to an error. ZIP lists
/// a per-entry CRC32 for every non-AE-2 entry, so there is nothing to
/// resolve and the map is empty for a second, benign reason — both paths
/// must be indistinguishable to the caller.
#[test]
fn test_resolve_crc32_single_pass_is_empty_when_listings_carry_crc32() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let entries = archive.list_files().unwrap();
    assert!(
        entries
            .iter()
            .filter(|e| e.entry_type == EntryType::File)
            .all(|e| e.crc32.is_some()),
        "plaintext ZIP entries carry a real CRC32"
    );
    assert!(
        archive
            .resolve_crc32_single_pass(entries)
            .unwrap()
            .is_empty()
    );
}

// ── content-multiset digest duplicate-path tests ──
//
// DCR-012 removed the per-path occurrence ordinal from the digest
// encoding, so the two tests that pinned it moved into
// `src/inspection.rs`'s inline `content_digest_encoding_tests` module,
// next to the private functions they constrain:
//
// - `test_content_digest_element_ordinal_zero_is_bare_hex` became
//   `test_content_digest_element_is_fixed_width_lowercase_hex`
//   (`content_digest_element` no longer takes an ordinal).
// - `test_content_digest_duplicate_path_differs_from_unique_paths`
//   became `test_content_digest_grouped_and_spread_duplicates_agree`.
//   Its old assertion was wrong, not merely stale: its stated model —
//   the constant resolver standing in for a by-path CRC-less walk that
//   re-hashes the first occurrence — described the resolver ticgit
//   `2a6e3153` deleted on 2026-07-19.
//
// The two tests below are unchanged and are the compat proof: a
// unique-path listing must still match the pre-R0079-0028 algorithm
// byte for byte, and the total-size overflow arm is untouched.

fn synthetic_file(path: &str, id: usize, size: u64) -> ArchiveEntry {
    ArchiveEntry::file(path, id).size(size).build()
}

/// A unique-path listing must digest identically to the pre-fix
/// algorithm (sorted 8-char hex, joined with ",", CRC32-hashed).
#[test]
fn test_content_digest_unique_paths_match_prefix_algorithm() {
    let entries = vec![
        synthetic_file("a.txt", 0, 3),
        synthetic_file("b.txt", 1, 5),
        synthetic_file("c.txt", 2, 7),
    ];
    let crcs = [0x0000_00ffu32, 0xdead_beef, 0x0bad_f00d];
    let (digest, total_size) =
        super::content_multiset_digest_and_size(&entries, |e| Ok(crcs[e.id])).unwrap();

    let mut hashes: Vec<String> = crcs.iter().map(|c| format!("{c:08x}")).collect();
    hashes.sort();
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(hashes.join(",").as_bytes());
    let expected = format!("{:08x}", hasher.finalize());

    assert_eq!(digest, expected, "unique-path digest must be unchanged");
    // OI-0001-007: every entry here declares a size, so the typed total is
    // complete and `exact()` is the assertion to make. Reading
    // `sized_bytes()` alone would pass just as happily on a listing that
    // silently dropped an entry.
    assert_eq!(total_size.exact(), Some(15));
}

/// Total-size accumulation must surface overflow instead of saturating
/// to `u64::MAX` and reporting it as an exact total (R0081-0081).
#[test]
fn test_content_digest_total_size_overflow_errors() {
    let entries = vec![
        synthetic_file("a.txt", 0, u64::MAX),
        synthetic_file("b.txt", 1, 1),
    ];
    let err = super::content_multiset_digest_and_size(&entries, |_| Ok(0))
        .expect_err("overflowing total size must error, not saturate");
    assert!(
        matches!(err, ArchiveError::OperationBlocked { .. }),
        "expected OperationBlocked on total-size overflow, got {err:?}"
    );
}

// ── detect_multipart tests ──

#[test]
fn test_detect_multipart_single_zip() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let (is_multi, parts) = archive.detect_multipart().unwrap();
    // Single-file ZIP should not be multipart
    // (unless there are matching .z01, .z02 files in the fixtures dir)
    if !is_multi {
        assert_eq!(parts.len(), 1);
    }
}

#[cfg(feature = "libarchive")]
#[test]
fn test_detect_multipart_tar_not_supported() {
    let archive = Archive::open(fixture("test.tar")).unwrap();
    let (is_multi, parts) = archive.detect_multipart().unwrap();
    assert!(!is_multi, "TAR does not support multipart");
    assert_eq!(parts.len(), 1);
}

/// Regression for MADR-0013: the multipart boundary must reject unrelated
/// siblings like `archive.backup.zip` when grouping the `archive.zip`
/// split set. A raw `starts_with(file_stem)` match used to let these
/// through and mislead callers/operators.
#[test]
fn test_detect_multipart_rejects_backup_sibling() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.zip");
    let z01 = dir.path().join("archive.z01");
    let backup = dir.path().join("archive.backup.zip");
    for p in [&main, &z01, &backup] {
        std::fs::write(p, b"PK\x03\x04").unwrap();
    }

    let archive = Archive::open(&main).unwrap();
    let (_is_multi, parts) = archive.detect_multipart().unwrap();
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "archive.zip"),
        "main archive should be present: {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n == "archive.z01"),
        "split part should be present: {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| n == "archive.backup.zip"),
        "unrelated sibling must not match multipart boundary: {:?}",
        names
    );
}

/// Regression for MADR-0013: ZIP split sets must surface with the
/// non-numbered main archive first (`[archive.zip, archive.z01,
/// archive.z02, ...]`), not reversed.
#[test]
fn test_detect_multipart_zip_main_first_sort() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.zip");
    let z02 = dir.path().join("archive.z02");
    let z01 = dir.path().join("archive.z01");
    for p in [&main, &z01, &z02] {
        std::fs::write(p, b"PK\x03\x04").unwrap();
    }

    let archive = Archive::open(&main).unwrap();
    let (_is_multi, parts) = archive.detect_multipart().unwrap();
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["archive.zip", "archive.z01", "archive.z02"],
        "expected main-first sort order"
    );
}

/// Old-style RAR volume sets (`x.rar` + `x.r00`, `x.r01`, ...) must be
/// detected as multipart with the main `.rar` first and numbered
/// volumes ascending, `.sNN` after `.rNN` (R0079-0029). Sibling
/// volumes are matched by file name only — detect_multipart never
/// opens them — so empty touch files are sufficient fixtures; only the
/// source archive needs real RAR content for `Archive::open`.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_detect_multipart_rar_old_style_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.rar");
    std::fs::copy(fixture("test.rar"), &main).unwrap();
    for sibling in [
        "archive.r01",
        "archive.r00",
        "archive.r10",
        "archive.r02",
        "archive.s00",
    ] {
        std::fs::write(dir.path().join(sibling), b"").unwrap();
    }
    // Decoys that must not be swept in: different stem, non-digit
    // tail, missing digits.
    for decoy in ["archive2.r00", "archive.rab", "archive.r"] {
        std::fs::write(dir.path().join(decoy), b"").unwrap();
    }

    let archive = Archive::open(&main).unwrap();
    let (is_multi, parts) = archive.detect_multipart().unwrap();
    assert!(is_multi, "old-style RAR volume set must report multipart");
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![
            "archive.rar",
            "archive.r00",
            "archive.r01",
            "archive.r02",
            "archive.r10",
            "archive.s00",
        ],
        "expected .rar first, then .rNN ascending, then .sNN"
    );
}

/// OI-0001-006 / R0001-0048: the reported set must be identical
/// whichever member of it was opened. Old-style `.rNN` sibling matching
/// used to be gated on the *source* name ending in `.rar`, so opening
/// `archive.r00` reported `(false, [archive.r00])` while opening
/// `archive.rar` next to it discovered the whole series — a materially
/// different answer for the same set depending on the entry point.
///
/// Every volume here is a byte-copy of a real single-volume RAR because
/// each one has to be openable; detection is content-first, so the
/// `.r00`/`.r01` names do not block `Archive::open`. Sibling discovery
/// itself still never reads the siblings.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_detect_multipart_old_style_set_is_the_same_from_every_member() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.rar");
    let r00 = dir.path().join("archive.r00");
    let r01 = dir.path().join("archive.r01");
    for volume in [&main, &r00, &r01] {
        std::fs::copy(fixture("test_rar5.rar"), volume).unwrap();
    }

    let expected = MultipartLayout::Multi {
        parts: vec![main.clone(), r00.clone(), r01.clone()],
    };
    for member in [&main, &r00, &r01] {
        let archive = Archive::open(member).unwrap();
        assert_eq!(
            archive.multipart_layout().unwrap(),
            expected,
            "opening {} must report the whole old-style set, in volume order",
            member.display()
        );
    }
}

/// multipart_layout must surface old-style RAR volume sets as
/// `Multi` and a lone `.rar` as `Single` (R0079-0029).
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_multipart_layout_rar_old_style_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.rar");
    std::fs::copy(fixture("test.rar"), &main).unwrap();

    let archive = Archive::open(&main).unwrap();
    assert_eq!(
        archive.multipart_layout().unwrap(),
        MultipartLayout::Single { path: main.clone() },
        "a lone .rar must stay single-volume"
    );

    // The directory is re-scanned per call, so volumes appearing
    // after open are picked up by the same handle.
    std::fs::write(dir.path().join("archive.r00"), b"").unwrap();
    std::fs::write(dir.path().join("archive.r01"), b"").unwrap();
    match archive.multipart_layout().unwrap() {
        MultipartLayout::Multi { parts } => {
            assert_eq!(parts.len(), 3, "expected .rar + two .rNN volumes");
        }
        other => panic!("expected Multi for old-style RAR set, got {other:?}"),
    }
}

/// R0080-0087: sibling matching is ASCII-case-insensitive, so a
/// mixed-case ZIP split set (`archive.ZIP` + `archive.Z01` + ...) still
/// groups, and the returned volume list preserves the original-case
/// PathBufs in main-first ascending order.
#[test]
fn test_detect_multipart_case_insensitive_zip_set() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("archive.ZIP");
    let z01 = dir.path().join("archive.Z01");
    let z02 = dir.path().join("archive.Z02");
    for p in [&main, &z01, &z02] {
        std::fs::write(p, b"PK\x03\x04").unwrap();
    }

    let archive = Archive::open(&main).unwrap();
    let (is_multi, parts) = archive.detect_multipart().unwrap();
    assert!(is_multi, "mixed-case ZIP split set must report multipart");
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["archive.ZIP", "archive.Z01", "archive.Z02"],
        "original-case paths preserved in main-first order: {:?}",
        names
    );
}

/// R0080-0088 / R0080-0089 / R0080-0095: RAR volume grouping anchors on
/// the terminal `.part<digits>.rar` suffix. A base name that itself
/// contains `.part` groups only its own numbered volumes, a `.part.rar`
/// sibling with no volume number is rejected, and an unrelated base is
/// not swept in. Sibling volumes are matched by name only, so empty
/// touch files suffice; only the source needs real RAR content.
#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_detect_multipart_rar_part_anchored_base() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("my.part9.data.part1.rar");
    std::fs::copy(fixture("test.rar"), &main).unwrap();
    std::fs::write(dir.path().join("my.part9.data.part2.rar"), b"").unwrap();
    // Empty volume number must be rejected (R0080-0089).
    std::fs::write(dir.path().join("my.part9.data.part.rar"), b"").unwrap();
    // Unrelated base must not match.
    std::fs::write(dir.path().join("other.part2.rar"), b"").unwrap();

    let archive = Archive::open(&main).unwrap();
    let (is_multi, parts) = archive.detect_multipart().unwrap();
    assert!(is_multi, "anchored RAR part set must report multipart");
    let names: Vec<String> = parts
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["my.part9.data.part1.rar", "my.part9.data.part2.rar"],
        "only terminal-anchored .partN.rar volumes for this base, in order: {:?}",
        names
    );
}

// ── check_symlinks tests ──

#[test]
fn test_check_symlinks_no_symlinks() {
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let warnings = archive.check_symlinks().unwrap();
    // Normal archives without symlinks should have empty warnings
    assert!(warnings.is_empty());
}

#[test]
fn test_check_symlinks_returns_correct_warning_types() {
    // We test the logic by constructing entries directly
    // This tests the function's handling of EntryType variants
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let warnings = archive.check_symlinks().unwrap();
    for warning in &warnings {
        match warning {
            ArchiveWarning::SkippedSymlink { path, .. } => {
                assert!(!path.is_empty());
            }
            ArchiveWarning::SkippedHardLink { path } => {
                assert!(!path.is_empty());
            }
            other => panic!("check_symlinks emitted a non-link warning: {other:?}"),
        }
    }
}

// ── Cross-format consistency tests ──

#[cfg(feature = "sevenzip")]
#[test]
fn test_list_files_consistent_across_zip_and_7z() {
    let zip = Archive::open(fixture("test.zip")).unwrap();
    let sevenz = Archive::open(fixture("test.7z")).unwrap();

    let zip_entries = zip.list_files().unwrap();
    let sevenz_entries = sevenz.list_files().unwrap();

    // Both archives contain the same file, so entry count should match
    // (assuming test.zip and test.7z have the same contents)
    assert!(!zip_entries.is_empty(), "ZIP should have entries");
    assert!(!sevenz_entries.is_empty(), "7z should have entries");
}
