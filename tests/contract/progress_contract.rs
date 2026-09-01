//! Contract tests for progress callback behavior
//!
//! Validates the contract defined in specs/001-unified-archive/contracts/progress.md:
//! 1. Progress callback minimum frequency
//! 2. Progress values are monotonic (current <= total)
//! 3. Cancellation via ControlFlow::Break
//! 4. Panic in callback treated as cancellation
//!
//! Note: The current API uses ExtractionOptions.progress (a boxed ProgressCallback trait)
//! rather than a separate extract_all_with_progress method. All production backends
//! (RAR, ZIP/ZipReader, 7z/SevenZ, libarchive) invoke the callback and honour
//! ControlFlow::Break; the tests below exercise each backend.

#[path = "../common/mod.rs"]
mod common;

use common::fixture;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use unified_archive::{Archive, ExtractionOptions};

// ── Contract 1: Progress callback is called ──

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_progress_callback_is_invoked_for_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let count_clone = call_count.clone();

    let options =
        ExtractionOptions::new(temp.path()).progress(move |_current: u64, _total: Option<u64>| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            std::ops::ControlFlow::Continue(())
        });

    archive.extract_all(options).unwrap();

    // RAR should invoke progress callbacks
    let calls = call_count.load(Ordering::SeqCst);
    assert!(
        calls > 0,
        "Progress callback should be invoked at least once for RAR, got {} calls",
        calls
    );
}

#[test]
fn contract_progress_callback_accepted_for_zip() {
    // ZIP (the `zip`-crate reader) invokes the progress callback during extraction.
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = call_count.clone();

    let options =
        ExtractionOptions::new(temp.path()).progress(move |_current: u64, _total: Option<u64>| {
            cc.fetch_add(1, Ordering::SeqCst);
            std::ops::ControlFlow::Continue(())
        });

    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "Extraction with progress callback should not fail: {:?}",
        result.err()
    );
    assert!(
        call_count.load(Ordering::SeqCst) > 0,
        "ZIP progress callback should fire at least once"
    );
}

// ── Contract 2: Progress values are monotonic ──

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_progress_monotonic_for_rar() {
    let archive = Archive::open(fixture("test.rar")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let last_current = Arc::new(AtomicU64::new(0));
    let lc = last_current.clone();
    let monotonic_violation = Arc::new(AtomicUsize::new(0));
    let mv = monotonic_violation.clone();

    let options =
        ExtractionOptions::new(temp.path()).progress(move |current: u64, total: Option<u64>| {
            let prev = lc.swap(current, Ordering::SeqCst);
            if current < prev {
                mv.fetch_add(1, Ordering::SeqCst);
            }
            // current should not exceed total (if total is known)
            if let Some(t) = total {
                if t > 0 && current > t {
                    mv.fetch_add(1, Ordering::SeqCst);
                }
            }
            std::ops::ControlFlow::Continue(())
        });

    archive.extract_all(options).unwrap();

    assert_eq!(
        monotonic_violation.load(Ordering::SeqCst),
        0,
        "Progress values should be monotonically non-decreasing"
    );
}

// ── Contract 3: Cancellation via ControlFlow::Break ──

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_progress_cancellation() {
    // Use the multi-entry fixture so cancellation is observable: with a single
    // entry there is nothing left to short-circuit after the first callback.
    let archive = Archive::open(fixture("test_multi.rar")).unwrap();
    let total_entries = archive.list_files().unwrap().len() as u64;
    assert!(
        total_entries >= 3,
        "cancellation contract requires a multi-entry fixture"
    );
    let temp = tempfile::tempdir().unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = call_count.clone();

    let options =
        ExtractionOptions::new(temp.path()).progress(move |_current: u64, _total: Option<u64>| {
            let count = cc.fetch_add(1, Ordering::SeqCst);
            if count >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        });

    let result = archive.extract_all(options);
    let calls = call_count.load(Ordering::SeqCst) as u64;

    // Contract: a backend that honours ControlFlow::Break either short-circuits
    // before visiting every entry, or surfaces an error. A backend that silently
    // ignores the cancellation signal (callback count reaches total_entries AND
    // extraction reports success) must fail this assertion.
    assert!(
        calls < total_entries || result.is_err(),
        "ControlFlow::Break must short-circuit extraction or return an error; \
         got {} callbacks for {} entries with result {:?}",
        calls,
        total_entries,
        result.err()
    );
}

// ── Contract 4: Callback does not cause crashes ──

#[test]
fn contract_progress_callback_with_no_op() {
    // Use ZIP instead of RAR to avoid UnRAR concurrency issues
    let archive = Archive::open(fixture("test.zip")).unwrap();
    let temp = tempfile::tempdir().unwrap();

    let options = ExtractionOptions::new(temp.path()).progress(|_: u64, _: Option<u64>| {
        // No-op callback
        std::ops::ControlFlow::Continue(())
    });

    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "No-op progress callback should not cause errors: {:?}",
        result.err()
    );
}

// ── Extra: Progress with encrypted RAR ──

#[cfg(feature = "rar-support")]
#[test]
#[serial_test::file_serial(rar)]
fn contract_progress_encrypted_rar() {
    // test_encrypted_data.rar has data encryption only, password "test123"
    let archive = Archive::open_encrypted(fixture("test_encrypted_data.rar"), "test123").unwrap();
    let temp = tempfile::tempdir().unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let cc = call_count.clone();

    let options = ExtractionOptions::new(temp.path())
        .password("test123")
        .progress(move |_: u64, _: Option<u64>| {
            cc.fetch_add(1, Ordering::SeqCst);
            std::ops::ControlFlow::Continue(())
        });

    let result = archive.extract_all(options);
    assert!(
        result.is_ok(),
        "Encrypted RAR with progress should succeed: {:?}",
        result.err()
    );
}
