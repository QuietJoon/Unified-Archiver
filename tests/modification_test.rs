//! ZIP metadata preservation: archive comment and per-entry compression methods
//! survive `commit_changes`.

use unified_archive::Archive;

#[test]
fn test_commit_changes_preserves_archive_comment() {
    use std::io::Write;

    let tmp_dir = tempfile::tempdir().unwrap();
    let archive_path = tmp_dir.path().join("with_comment.zip");
    let comment = b"unified-archive roundtrip comment \xe2\x9c\x94";

    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("first.txt", opts).unwrap();
        writer.write_all(b"first content").unwrap();
        writer.start_file("second.txt", opts).unwrap();
        writer.write_all(b"second content").unwrap();
        writer.set_raw_comment(comment.to_vec().into_boxed_slice());
        writer.finish().unwrap();
    }

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive.add_entry("added.txt", b"added content").unwrap();
    archive.commit_changes().unwrap();

    let file = std::fs::File::open(&archive_path).unwrap();
    let zip = zip::ZipArchive::new(file).unwrap();
    assert_eq!(
        zip.comment(),
        comment,
        "archive comment must survive commit_changes"
    );
}

#[test]
fn test_commit_changes_preserves_per_entry_compression() {
    use std::io::{Read, Write};

    let tmp_dir = tempfile::tempdir().unwrap();
    let archive_path = tmp_dir.path().join("mixed_methods.zip");

    {
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let stored: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("plain.bin", stored).unwrap();
        writer
            .write_all(b"raw data that would compress but stays stored")
            .unwrap();
        writer.start_file("compressed.txt", deflated).unwrap();
        writer
            .write_all(b"content meant to be deflated content meant to be deflated content")
            .unwrap();
        writer.finish().unwrap();
    }

    let mut archive = Archive::modify(&archive_path).unwrap();
    archive
        .add_entry("fresh.txt", b"added after commit")
        .unwrap();
    archive.commit_changes().unwrap();

    let file = std::fs::File::open(&archive_path).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();

    let methods: std::collections::HashMap<String, zip::CompressionMethod> = (0..zip.len())
        .map(|i| {
            let entry = zip.by_index_raw(i).unwrap();
            (entry.name().to_string(), entry.compression())
        })
        .collect();

    assert_eq!(
        methods.get("plain.bin"),
        Some(&zip::CompressionMethod::Stored),
        "Stored entry must keep its method"
    );
    assert_eq!(
        methods.get("compressed.txt"),
        Some(&zip::CompressionMethod::Deflated),
        "Deflated entry must keep its method"
    );

    let mut plain = zip.by_name("plain.bin").unwrap();
    let mut plain_data = Vec::new();
    plain.read_to_end(&mut plain_data).unwrap();
    assert_eq!(plain_data, b"raw data that would compress but stays stored");
}
