# 테스트 픽스처

CRC32 추출 및 통합 인터페이스 검증을 위한 테스트 압축 파일입니다.

## 파일

### test_file.txt
- **내용**: `Hello, RAR World!\n`
- **크기**: 18 바이트
- **CRC32**: `0x054607BC` (십진수 88,327,100)

### test.rar (RAR 4.x 형식)
- **형식**: RAR 4.x (레거시)
- **포함 파일**: test_file.txt
- **압축**: RAR 방식, 레벨 5
- **예상 CRC32**: `0x054607BC`
- **압축 파일 크기**: 97 바이트

### test_rar5.rar (RAR 5.0 형식)
- **형식**: RAR 5.0 (모던)
- **포함 파일**: test_file.txt
- **압축**: RAR5 방식
- **예상 CRC32**: `0x054607BC`
- **압축 파일 크기**: 97 바이트

## 테스트에서의 사용

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
    assert_eq!(entries[0].crc32, Some(0x054607BC)); // 동일한 CRC32!
    assert_eq!(entries[0].size, Some(18));
}
```

## 검증

CRC32를 수동으로 검증하려면:
```bash
rar lt tests/fixtures/test.rar | grep CRC32
rar lt tests/fixtures/test_rar5.rar | grep CRC32
```

둘 다 다음과 같이 표시되어야 합니다: `CRC32: 054607BC`
