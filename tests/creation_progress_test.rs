//! Phase B.1 — `CompressionOptions.progress` wiring (OI-025-003)
//!
//! Verifies that the progress callback is invoked once per added entry and
//! that returning `ControlFlow::Break` cancels creation with a useful error.

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};
use unified_archive::{Archive, CompressionOptions, WritableFormat};

#[derive(Default)]
struct Recorder {
    invocations: Vec<u64>,
}

impl Recorder {
    fn callback(this: Arc<Mutex<Self>>) -> Box<dyn unified_archive::ProgressCallback> {
        Box::new(
            move |processed: u64, _total: Option<u64>| -> ControlFlow<()> {
                this.lock().unwrap().invocations.push(processed);
                ControlFlow::Continue(())
            },
        )
    }
}

#[test]
fn progress_callback_invoked_once_per_entry_zip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("progress.zip");

    let recorder = Arc::new(Mutex::new(Recorder::default()));
    let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
    options.progress = Some(Recorder::callback(Arc::clone(&recorder)));

    let mut archive = Archive::create(&path, options).unwrap();
    for i in 0..10 {
        archive
            .add_file_from_data(&format!("file{i}.bin"), &[0xAB; 64])
            .unwrap();
    }
    archive.finish().unwrap();

    let invocations = recorder.lock().unwrap().invocations.clone();
    assert_eq!(invocations.len(), 10, "one callback per added entry");
    // bytes_written must be monotonically non-decreasing
    assert!(invocations.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(*invocations.last().unwrap(), 10 * 64);
}

#[cfg(feature = "libarchive")]
#[test]
fn progress_callback_invoked_once_per_entry_tar() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("progress.tar");

    let recorder = Arc::new(Mutex::new(Recorder::default()));
    let mut options = CompressionOptions::for_writable(WritableFormat::TAR);
    options.progress = Some(Recorder::callback(Arc::clone(&recorder)));

    let mut archive = Archive::create(&path, options).unwrap();
    for i in 0..5 {
        archive
            .add_file_from_data(&format!("file{i}.bin"), &[0xCD; 32])
            .unwrap();
    }
    archive.finish().unwrap();

    let invocations = recorder.lock().unwrap().invocations.clone();
    assert_eq!(
        invocations.len(),
        5,
        "one callback per added entry (TAR via libarchive)"
    );
    assert_eq!(*invocations.last().unwrap(), 5 * 32);
}

#[test]
fn progress_callback_break_cancels_creation_zip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cancelled.zip");

    let cutoff = 3usize;
    let count = Arc::new(Mutex::new(0usize));
    let count_clone = Arc::clone(&count);

    let mut options = CompressionOptions::for_writable(WritableFormat::ZIP);
    options.progress = Some(Box::new(
        move |_processed: u64, _total: Option<u64>| -> ControlFlow<()> {
            let mut c = count_clone.lock().unwrap();
            *c += 1;
            if *c >= cutoff {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    ));

    let mut archive = Archive::create(&path, options).unwrap();
    let mut last_err = None;
    for i in 0..10 {
        if let Err(e) = archive.add_file_from_data(&format!("f{i}"), b"data") {
            last_err = Some(e);
            break;
        }
    }

    let err = last_err.expect("creation must surface cancellation as error");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("cancel"),
        "error message should mention cancellation, got: {msg}"
    );
    assert_eq!(*count.lock().unwrap(), cutoff);
}
