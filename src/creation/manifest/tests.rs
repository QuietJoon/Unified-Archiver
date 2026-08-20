//! Unit tests for the single-walk source manifest (OI-0080-005).
//!
//! The drift classes are exercised against [`ManifestEntry::verify_unchanged`]
//! directly: the write phase calls it immediately before each backend
//! emit, and driving it here pins *which* error each kind of change
//! produces without needing a real race. The end-to-end proof that the
//! write phase reports a source that changes mid-run lives in
//! `crate::creation::tests` (`test_recursive_add_reports_source_removed_mid_write`),
//! which mutates the tree from a progress callback.

use super::*;
use std::io::Write as _;

const OP: &str = crate::error::ops::ADD_DIRECTORY_RECURSIVE;

/// Build a manifest for `dir`, discarding the reservations (the
/// namespace side is covered by the facade tests).
fn manifest_of(dir: &Path) -> SourceManifest {
    // The output archive is a path outside the tree, so the
    // self-ingestion guard never fires.
    let output = dir.join("..").join("unrelated-output.zip");
    SourceManifest::build(dir, &output, OP, |_| Ok(())).expect("manifest build")
}

fn entry_paths(manifest: &SourceManifest) -> Vec<&str> {
    manifest
        .entries()
        .iter()
        .map(|e| e.archive_path.as_str())
        .collect()
}

fn find<'m>(manifest: &'m SourceManifest, archive_path: &str) -> &'m ManifestEntry {
    manifest
        .entries()
        .iter()
        .find(|e| e.archive_path == archive_path)
        .unwrap_or_else(|| {
            panic!(
                "manifest has no '{archive_path}': {:?}",
                entry_paths(manifest)
            )
        })
}

/// Seed `root/tree` with a nested tree and return the walk root.
fn seed(root: &Path) -> PathBuf {
    let tree = root.join("tree");
    std::fs::create_dir_all(tree.join("nonempty")).unwrap();
    std::fs::create_dir_all(tree.join("empty")).unwrap();
    std::fs::write(tree.join("a.txt"), b"alpha").unwrap();
    std::fs::write(tree.join("nonempty").join("b.txt"), b"bravo!!").unwrap();
    tree
}

#[test]
fn build_records_the_whole_tree_in_walk_order() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    // Pre-order, children sorted by name: the root precedes everything,
    // every directory precedes its contents, and *non-leaf* directories
    // are recorded too (that is what carries their metadata).
    assert_eq!(
        entry_paths(&manifest),
        vec![
            "tree",
            "tree/a.txt",
            "tree/empty",
            "tree/nonempty",
            "tree/nonempty/b.txt",
        ]
    );

    let a = find(&manifest, "tree/a.txt");
    assert_eq!(a.kind, ManifestKind::File);
    assert_eq!(a.size, 5);
    assert!(a.mtime.is_some(), "a regular file must carry an mtime");

    let root = find(&manifest, "tree");
    assert_eq!(root.kind, ManifestKind::Dir);
    assert_eq!(root.size, 0, "directories carry no length");
    #[cfg(unix)]
    assert!(root.mode.is_some(), "unix directories carry a mode");
}

#[test]
fn build_reserves_every_recorded_entry_exactly_once() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());

    let mut reserved: Vec<String> = Vec::new();
    let output = temp.path().join("out.zip");
    let manifest = SourceManifest::build(&tree, &output, OP, |entry| {
        reserved.push(entry.archive_path.clone());
        Ok(())
    })
    .unwrap();

    // One reservation per recorded entry, in the same order: the walk
    // that reserves *is* the walk that records.
    assert_eq!(reserved, entry_paths(&manifest));
}

#[test]
fn build_stops_at_the_first_rejected_reservation() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());

    let mut seen = 0usize;
    let output = temp.path().join("out.zip");
    let err = SourceManifest::build(&tree, &output, OP, |_| {
        seen += 1;
        if seen == 2 {
            return Err(ArchiveError::operation_blocked(OP, "reservation refused"));
        }
        Ok(())
    })
    .unwrap_err();

    assert!(
        err.to_string().contains("reservation refused"),
        "the reservation's own rejection must propagate unchanged, got: {err}"
    );
    assert_eq!(seen, 2, "the walk must stop at the rejection");
}

#[test]
fn verify_passes_on_an_untouched_tree() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);
    for entry in manifest.entries() {
        entry
            .verify_unchanged(OP)
            .unwrap_or_else(|e| panic!("unchanged '{}' reported drift: {e}", entry.archive_path));
    }
}

#[test]
fn verify_reports_a_vanished_file_as_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    std::fs::remove_file(tree.join("a.txt")).unwrap();
    let err = find(&manifest, "tree/a.txt")
        .verify_unchanged(OP)
        .unwrap_err();

    // Machine-distinguishable from every other drift class: a caller
    // restoring a backup matches the io kind, not the message.
    match &err {
        ArchiveError::Io { source, .. } => {
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound, "got: {err}");
        }
        other => panic!("expected an Io/NotFound for a vanished source, got: {other:?}"),
    }
    assert!(
        err.to_string().contains("disappeared"),
        "expected the disappearance to be named, got: {err}"
    );
}

#[test]
fn verify_reports_a_vanished_directory_as_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    std::fs::remove_dir(tree.join("empty")).unwrap();
    let err = find(&manifest, "tree/empty")
        .verify_unchanged(OP)
        .unwrap_err();
    match &err {
        ArchiveError::Io { source, .. } => {
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound, "got: {err}")
        }
        other => panic!("expected an Io/NotFound for a vanished directory, got: {other:?}"),
    }
}

#[test]
fn verify_reports_growth_separately_from_disappearance() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(tree.join("a.txt"))
        .unwrap();
    f.write_all(b" and then some").unwrap();
    drop(f);

    let err = find(&manifest, "tree/a.txt")
        .verify_unchanged(OP)
        .unwrap_err();
    match &err {
        ArchiveError::Corruption { details, .. } => {
            assert!(
                details.contains("grew") && details.contains("5 bytes") && details.contains("19"),
                "a grown source must name the direction and both byte counts, got: {details}"
            );
        }
        other => panic!("expected Corruption for a size change, got: {other:?}"),
    }
}

#[test]
fn verify_reports_truncation_as_a_shrink() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    std::fs::write(tree.join("nonempty").join("b.txt"), b"br").unwrap();
    let err = find(&manifest, "tree/nonempty/b.txt")
        .verify_unchanged(OP)
        .unwrap_err();
    match &err {
        ArchiveError::Corruption { details, .. } => assert!(
            details.contains("shrank") && details.contains("7 bytes") && details.contains("2"),
            "got: {details}"
        ),
        other => panic!("expected Corruption for a size change, got: {other:?}"),
    }
}

#[test]
fn verify_reports_an_in_place_rewrite_at_the_same_size() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);
    let recorded = find(&manifest, "tree/a.txt");

    // Same length, different bytes — only the timestamp betrays it, so
    // this is the case a size check alone would miss.
    std::fs::write(tree.join("a.txt"), b"ALPHA").unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(tree.join("a.txt"))
        .unwrap();
    file.set_modified(recorded.mtime.unwrap() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(file);

    let err = recorded.verify_unchanged(OP).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("modified in place"),
        "an in-place rewrite must be reported as such, not as a size change or a \
         disappearance, got: {msg}"
    );
}

#[test]
fn verify_reports_a_file_replaced_by_a_directory() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    std::fs::remove_file(tree.join("a.txt")).unwrap();
    std::fs::create_dir(tree.join("a.txt")).unwrap();

    let err = find(&manifest, "tree/a.txt")
        .verify_unchanged(OP)
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("was replaced") && msg.contains("different kind"),
        "got: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn verify_reports_a_swap_that_preserves_size_and_mtime() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);
    let recorded = find(&manifest, "tree/a.txt");

    // The replacement is byte-identical in length and carries the same
    // timestamp; only the inode differs. Without the identity check the
    // writer would archive a file the walk never observed.
    let decoy = temp.path().join("decoy");
    std::fs::write(&decoy, b"OTHER").unwrap();
    let decoy_file = std::fs::OpenOptions::new()
        .write(true)
        .open(&decoy)
        .unwrap();
    decoy_file.set_modified(recorded.mtime.unwrap()).unwrap();
    drop(decoy_file);
    std::fs::rename(&decoy, tree.join("a.txt")).unwrap();

    let err = recorded.verify_unchanged(OP).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("was replaced") && msg.contains("different filesystem object"),
        "an inode swap must be reported as a replacement, got: {msg}"
    );
}

#[cfg(unix)]
#[test]
fn verify_reports_a_file_replaced_by_a_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let manifest = manifest_of(&tree);

    // A symlink planted where a recorded regular file was must not be
    // followed: `verify_unchanged` stats the link itself.
    std::fs::remove_file(tree.join("a.txt")).unwrap();
    std::os::unix::fs::symlink(tree.join("nonempty").join("b.txt"), tree.join("a.txt")).unwrap();

    let err = find(&manifest, "tree/a.txt")
        .verify_unchanged(OP)
        .unwrap_err();
    assert!(err.to_string().contains("was replaced"), "got: {err}");
}

#[test]
fn build_rejects_the_output_archive_inside_the_tree() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    let output = tree.join("out.zip");
    std::fs::write(&output, b"pretend archive").unwrap();

    let err = SourceManifest::build(&tree, &output, OP, |_| Ok(())).unwrap_err();
    assert!(
        err.to_string().contains("self-ingestion"),
        "the walk must still reject the destination archive, got: {err}"
    );
}

#[cfg(unix)]
#[test]
fn build_rejects_a_symlink_in_the_tree() {
    let temp = tempfile::tempdir().unwrap();
    let tree = seed(temp.path());
    std::os::unix::fs::symlink(tree.join("a.txt"), tree.join("link.txt")).unwrap();

    let output = temp.path().join("out.zip");
    let err = SourceManifest::build(&tree, &output, OP, |_| Ok(())).unwrap_err();
    assert!(err.to_string().contains("symlink"), "got: {err}");
}
