//! Regression tests for R0075-0083 — typed `MultipartLayout` return.
//!
//! `Archive::multipart_layout()` replaces the historical
//! `(bool, Vec<PathBuf>)` shape from `detect_multipart` with a typed
//! enum so single-part archives surface distinctly from multipart
//! sets.

use unified_archive::{Archive, MultipartLayout};

#[test]
fn multipart_layout_returns_single_for_non_multipart_zip() {
    let archive = Archive::open(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip"),
    )
    .expect("open test.zip");
    let layout = archive
        .multipart_layout()
        .expect("multipart_layout on a single-part ZIP must succeed");
    match layout {
        MultipartLayout::Single { path } => {
            assert!(
                path.ends_with("test.zip"),
                "Single path should be the source archive"
            );
        }
        MultipartLayout::Multi { parts } => panic!(
            "Expected Single for non-multipart ZIP, got Multi with {} parts",
            parts.len()
        ),
    }
}

#[cfg(feature = "sevenzip")]
#[test]
fn multipart_layout_returns_single_for_non_multipart_7z() {
    let archive = Archive::open(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.7z"),
    )
    .expect("open test.7z");
    let layout = archive.multipart_layout().expect("multipart_layout 7z");
    assert!(matches!(layout, MultipartLayout::Single { .. }));
}

#[test]
fn multipart_layout_returns_single_for_tar() {
    // Compressed/uncompressed TARs are libarchive-backed and not
    // currently multipart-aware. They must surface as Single, not as
    // a one-element Multi.
    let archive = Archive::open(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.tar"),
    )
    .expect("open test.tar");
    let layout = archive.multipart_layout().expect("multipart_layout tar");
    assert!(matches!(layout, MultipartLayout::Single { .. }));
}
