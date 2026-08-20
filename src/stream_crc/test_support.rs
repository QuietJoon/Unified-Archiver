//! Fixture helper shared by the module's unit tests.

/// Write `bytes` to a uniquely named file in the process temp directory
/// and return its path.
///
/// Each caller passes a distinct `name` so concurrent tests never share
/// a fixture, and every test removes its own file when it finishes.
pub(super) fn write_fixture(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, bytes).expect("write stream_crc test fixture");
    path
}
