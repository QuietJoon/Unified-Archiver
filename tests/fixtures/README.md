# Test Fixtures

Test archives for validating CRC32 extraction and unified interface.

## Files

### test_file.txt
- **Content**: `Hello, RAR World!\n`
- **Size**: 18 bytes
- **CRC32**: `0x054607BC` (88,327,100 decimal)

### test.rar (RAR 5.0 format)
- **Format**: RAR 5.0 (`file` reports `RAR archive data, v5`)
- **Contains**: test_file.txt
- **Expected CRC32**: `0x054607BC`
- **Archive Size**: 97 bytes
- **Note (R0074-0077)**: previously labeled "RAR 4.x legacy" but the
  on-disk magic bytes are RAR5. Integration tests assert
  `ArchiveFormat::Rar5` for this fixture
  (see `tests/integration/format_compatibility.rs`). `test_rar5.rar`
  exists alongside it as a second RAR5 fixture used by separate tests.

### test_rar5.rar (RAR 5.0 format)
- **Format**: RAR 5.0 (modern)
- **Contains**: test_file.txt
- **Compression**: RAR5 method
- **Expected CRC32**: `0x054607BC`
- **Archive Size**: 97 bytes

### test.iso (ISO 9660 + Joliet image)
- **Contains**: test_file.txt (same 18-byte payload as the other fixtures)
- **Archive Size**: 921,600 bytes (the minimum `hdiutil makehybrid` emits)
- **Purpose (R0079-0033)**: end-to-end coverage for the advertised
  ISO read/extract support (open / list / extract_to_memory in
  `tests/format_compatibility_test.rs`)

Generation command (macOS):
```bash
mkdir -p /tmp/iso_stage
printf 'Hello, RAR World!\n' > /tmp/iso_stage/test_file.txt
hdiutil makehybrid -iso -joliet -default-volume-name UA_TEST \
    -o tests/fixtures/test.iso /tmp/iso_stage
```

### test_encrypted_data.rar (RAR with data-only encryption)
- **Format**: RAR 5 with data encryption (headers readable without password)
- **Contains**: test_file.txt
- **Password**: `test123`
- **Expected CRC32**: `0x054607BC` (same payload as test.rar)

Generation command:
```bash
cd tests/fixtures
rar a -ptest123 test_encrypted_data.rar test_file.txt    # -p (data only), NOT -hp
unrar t -ptest123 test_encrypted_data.rar                # must print "All OK"
unrar l test_encrypted_data.rar                          # must list without password
```

The fixture MUST use data-only encryption (`-p`, not `-hp`). The
`test_detect_encrypted_rar_archive` test opens the archive without a
password and needs to read the headers to detect the encryption flag.
Every regeneration must pass `unrar t -ptest123` and `unrar l` (no
password) before the fixture is committed.

### test.txt.zst / test.txt.lz4 / test.txt.lzma (standalone codec streams)
- **Content**: `unified-archive codec fixture\n` repeated 4 times (120 bytes)
- **Sizes**: 50 / 60 / 56 bytes
- **Purpose (OI-0078-001)**: end-to-end open / list / extract coverage for
  the read-only ZST, LZ4, and LZMA formats
  (see `tests/integration/readonly_codec_formats.rs`). Per MADR-0019 the
  single raw stream is exposed as one pseudo-entry named after the archive
  stem (`test.txt`).

### test.tar.zst / test.tar.lz4 / test.tar.lzma (compressed tar archives)
- **Contains**: `codec_member.txt` with the same 120-byte payload as above
- **Sizes**: 148 / 198 / 151 bytes (ustar tar, 2048 bytes uncompressed)
- **Purpose (OI-0078-001)**: list / extract_all coverage for the TarZst,
  TarLz4, and TarLzma formats.

Generation commands (macOS, Homebrew `zstd` 1.5.7, `lz4` 1.10.0, `xz` 5.8.3;
staged under a scratch dir, only the compressed outputs are committed):
```bash
mkdir -p stage/tarstage && cd stage
for i in 1 2 3 4; do printf 'unified-archive codec fixture\n'; done > test.txt
cp test.txt tarstage/codec_member.txt
tar --format ustar -cf test.tar -C tarstage codec_member.txt
zstd -q -o test.txt.zst test.txt
lz4 -q test.txt test.txt.lz4
xz --format=lzma -q -k test.txt        # emits test.txt.lzma
zstd -q -o test.tar.zst test.tar
lz4 -q test.tar test.tar.lz4
xz --format=lzma -q -k test.tar        # emits test.tar.lzma
cp test.txt.{zst,lz4,lzma} test.tar.{zst,lz4,lzma} <repo>/tests/fixtures/
```

## Usage in Tests

```rust
use unified_archive::ffi::wrapper::UnrarArchive;

#[test]
fn test_rar4_crc32() {
    let archive = UnrarArchive::open("tests/fixtures/test.rar").unwrap();
    let entries = archive.list_files().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].crc32, Some(0x054607BC));
    assert_eq!(entries[0].size, Some(18));
}

#[test]
fn test_rar5_crc32() {
    let archive = UnrarArchive::open("tests/fixtures/test_rar5.rar").unwrap();
    let entries = archive.list_files().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "test_file.txt");
    assert_eq!(entries[0].crc32, Some(0x054607BC)); // Same CRC32!
    assert_eq!(entries[0].size, Some(18));
}
```

## Verification

To verify CRC32 manually:
```bash
rar lt tests/fixtures/test.rar | grep CRC32
rar lt tests/fixtures/test_rar5.rar | grep CRC32
```

Both should show: `CRC32: 054607BC`
