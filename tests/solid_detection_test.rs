//! Tests for solid archive detection and recovery records
//!
//! Verifies that is_solid() and has_recovery_record() methods work correctly across different formats

use unified_archive::Archive;

#[test]
#[serial_test::file_serial(rar)]
fn test_is_solid_rar() {
    // Test RAR archive (may or may not be solid, just verify method works)
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
    let result = archive.is_solid();
    assert!(result.is_ok(), "is_solid() should succeed for RAR archives");
}

#[test]
#[serial_test::file_serial(rar)]
fn test_is_solid_rar5() {
    // Test RAR5 archive
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");
    let result = archive.is_solid();
    assert!(
        result.is_ok(),
        "is_solid() should succeed for RAR5 archives"
    );
}

#[test]
fn test_is_solid_zip() {
    // ZIP archives are never solid
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");
    let is_solid = archive.is_solid().expect("is_solid() should succeed");
    assert!(!is_solid, "ZIP archives should never be solid");
}

#[test]
fn test_is_solid_7z() {
    // 7z archive - check actual solid compression detection
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");
    let is_solid = archive
        .is_solid()
        .expect("is_solid() should succeed for 7z archives");

    // The result depends on how the test fixture was created
    // Just verify the method works and returns a boolean
    println!("7z archive solid status: {}", is_solid);
}

// Recovery record tests
#[test]
#[serial_test::file_serial(rar)]
fn test_has_recovery_rar() {
    // Test RAR archive (may or may not have recovery, just verify method works)
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
    let result = archive.has_recovery_record();
    assert!(
        result.is_ok(),
        "has_recovery_record() should succeed for RAR archives"
    );
}

#[test]
#[serial_test::file_serial(rar)]
fn test_has_recovery_rar5() {
    // Test RAR5 archive
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");
    let result = archive.has_recovery_record();
    assert!(
        result.is_ok(),
        "has_recovery_record() should succeed for RAR5 archives"
    );
}

#[test]
fn test_has_recovery_zip() {
    // ZIP archives don't support recovery records
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");
    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record() should succeed");
    assert!(
        !has_recovery,
        "ZIP archives should not have recovery records"
    );
}

#[test]
fn test_has_recovery_7z() {
    // 7z archives don't support recovery records
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");
    let has_recovery = archive
        .has_recovery_record()
        .expect("has_recovery_record() should succeed");
    assert!(
        !has_recovery,
        "7z archives should not have recovery records"
    );
}

// Recovery percentage tests
#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_rar() {
    // Test RAR archive - may or may not have recovery records
    let archive = Archive::open("tests/fixtures/test.rar").expect("Failed to open RAR archive");
    let result = archive.recovery_percentage();
    assert!(
        result.is_ok(),
        "recovery_percentage() should succeed for RAR archives"
    );

    // If recovery exists, percentage should be 1-100%
    if let Ok(Some(pct)) = result {
        assert!(
            (1..=100).contains(&pct),
            "Recovery percentage should be 1-100%, got {}",
            pct
        );
        println!("RAR archive has {}% recovery", pct);
    }
}

#[test]
#[serial_test::file_serial(rar)]
fn test_recovery_percentage_rar5() {
    // Test RAR5 archive - may or may not have recovery records
    let archive =
        Archive::open("tests/fixtures/test_rar5.rar").expect("Failed to open RAR5 archive");
    let result = archive.recovery_percentage();
    assert!(
        result.is_ok(),
        "recovery_percentage() should succeed for RAR5 archives"
    );

    // If recovery exists, percentage should be 1-100%
    if let Ok(Some(pct)) = result {
        assert!(
            (1..=100).contains(&pct),
            "Recovery percentage should be 1-100%, got {}",
            pct
        );
        println!("RAR5 archive has {}% recovery", pct);
    }
}

#[test]
fn test_recovery_percentage_zip() {
    // ZIP archives don't support recovery records
    let archive = Archive::open("tests/fixtures/test.zip").expect("Failed to open ZIP archive");
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage() should succeed");
    assert_eq!(
        pct, None,
        "ZIP archives should return None for recovery percentage"
    );
}

#[test]
fn test_recovery_percentage_7z() {
    // 7z archives don't support recovery records
    let archive = Archive::open("tests/fixtures/test.7z").expect("Failed to open 7z archive");
    let pct = archive
        .recovery_percentage()
        .expect("recovery_percentage() should succeed");
    assert_eq!(
        pct, None,
        "7z archives should return None for recovery percentage"
    );
}
