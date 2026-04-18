//! Tests for Phase 1: Progress callback functionality
//!
//! Verifies progress reporting and cancellation support

#[path = "common/mod.rs"]
mod common;

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};
use unified_archive::{Archive, ExtractionOptions};

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_progress_callback_called() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let progress_calls = Arc::new(Mutex::new(Vec::new()));
    let progress_calls_clone = Arc::clone(&progress_calls);

    let dest = std::env::temp_dir().join("unified_archive_test_progress");

    let options = ExtractionOptions {
        progress: Some(Box::new(move |current, total| {
            progress_calls_clone.lock().unwrap().push((current, total));
            ControlFlow::Continue(())
        })),
        ..common::default_extraction_options(dest.clone())
    };

    // Clean up before test
    let _ = std::fs::remove_dir_all(&dest);

    archive.extract_all(options).expect("Extraction failed");

    let calls = progress_calls.lock().unwrap();
    assert!(!calls.is_empty(), "Progress callback should be called");

    // Note: With small test archives (97 bytes), we may get fewer calls due to rate limiting
    // The important thing is that progress callback is invoked and reports completion
    assert!(!calls.is_empty(), "Should have at least 1 progress update");

    // Final call should be 100% (current == total)
    let (last_current, last_total) = calls.last().unwrap();
    assert_eq!(
        *last_current,
        last_total.unwrap(),
        "Final progress should be 100%"
    );

    // Clean up after test
    let _ = std::fs::remove_dir_all(&dest);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_progress_callback_cancellation() {
    // Multi-entry fixture: `ControlFlow::Break` is a no-op on a single-entry
    // archive (nothing left to visit after the first callback).
    let archive =
        Archive::open("tests/fixtures/test_multi.rar").expect("Failed to open multi-entry RAR");
    let total_entries = archive.list_files().unwrap().len();
    assert!(
        total_entries >= 3,
        "cancellation test requires a multi-entry fixture (found {})",
        total_entries
    );

    let calls = Arc::new(Mutex::new(0usize));
    let calls_clone = Arc::clone(&calls);

    let dest = std::env::temp_dir().join(format!(
        "unified_archive_test_cancel_{}",
        std::process::id()
    ));

    let options = ExtractionOptions {
        progress: Some(Box::new(move |_current, _total| {
            let mut n = calls_clone.lock().unwrap();
            *n += 1;
            if *n >= 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })),
        ..common::default_extraction_options(dest.clone())
    };

    let _ = std::fs::remove_dir_all(&dest);

    let result = archive.extract_all(options);

    let observed = *calls.lock().unwrap();
    // Contract: on Break the backend must either surface an error OR stop
    // invoking the callback before reaching every entry. A backend that
    // silently finishes all entries must fail this assertion.
    assert!(
        result.is_err() || observed < total_entries,
        "ControlFlow::Break must short-circuit extraction; \
         got {} callbacks for {} entries, result={:?}",
        observed,
        total_entries,
        result
    );

    let _ = std::fs::remove_dir_all(&dest);
}

#[test]
fn test_progress_callback_zip() {
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");

    let progress_calls = Arc::new(Mutex::new(Vec::new()));
    let progress_calls_clone = Arc::clone(&progress_calls);

    let dest = std::env::temp_dir().join("unified_archive_test_zip");

    let options = ExtractionOptions {
        progress: Some(Box::new(move |current, total| {
            progress_calls_clone.lock().unwrap().push((current, total));
            ControlFlow::Continue(())
        })),
        ..common::default_extraction_options(dest.clone())
    };

    // Clean up before test
    let _ = std::fs::remove_dir_all(&dest);

    archive.extract_all(options).expect("ZIP extraction failed");

    let calls = progress_calls.lock().unwrap();
    assert!(
        !calls.is_empty(),
        "Progress callback should be called for ZIP"
    );

    // Final call should be 100%
    let (last_current, last_total) = calls.last().unwrap();
    assert_eq!(
        *last_current,
        last_total.unwrap(),
        "Final ZIP progress should be 100%"
    );

    // Clean up after test
    let _ = std::fs::remove_dir_all(&dest);
}

#[test]
fn test_progress_callback_7z() {
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");

    let progress_calls = Arc::new(Mutex::new(Vec::new()));
    let progress_calls_clone = Arc::clone(&progress_calls);

    let dest = std::env::temp_dir().join("unified_archive_test_7z");

    let options = ExtractionOptions {
        progress: Some(Box::new(move |current, total| {
            progress_calls_clone.lock().unwrap().push((current, total));
            ControlFlow::Continue(())
        })),
        ..common::default_extraction_options(dest.clone())
    };

    // Clean up before test
    let _ = std::fs::remove_dir_all(&dest);

    archive.extract_all(options).expect("7z extraction failed");

    let calls = progress_calls.lock().unwrap();
    assert!(
        !calls.is_empty(),
        "Progress callback should be called for 7z"
    );

    // Final call should be 100%
    let (last_current, last_total) = calls.last().unwrap();
    assert_eq!(
        *last_current,
        last_total.unwrap(),
        "Final 7z progress should be 100%"
    );

    // Clean up after test
    let _ = std::fs::remove_dir_all(&dest);
}

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn test_progress_without_callback() {
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");

    let dest = std::env::temp_dir().join("unified_archive_test_no_callback");

    let options = ExtractionOptions {
        progress: None, // No callback
        ..common::default_extraction_options(dest.clone())
    };

    // Clean up before test
    let _ = std::fs::remove_dir_all(&dest);

    // Should work fine without progress callback
    archive
        .extract_all(options)
        .expect("Extraction without callback failed");

    // Clean up after test
    let _ = std::fs::remove_dir_all(&dest);
}
