//! `add_file_from_path` must refuse a symlink source, on every writer.
//!
//! R0069-0054 / R0069-0055 set the policy: archiving a symlink by name would
//! silently store the *target's* bytes under the *link's* name, which is a
//! content substitution the caller never asked for.
//!
//! **What these tests actually pin.** The refusal a caller meets comes from
//! `creation::validate_file_path`, the facade-level gate — not from the
//! backend writers. That was established by injection: deleting *both* checks
//! inside `ffi::common::open_file_no_follow_symlinks` leaves all three of
//! these green, because the facade already refused. So these are tests of the
//! facade policy, and they are worth having because that policy had **no**
//! coverage at all — the refusal message occurs three times in `src/` and
//! nothing asserted any of them.
//!
//! The backend layer is defence in depth behind this gate and is exercised
//! separately, by the unit tests on `open_file_no_follow_symlinks` itself in
//! `src/ffi/common.rs`, which an integration test cannot reach for a plain
//! symlink.
//!
//! Unix-only: the whole point is a real `symlink(2)`, not a simulation.

#![cfg(unix)]

use std::os::unix::fs::symlink;

#[cfg_attr(
    not(any(feature = "create", feature = "modify")),
    allow(unused_imports)
)]
use unified_archive::{Archive, CompressionOptions, WritableFormat};

#[cfg_attr(not(any(feature = "create", feature = "modify")), allow(dead_code))]
/// A real file, and a real symlink pointing at it.
fn target_and_link(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let target = dir.join("real.txt");
    std::fs::write(&target, b"the target's bytes").expect("write target");
    let link = dir.join("link.txt");
    symlink(&target, &link).expect("create symlink");
    (target, link)
}

#[cfg(feature = "create")]
/// ZIP creation path. The refusal arrives from the facade gate; the ZIP
/// writer's own check never runs for this input.
#[test]
fn zip_add_file_from_path_refuses_a_symlink_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let (_target, link) = target_and_link(temp.path());

    let mut archive = Archive::create(
        temp.path().join("out.zip"),
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .expect("create zip");

    let err = archive
        .add_file_from_path(&link)
        .expect_err("a symlink source must be refused, not silently dereferenced");
    let text = err.to_string();
    assert!(
        text.contains("symlink"),
        "the refusal must name the reason; got: {text}"
    );
}

#[cfg(feature = "create")]
/// TAR creation path — the same policy has to hold whichever format the
/// caller picked, or the guarantee depends on a choice they made for
/// unrelated reasons.
#[cfg(feature = "libarchive")]
#[test]
fn tar_add_file_from_path_refuses_a_symlink_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let (_target, link) = target_and_link(temp.path());

    let mut archive = Archive::create(
        temp.path().join("out.tar"),
        CompressionOptions::for_writable(WritableFormat::TAR),
    )
    .expect("create tar");

    let err = archive
        .add_file_from_path(&link)
        .expect_err("a symlink source must be refused, not silently dereferenced");
    let text = err.to_string();
    assert!(
        text.contains("symlink"),
        "the refusal must name the reason; got: {text}"
    );
}

#[cfg(feature = "create")]
/// The control: the same call on the real file succeeds. Without this, both
/// tests above would still pass if `add_file_from_path` were broken outright.
#[test]
fn a_regular_file_is_still_accepted() {
    let temp = tempfile::tempdir().expect("temp dir");
    let (target, _link) = target_and_link(temp.path());

    let mut archive = Archive::create(
        temp.path().join("ok.zip"),
        CompressionOptions::for_writable(WritableFormat::ZIP),
    )
    .expect("create zip");

    archive
        .add_file_from_path(&target)
        .expect("a regular file must still be archivable");
    archive.finish().expect("finish");
}
