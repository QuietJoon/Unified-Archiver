//! `ExtractionLimits::max_sfx_payload_size` is enforced by the staging
//! copy (AD 0040), and the limits-free entry points still stage under
//! the documented default.
//!
//! The fixtures are deliberately not archives: the ceiling is checked
//! before a single byte is copied, so "blocked by the cap" is
//! distinguishable from "staged, then rejected as a non-archive" by the
//! error text alone — which is exactly the difference a caller who
//! lowered the cap is asking about.

use super::{Archive, DEFAULT_SFX_PAYLOAD_CAP};
use crate::error::ArchiveError;
use crate::security::{Cap, ExtractionLimits};

const OVER_CAP: &str = "exceeds maximum";

fn staging_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("payload.bin");
    std::fs::write(&path, vec![0x5Au8; 4096]).unwrap();
    path
}

fn limits_with_sfx_cap(cap: Cap) -> ExtractionLimits {
    ExtractionLimits::builder()
        .max_sfx_payload_size(cap)
        .build()
}

/// The constant the limits-free entry points pass is the field's
/// default — so wiring the caller's cap did not move the default.
#[test]
fn default_sfx_cap_matches_extraction_limits_default() {
    assert_eq!(
        DEFAULT_SFX_PAYLOAD_CAP,
        ExtractionLimits::default().max_sfx_payload_size(),
    );
}

#[test]
fn lowered_cap_blocks_staging() {
    let dir = tempfile::tempdir().unwrap();
    let path = staging_fixture(dir.path());
    // 4096 - 1024 = 3072 bytes of payload against a 1 KiB ceiling.
    let limits = limits_with_sfx_cap(Cap::Limited(1024));
    let err = match Archive::open_at_offset_with_limits(&path, 1024, &limits) {
        Ok(_) => panic!("a lowered max_sfx_payload_size must block staging"),
        Err(e) => e,
    };
    assert!(
        matches!(err, ArchiveError::Format { .. }),
        "expected Format, got: {err:?}",
    );
    let message = err.to_string();
    assert!(
        message.contains(OVER_CAP) && message.contains("3072"),
        "message must name the payload size and the ceiling, got: {message}",
    );
    assert!(
        message.contains("1024"),
        "message must report the caller's ceiling, not the default, got: {message}",
    );
}

#[test]
fn default_cap_still_stages_the_same_payload() {
    let dir = tempfile::tempdir().unwrap();
    let path = staging_fixture(dir.path());
    // Same payload, no caller limits: the copy runs and the staged
    // tempfile is then rejected as a non-archive — a different error
    // than the ceiling's, which is the point.
    let err = match Archive::open_at_offset(&path, 1024) {
        Ok(_) => panic!("a non-archive payload cannot open"),
        Err(e) => e,
    };
    assert!(
        !err.to_string().contains(OVER_CAP),
        "the default ceiling must not reject a 3 KiB payload, got: {err}",
    );
}

#[test]
fn unlimited_cap_never_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = staging_fixture(dir.path());
    let limits = limits_with_sfx_cap(Cap::Unlimited);
    let err = match Archive::open_at_offset_with_limits(&path, 1024, &limits) {
        Ok(_) => panic!("a non-archive payload cannot open"),
        Err(e) => e,
    };
    assert!(
        !err.to_string().contains(OVER_CAP),
        "Cap::Unlimited means no ceiling, not u64::MAX-as-ceiling, got: {err}",
    );
}

/// The SFX-detecting entry points thread the caller's cap too, not
/// only the raw-offset one.
#[test]
fn lowered_cap_blocks_sfx_entry_points() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("installer.sh");
    let mut sfx = b"#!/bin/sh\n".to_vec();
    sfx.extend(vec![0u8; 100]);
    let mut zip_header = [0u8; 30];
    zip_header[..4].copy_from_slice(b"PK\x03\x04");
    zip_header[8] = 8; // deflate
    sfx.extend_from_slice(&zip_header);
    sfx.extend(vec![0u8; 4096]);
    std::fs::write(&path, &sfx).unwrap();

    let detection = Archive::detect_sfx(&path).expect("detect");
    assert!(detection.is_sfx(), "fixture must screen as a probable SFX");

    let limits = limits_with_sfx_cap(Cap::Limited(16));
    for (label, result) in [
        (
            "open_sfx_with_limits",
            Archive::open_sfx_with_limits(&path, &limits),
        ),
        (
            "open_with_sfx_progress_and_limits",
            Archive::open_with_sfx_progress_and_limits(&path, None, &limits),
        ),
    ] {
        let err = match result {
            Ok(_) => panic!("{label} must honour the lowered ceiling"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains(OVER_CAP),
            "{label} must fail on the ceiling, got: {err}",
        );
    }
}
