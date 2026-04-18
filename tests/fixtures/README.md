# Test Fixtures

Test archives for validating CRC32 extraction and unified interface.

## Files

### test_file.txt
- **Content**: `Hello, RAR World!\n`
- **Size**: 18 bytes
- **CRC32**: `0x054607BC` (88,327,100 decimal)

### test.rar (RAR 4.x format)
- **Format**: RAR 4.x (legacy)
- **Contains**: test_file.txt
- **Compression**: RAR method, level 5
- **Expected CRC32**: `0x054607BC`
- **Archive Size**: 97 bytes

### test_rar5.rar (RAR 5.0 format)
- **Format**: RAR 5.0 (modern)
- **Contains**: test_file.txt
- **Compression**: RAR5 method
- **Expected CRC32**: `0x054607BC`
- **Archive Size**: 97 bytes

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
