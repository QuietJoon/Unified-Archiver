//! Thread-safety tests (FR-020, FR-021, T120/T120a)
//!
//! Verifies that unified-archive supports:
//! - FR-020: Thread-safe APIs for concurrent archive operations on different files
//! - FR-021: Multiple archive handles used concurrently without blocking

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use unified_archive::ExtractionOptions;

/// Create a minimal valid ZIP file for testing
fn create_test_zip(path: &Path, content: &str) -> std::io::Result<()> {
    // Use the zip crate if available, otherwise create minimal ZIP structure
    // For simplicity, we'll create a text file and use system zip if available
    let temp_dir = tempfile::tempdir()?;
    let temp_file = temp_dir.path().join("test.txt");
    fs::write(&temp_file, content)?;

    let output = std::process::Command::new("zip")
        .args(["-j"]) // junk paths
        .arg(path)
        .arg(&temp_file)
        .output()?;

    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to create ZIP",
        ));
    }

    Ok(())
}

/// Check if zip command is available
fn zip_available() -> bool {
    std::process::Command::new("which")
        .arg("zip")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_concurrent_archive_open() {
    // T120a: Test concurrent Archive::open() calls on different files
    if !zip_available() {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create multiple test archives
    let num_archives = 8;
    let archive_paths: Vec<_> = (0..num_archives)
        .map(|i| {
            let path = temp_dir.path().join(format!("test_{}.zip", i));
            create_test_zip(&path, &format!("Content for archive {}", i))
                .expect("Failed to create test ZIP");
            path
        })
        .collect();

    // Track successful opens
    let success_count = Arc::new(AtomicUsize::new(0));

    // Open all archives concurrently from multiple threads
    let handles: Vec<_> = archive_paths
        .iter()
        .map(|path| {
            let path = path.clone();
            let counter = Arc::clone(&success_count);
            thread::spawn(move || {
                match unified_archive::Archive::open(&path) {
                    Ok(archive) => {
                        // Verify we can list files
                        if archive.list_files().is_ok() {
                            counter.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to open {:?}: {}", path, e);
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // All opens should succeed
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        num_archives,
        "All concurrent opens should succeed"
    );
}

#[test]
fn test_concurrent_list_files() {
    // Test that multiple threads can list files from different archives concurrently
    if !zip_available() {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create test archives
    let num_archives = 4;
    let archives: Vec<_> = (0..num_archives)
        .map(|i| {
            let path = temp_dir.path().join(format!("list_test_{}.zip", i));
            create_test_zip(&path, &format!("File content {}", i)).expect("Failed to create ZIP");
            unified_archive::Archive::open(&path).expect("Failed to open archive")
        })
        .collect();

    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    // List files from all archives concurrently
    let handles: Vec<_> = archives
        .into_iter()
        .enumerate()
        .map(|(i, archive)| {
            let results = Arc::clone(&results);
            thread::spawn(move || {
                let entries = archive.list_files().expect("Should list files");
                results.lock().unwrap().push((i, entries.len()));
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_results = results.lock().unwrap();
    assert_eq!(
        final_results.len(),
        num_archives,
        "All list operations should complete"
    );
}

#[test]
fn test_concurrent_extraction_different_archives() {
    // Test extracting from different archives concurrently
    if !zip_available() {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create test archives
    let num_archives = 4;
    let archive_data: Vec<_> = (0..num_archives)
        .map(|i| {
            let archive_path = temp_dir.path().join(format!("extract_test_{}.zip", i));
            let extract_path = temp_dir.path().join(format!("output_{}", i));
            fs::create_dir_all(&extract_path).expect("Failed to create output dir");

            create_test_zip(&archive_path, &format!("Extract content {}", i))
                .expect("Failed to create ZIP");

            (archive_path, extract_path)
        })
        .collect();

    let success_count = Arc::new(AtomicUsize::new(0));

    // Extract all archives concurrently
    let handles: Vec<_> = archive_data
        .into_iter()
        .map(|(archive_path, extract_path)| {
            let counter = Arc::clone(&success_count);
            thread::spawn(move || {
                let archive =
                    unified_archive::Archive::open(&archive_path).expect("Should open archive");
                let options = ExtractionOptions {
                    destination: extract_path.clone(),
                    ..Default::default()
                };
                if archive.extract_all(options).is_ok() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert_eq!(
        success_count.load(Ordering::SeqCst),
        num_archives,
        "All concurrent extractions should succeed"
    );
}

#[test]
fn test_no_data_races_on_repeated_access() {
    // Test that repeated concurrent access doesn't cause data races
    if !zip_available() {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let archive_path = temp_dir.path().join("race_test.zip");
    create_test_zip(&archive_path, "Race test content").expect("Failed to create ZIP");

    let archive_path = Arc::new(archive_path);
    let iteration_count = Arc::new(AtomicUsize::new(0));

    // Multiple threads opening and listing the same archive path
    // (different Archive instances)
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = Arc::clone(&archive_path);
            let counter = Arc::clone(&iteration_count);
            thread::spawn(move || {
                for _ in 0..10 {
                    let archive =
                        unified_archive::Archive::open(path.as_ref()).expect("Should open");
                    let _ = archive.list_files();
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // All 40 iterations (4 threads * 10 iterations) should complete
    assert_eq!(iteration_count.load(Ordering::SeqCst), 40);
}

#[test]
fn test_concurrent_performance_no_excessive_blocking() {
    // Test that concurrent operations don't block excessively
    if !zip_available() {
        eprintln!("Skipping test: zip command not available");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create test archives
    let num_archives = 4;
    let archive_paths: Vec<_> = (0..num_archives)
        .map(|i| {
            let path = temp_dir.path().join(format!("perf_test_{}.zip", i));
            create_test_zip(&path, &format!("Performance test content {}", i))
                .expect("Failed to create ZIP");
            path
        })
        .collect();

    let start = Instant::now();

    // Run operations concurrently
    let handles: Vec<_> = archive_paths
        .iter()
        .map(|path| {
            let path = path.clone();
            thread::spawn(move || {
                let archive = unified_archive::Archive::open(&path).expect("Should open");
                let _ = archive.list_files();
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let elapsed = start.elapsed();

    // Concurrent operations should complete in reasonable time
    // (much less than sequential would take)
    assert!(
        elapsed < Duration::from_secs(10),
        "Concurrent operations took too long: {:?}",
        elapsed
    );
}
