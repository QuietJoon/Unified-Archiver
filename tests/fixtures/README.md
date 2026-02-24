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
