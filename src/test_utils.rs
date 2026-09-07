//! Shared test utilities

use std::path::PathBuf;

/// Get path to a test fixture file
#[cfg(test)]
pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Build a single-entry ZIP ("tool.sh") whose central directory carries
/// Unix mode 0o755 and the fixed DOS-precision modification time
/// 2010-01-02 03:04:06. Shared by the zip-crate backend
/// metadata-preservation tests (R0079-0019).
#[cfg(all(test, unix))]
#[cfg_attr(not(feature = "modify"), allow(dead_code))]
pub(crate) fn build_zip_with_mode_and_mtime(path: &std::path::Path) {
    use std::io::Write as _;

    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755)
        .last_modified_time(zip::DateTime::from_date_and_time(2010, 1, 2, 3, 4, 6).unwrap());
    writer.start_file("tool.sh", options).unwrap();
    writer.write_all(b"#!/bin/sh\n").unwrap();
    writer.finish().unwrap();
}
