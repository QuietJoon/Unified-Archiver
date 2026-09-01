//! Common test utilities and helpers
//!
//! Provides shared infrastructure for integration and unit tests. The
//! `default_compression_options` and `default_extraction_options` helpers
//! exist so individual tests do not have to restate the library's own
//! defaults — chain the setters for the fields that differ.

pub mod config;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use unified_archive::{ArchiveFormat, CompressionOptions, ExtractionOptions};

/// Create unique temporary directory for testing
///
/// Uses configured temp directory (see .unified-archive.toml for development).
/// Falls back to system temp dir if configured path is not available. The
/// path mixes pid + nanos + a process-wide atomic counter so two tests
/// scheduled within the same nanosecond cannot collide on the directory
/// name (the prior pid+nanos scheme flaked under `cargo test`'s parallel
/// scheduler).
pub fn temp_test_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("test_{}_{}_{}", std::process::id(), timestamp, counter);

    // Use test config to get temp directory
    let base = config::temp_dir().join("7zip");
    let path = if base.exists() || fs::create_dir_all(&base).is_ok() {
        base.join(&suffix)
    } else {
        // Fallback to system temp
        std::env::temp_dir().join(format!("unified-archive-{}", suffix))
    };

    fs::create_dir_all(&path).expect("Failed to create temp directory");
    path
}

/// Cleanup temporary directory
///
/// Silently ignores errors (directory may already be removed)
pub fn cleanup(path: &PathBuf) {
    fs::remove_dir_all(path).ok();
}

/// Default `CompressionOptions` for tests — Normal level, no encryption, no
/// splitting, no progress callback.
///
/// Goes through the checked constructor, so a non-creatable format panics
/// here instead of building options whose only possible outcome is an error
/// at the create call. Every caller passes Zip or SevenZip.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn default_compression_options(format: ArchiveFormat) -> CompressionOptions {
    CompressionOptions::try_new(format).expect("test helper is only called with creatable formats")
}

/// Default `ExtractionOptions` for tests — library defaults with the given
/// destination.
///
/// Now a thin delegate to [`ExtractionOptions::new`], which does exactly
/// this. It is kept because 54 call sites across 17 test files name it, and
/// churning all of them belongs to a cleanup commit rather than to the
/// breaking change that made the constructor necessary.
///
/// Override further fields by chaining the setters —
/// `default_extraction_options(dest).overwrite(true)`. The struct-update
/// form this helper used to recommend
/// (`ExtractionOptions { overwrite: true, ..default_extraction_options(dest) }`)
/// no longer compiles: `ExtractionOptions` is `#[non_exhaustive]`, and the
/// `..base` syntax is refused outside the defining crate whatever `base` is.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn default_extraction_options(dest: impl Into<PathBuf>) -> ExtractionOptions {
    ExtractionOptions::new(dest)
}

/// Get fixture path for test files
///
/// Returns absolute path to tests/fixtures/{name}
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Check whether the given executable is available on PATH. Tests use this
/// to skip gracefully when a fixture-building CLI (tar, zip, 7z…) is
/// missing from the environment.
///
/// R0001-0086: this used to launch `which`, which Windows does not
/// normally provide — so every tool-backed lane reported a graceful skip
/// there even when the tool was installed and usable. Resolve `PATH` in
/// Rust instead (honouring `PATHEXT` on Windows) so discovery behaves the
/// same on every host and needs no child process at all.
#[allow(dead_code)]
pub fn command_exists(cmd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }

    // A name that already carries a separator (`./tool`, `/usr/bin/tar`)
    // is not a PATH lookup — probe it directly, as the shell would.
    let explicit = Path::new(cmd);
    if explicit.components().count() > 1 {
        return is_executable_file(explicit);
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    let extensions = executable_extensions();
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if is_executable_file(&dir.join(cmd)) {
            return true;
        }
        for ext in &extensions {
            if is_executable_file(&dir.join(format!("{cmd}{ext}"))) {
                return true;
            }
        }
    }

    false
}

/// Extensions Windows appends to a bare command name when resolving it
/// through `PATH` (R0001-0086). Values are normalized to carry a leading
/// dot so callers can concatenate them onto the command name.
#[cfg(windows)]
#[allow(dead_code)]
fn executable_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .map(str::trim)
        .filter(|ext| !ext.is_empty())
        .map(|ext| {
            if ext.starts_with('.') {
                ext.to_string()
            } else {
                format!(".{ext}")
            }
        })
        .collect()
}

/// Unix resolves the command name verbatim — nothing to append.
#[cfg(not(windows))]
#[allow(dead_code)]
fn executable_extensions() -> Vec<String> {
    Vec::new()
}

/// A `PATH` candidate counts only when it is a regular file carrying at
/// least one execute bit (R0001-0086).
#[cfg(unix)]
#[allow(dead_code)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Windows has no execute bit — `PATHEXT` already decides what is
/// runnable, so a regular file at a candidate name is enough.
#[cfg(not(unix))]
#[allow(dead_code)]
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Library-native fixture builders (OI-0056-010)
//
// These used to shell out to the `tar`, `ln` and `zip` CLIs and return
// `false` when a binary was missing, which let every consuming lane report
// success having asserted nothing. The TAR side is now a ~40-line ustar
// writer and the ZIP side goes through the `zip` dev-dependency, so no lane
// depends on a host binary any more and none of them can skip.
// ---------------------------------------------------------------------------

/// One member of a hand-assembled ustar archive.
///
/// `typeflag` follows POSIX.1-1988: `b'0'` regular file, `b'1'` hard link,
/// `b'2'` symbolic link, `b'5'` directory. `link` is the `linkname` field
/// and is only meaningful for the two link types.
#[allow(dead_code)]
pub struct TarMember<'a> {
    pub name: &'a str,
    pub typeflag: u8,
    pub link: &'a str,
    pub data: &'a [u8],
    pub mode: u32,
}

#[allow(dead_code)]
impl<'a> TarMember<'a> {
    pub fn file(name: &'a str, data: &'a [u8]) -> Self {
        Self {
            name,
            typeflag: b'0',
            link: "",
            data,
            mode: 0o644,
        }
    }

    pub fn dir(name: &'a str) -> Self {
        Self {
            name,
            typeflag: b'5',
            link: "",
            data: &[],
            mode: 0o755,
        }
    }

    pub fn symlink(name: &'a str, target: &'a str) -> Self {
        Self {
            name,
            typeflag: b'2',
            link: target,
            data: &[],
            mode: 0o777,
        }
    }

    pub fn hardlink(name: &'a str, target: &'a str) -> Self {
        Self {
            name,
            typeflag: b'1',
            link: target,
            data: &[],
            mode: 0o644,
        }
    }
}

/// Write a ustar archive containing `members`, in order, to `path`.
///
/// Deliberately byte-level rather than CLI-driven: it works on every host,
/// it can emit shapes no portable CLI invocation reaches (a duplicated
/// member name, a link record with no filesystem link behind it), and the
/// resulting fixture is identical everywhere, so a lane that depends on it
/// can neither skip nor drift between hosts.
#[allow(dead_code)]
pub fn write_tar(path: &Path, members: &[TarMember<'_>]) {
    fn octal(buf: &mut [u8], value: u64) {
        // POSIX wants `width - 1` octal digits followed by NUL.
        let digits = buf.len() - 1;
        let text = format!("{value:0digits$o}");
        assert!(
            text.len() == digits,
            "value {value} does not fit in {digits} octal digits"
        );
        buf[..digits].copy_from_slice(text.as_bytes());
        buf[digits] = 0;
    }

    let mut out: Vec<u8> = Vec::new();
    for m in members {
        assert!(m.name.len() < 100, "fixture name too long: {}", m.name);
        assert!(m.link.len() < 100, "fixture link too long: {}", m.link);

        let mut header = [0u8; 512];
        header[..m.name.len()].copy_from_slice(m.name.as_bytes());
        octal(&mut header[100..108], m.mode as u64);
        octal(&mut header[108..116], 0); // uid
        octal(&mut header[116..124], 0); // gid
        octal(&mut header[124..136], m.data.len() as u64);
        octal(&mut header[136..148], 0); // mtime — fixed, so fixtures are reproducible
        header[148..156].fill(b' '); // checksum placeholder
        header[156] = m.typeflag;
        header[157..157 + m.link.len()].copy_from_slice(m.link.as_bytes());
        header[257..262].copy_from_slice(b"ustar");
        header[262] = 0;
        header[263..265].copy_from_slice(b"00");

        let sum: u64 = header.iter().map(|b| u64::from(*b)).sum();
        // The checksum field is 6 octal digits, NUL, space.
        let text = format!("{sum:06o}");
        header[148..154].copy_from_slice(text.as_bytes());
        header[154] = 0;
        header[155] = b' ';

        out.extend_from_slice(&header);
        out.extend_from_slice(m.data);
        let rem = m.data.len() % 512;
        if rem != 0 {
            out.extend(std::iter::repeat_n(0u8, 512 - rem));
        }
    }
    // Two zero blocks close the archive.
    out.extend(std::iter::repeat_n(0u8, 1024));
    fs::write(path, &out).expect("write tar fixture");
}

/// TAR carrying `regular.txt` plus a `link.txt -> regular.txt` symlink
/// record. Infallible: no host binary is involved.
#[allow(dead_code)]
pub fn build_tar_with_symlink(archive_path: &Path) {
    write_tar(
        archive_path,
        &[
            TarMember::file("regular.txt", b"hello from a regular file\n"),
            TarMember::symlink("link.txt", "regular.txt"),
        ],
    );
}

/// TAR carrying `regular.txt` plus a `hardlink.txt` hard-link record
/// pointing at it. Infallible: no host binary is involved.
#[allow(dead_code)]
pub fn build_tar_with_hardlink(archive_path: &Path) {
    write_tar(
        archive_path,
        &[
            TarMember::file("regular.txt", b"hello from a regular file\n"),
            TarMember::hardlink("hardlink.txt", "regular.txt"),
        ],
    );
}

/// One member of a ZIP fixture built through the `zip` dev-dependency.
#[allow(dead_code)]
pub enum ZipMember<'a> {
    /// A regular file entry.
    File(&'a str, &'a [u8]),
    /// A trailing-slash directory entry carrying the MS-DOS directory
    /// attribute — the shape the `zip` crate reports as `is_dir()`.
    Dir(&'a str),
    /// An S_IFLNK-flagged entry whose payload is the link target.
    Symlink(&'a str, &'a str),
}

/// Build a ZIP at `path` from `members`.
///
/// `stored` selects STORE over DEFLATE, which is how the CLI's `-0` used to
/// be expressed. Replaces the `zip`/`zip --symlinks` CLI calls the fixture
/// lanes used to skip on (OI-0056-010).
#[allow(dead_code)]
pub fn build_zip(path: &Path, members: &[ZipMember<'_>], stored: bool) {
    use std::io::Write as _;

    let file = fs::File::create(path).expect("create zip fixture");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(if stored {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    });

    for m in members {
        match m {
            ZipMember::File(name, data) => {
                writer.start_file(*name, options).expect("start zip entry");
                writer.write_all(data).expect("write zip entry");
            }
            ZipMember::Dir(name) => {
                writer.add_directory(*name, options).expect("zip directory");
            }
            ZipMember::Symlink(name, target) => {
                writer
                    .add_symlink(*name, *target, options)
                    .expect("zip symlink");
            }
        }
    }
    writer.finish().expect("finish zip fixture");
}

/// ZIP carrying `regular.txt` plus a `link.txt -> regular.txt` symlink
/// entry, built in-process.
///
/// The old CLI-backed builder warned that "writing a real ZIP symlink from
/// pure Rust is fragile because many zip writers mask file-type bits out of
/// `unix_permissions`". `ZipWriter::add_symlink` does set S_IFLNK, and
/// `build_zip_with_symlink_checked` proves it against this crate's own
/// reader rather than trusting it, so the fragility is now asserted instead
/// of routed around.
#[allow(dead_code)]
pub fn build_zip_with_symlink(archive_path: &Path) {
    build_zip(
        archive_path,
        &[
            ZipMember::File("regular.txt", b"hello from a regular file\n"),
            ZipMember::Symlink("link.txt", "regular.txt"),
        ],
        true,
    );
}

/// ZIP carrying `regular.txt` plus a `subdir/` directory entry.
#[allow(dead_code)]
pub fn build_zip_with_dir(archive_path: &Path) {
    build_zip(
        archive_path,
        &[
            ZipMember::File("regular.txt", b"hello\n"),
            ZipMember::Dir("subdir/"),
        ],
        true,
    );
}

/// Build a single-entry ZIP carrying a 0x5455 ExtendedTimestamp extra
/// field with the supplied modification, access, and creation times.
/// Shared between OI-0065-002 round-trip tests on both the integration
/// suite and the standalone modification-options binary.
///
/// The 0x5455 wire format encodes each timestamp as a signed 32-bit
/// Unix-seconds value, so callers must stay below 2038-01-19T03:14:07Z.
#[allow(dead_code)] // Not every test binary that includes common/mod.rs uses this.
pub fn build_zip_with_extended_timestamp(
    path: &Path,
    mtime: SystemTime,
    atime: SystemTime,
    btime: SystemTime,
) {
    use std::fs::File;
    use std::io::Write as _;
    use zip::CompressionMethod;
    use zip::write::{FullFileOptions, ZipWriter};

    let secs = |t: SystemTime| -> i32 { t.duration_since(UNIX_EPOCH).unwrap().as_secs() as i32 };

    let mut payload = Vec::with_capacity(13);
    // Flags byte: bit0=mtime, bit1=atime, bit2=ctime — set all three.
    payload.push(0b0000_0111);
    payload.extend_from_slice(&secs(mtime).to_le_bytes());
    payload.extend_from_slice(&secs(atime).to_le_bytes());
    payload.extend_from_slice(&secs(btime).to_le_bytes());

    let file = File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    let mut opts = FullFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    opts.add_extra_data(0x5455, payload.into_boxed_slice(), false)
        .unwrap();
    writer.start_file("ts.txt", opts).unwrap();
    writer.write_all(b"hello world").unwrap();
    writer.finish().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_unique() {
        let dir1 = temp_test_dir();
        let dir2 = temp_test_dir();

        assert_ne!(dir1, dir2, "Each temp dir should be unique");
        assert!(dir1.exists(), "Temp dir should be created");
        assert!(dir2.exists(), "Temp dir should be created");

        cleanup(&dir1);
        cleanup(&dir2);
    }

    #[test]
    fn test_fixture_path() {
        let path = fixture("test.rar");
        assert!(path.to_string_lossy().contains("tests/fixtures/test.rar"));
    }

    /// R0001-0086: the PATH walk must reject names that are not on PATH
    /// on every host, without depending on a `which`/`where` binary.
    #[test]
    fn test_command_exists_rejects_unknown_and_empty_names() {
        assert!(!command_exists(""), "empty name is never a command");
        assert!(
            !command_exists("unified-archive-definitely-not-installed-xyzzy"),
            "a name that is not on PATH must not resolve"
        );
    }

    /// R0001-0086: a name carrying a separator is probed directly, and only
    /// a regular file with an execute bit counts. The probes use system
    /// paths with fixed modes on every supported Unix host, so the oracle
    /// does not depend on the scratch filesystem honouring `chmod`.
    #[cfg(unix)]
    #[test]
    fn test_command_exists_honours_explicit_paths() {
        assert!(
            command_exists("/bin/sh"),
            "/bin/sh (0755) must resolve through the explicit-path probe"
        );
        assert!(
            !command_exists("/etc/hosts"),
            "/etc/hosts (0644) is a regular file with no execute bit"
        );
        assert!(
            !command_exists("/etc"),
            "a directory must never resolve as a command"
        );
        assert!(
            !command_exists("/nonexistent/path/to/tool"),
            "a missing explicit path must not resolve"
        );
    }
}
