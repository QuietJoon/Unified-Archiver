//! Integration tests for FR-022 link-skip warning propagation through
//! selective extraction (R0064-0006/0007/0008 fix, R0064-0073/0074/0075
//! test coverage).
//!
//! `extract_filtered()`, `extract_files()`, and `extract_by_ids()` all
//! return `ResultWithWarnings<()>`. When the selected set contains
//! symlink or hard-link entries, those entries must be skipped and
//! surfaced as `ArchiveWarning::SkippedSymlink`/`SkippedHardLink` —
//! previously all three dispatchers returned `ResultWithWarnings::ok(())`
//! unconditionally, silently discarding the FR-022 signal.

// Links are carried by tar in these fixtures, so the whole file needs the
// libarchive backend (AD 0058 format features).
#![cfg(feature = "libarchive")]

use super::common;

use std::fs;

use unified_archive::Archive;
use unified_archive::error::ArchiveWarning;

fn assert_symlink_warning(warnings: &[ArchiveWarning], expected_path: &str) {
    let found = warnings.iter().any(|w| {
        matches!(
            w,
            ArchiveWarning::SkippedSymlink { path, .. } if path == expected_path
        )
    });
    assert!(
        found,
        "expected SkippedSymlink warning for '{}', got {:?}",
        expected_path, warnings
    );
}

fn assert_hardlink_warning(warnings: &[ArchiveWarning], expected_path: &str) {
    let found = warnings.iter().any(|w| {
        matches!(
            w,
            ArchiveWarning::SkippedHardLink { path } if path == expected_path
        )
    });
    assert!(
        found,
        "expected SkippedHardLink warning for '{}', got {:?}",
        expected_path, warnings
    );
}

#[cfg(unix)]
#[test]
fn extract_filtered_propagates_symlink_warning() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("filtered.tar");
    let out_dir = tmp.join("out");
    common::build_tar_with_symlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    let result = opened
        .extract_filtered(|_| true, common::default_extraction_options(&out_dir))
        .expect("extract_filtered");

    assert_eq!(result.warnings.len(), 1, "exactly one link warning");
    assert_symlink_warning(&result.warnings, "link.txt");
    assert!(
        out_dir.join("regular.txt").is_file(),
        "regular file must be extracted alongside skipped symlink"
    );
    assert!(
        !out_dir.join("link.txt").exists(),
        "symlink entry must not be materialized"
    );

    common::cleanup(&tmp);
}

#[cfg(unix)]
#[test]
fn extract_files_propagates_symlink_warning() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("files.tar");
    let out_dir = tmp.join("out");
    common::build_tar_with_symlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    let result = opened
        .extract_files(
            &["regular.txt", "link.txt"],
            common::default_extraction_options(&out_dir),
        )
        .expect("extract_files");

    assert_eq!(result.warnings.len(), 1, "exactly one link warning");
    assert_symlink_warning(&result.warnings, "link.txt");
    assert!(
        out_dir.join("regular.txt").is_file(),
        "regular file must be extracted"
    );
    assert!(
        !out_dir.join("link.txt").exists(),
        "symlink entry must not be materialized"
    );

    common::cleanup(&tmp);
}

#[cfg(unix)]
#[test]
fn extract_files_all_links_returns_only_warnings() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("only_link.tar");
    let out_dir = tmp.join("out");
    common::build_tar_with_symlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    let result = opened
        .extract_files(&["link.txt"], common::default_extraction_options(&out_dir))
        .expect("extract_files with only a link");

    assert_eq!(result.warnings.len(), 1, "exactly one link warning");
    assert_symlink_warning(&result.warnings, "link.txt");
    assert!(
        !out_dir.join("link.txt").exists(),
        "symlink entry must not be materialized"
    );

    common::cleanup(&tmp);
}

#[cfg(unix)]
#[test]
fn extract_by_ids_propagates_symlink_warning() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("ids.tar");
    let out_dir = tmp.join("out");
    common::build_tar_with_symlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    let entries = opened.list_files().expect("list");
    let ids: Vec<usize> = entries.iter().map(|e| e.id).collect();
    let link_path = entries
        .iter()
        .find(|e| e.is_symlink())
        .expect("archive must contain a symlink entry")
        .path
        .clone();

    let result = opened
        .extract_by_ids(&ids, common::default_extraction_options(&out_dir))
        .expect("extract_by_ids");

    assert_eq!(result.warnings.len(), 1, "exactly one link warning");
    assert_symlink_warning(&result.warnings, &link_path);
    assert!(
        out_dir.join("regular.txt").is_file(),
        "regular file must be extracted"
    );
    assert!(
        !out_dir.join(&link_path).exists(),
        "symlink entry must not be materialized"
    );

    common::cleanup(&tmp);
}

#[cfg(unix)]
#[test]
fn extract_files_propagates_hardlink_warning() {
    let tmp = common::temp_test_dir();
    let archive = tmp.join("hardlink.tar");
    let out_dir = tmp.join("out");
    common::build_tar_with_hardlink(&archive);
    fs::create_dir_all(&out_dir).unwrap();

    let opened = Archive::open(&archive).expect("open tar");
    // tar on macOS writes the second occurrence as the hardlink record,
    // so either "regular.txt" or "hardlink.txt" may be the skipped entry
    // depending on the emitted order. Discover which via list_files.
    let entries = opened.list_files().expect("list");
    let hardlink_entry = entries
        .iter()
        .find(|e| e.is_hardlink())
        .expect("archive must contain a hard-link entry")
        .path
        .clone();

    let result = opened
        .extract_files(
            &["regular.txt", "hardlink.txt"],
            common::default_extraction_options(&out_dir),
        )
        .expect("extract_files");

    assert_eq!(result.warnings.len(), 1, "exactly one link warning");
    assert_hardlink_warning(&result.warnings, &hardlink_entry);
    assert!(
        !out_dir.join(&hardlink_entry).exists(),
        "hardlink entry must not be materialized"
    );

    common::cleanup(&tmp);
}
