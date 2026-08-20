---
type: Reference
title: 포맷 지원 매트릭스
description: ArchiveFormat이 정의하는 포맷별 기능 표, 확장자 및 백엔드 매핑, 감지 규칙을 다룹니다.
tags: [formats, archive, api]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/reference/user/en/format-support-matrix.md }
synced_hash: 04d1f686d0e651f69570ab77be6a790123f8e1865f8571c960eef81f1b223794
---
# 포맷 지원 매트릭스

이 페이지는 `src/format.rs`에 정의된 `ArchiveFormat`, 해당 포맷이 보고하는 기능 값, 각 변형에 대한 확장자 및 백엔드 매핑, 그리고 파일을 변형으로 변환하는 감지 규칙을 설명합니다. 값은 `README.md`나 `src/lib.rs`의 크레이트 수준 문서가 아닌 코드에서 직접 읽어옵니다.

## 용어

### `Support`

`unified_archive::Support`는 기능별 지원 등급입니다. `#[non_exhaustive]`이며 `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. 현재 세 가지 변형이 존재합니다:

| 변형 | `src/format.rs`에 문서화된 의미 |
|---|---|
| `Support::Full` | 완전히 지원되고 테스트됨. |
| `Support::Partial` | 부분적으로 지원됨 (예: 읽기는 작동하지만 쓰기는 작동하지 않음). |
| `Support::None` | 지원되지 않음. |

열거형이 `#[non_exhaustive]`이므로 다운스트림 `match` 문에는 와일드카드 분기(arm)가 포함되어야 합니다.

### `FormatCapabilities`

`unified_archive::FormatCapabilities`는 `ArchiveFormat::capabilities`에 의해 반환되는 레코드입니다. `#[non_exhaustive]`이며 `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. 모든 필드는 `Support` 유형입니다:

| 필드 | 설명하는 내용 |
|---|---|
| `encryption_read` | 이 형식의 암호화된 아카이브 읽기 또는 복호화. |
| `encryption_write` | 이 형식의 암호화된 아카이브 생성. |
| `multipart_read` | 이 형식의 다중 파트(분할, 다중 볼륨) 아카이브 읽기. |
| `multipart_write` | 이 형식의 다중 파트 아카이브 생성. |
| `modification` | 기존 아카이브에서 항목 추가, 제거 또는 교체. |
| `compression_read` | 압축 해제 지원. |
| `compression_write` | 생성 측에서의 압축 지원. |

`FormatCapabilities::compression()`은 읽기/쓰기 분할을 단일 `Support`로 통합합니다: 어느 한 쪽이라도 `Full`이면 `Full`, 그렇지 않고 어느 한 쪽이라도 `Partial`이면 `Partial`, 그렇지 않으면 `None`입니다.

### `ArchiveFormat`의 불리언 조건자

각 조건자는 기능 레코드를 기반으로 정의됩니다. 모두 `bool`을 반환합니다.

| 메서드 | 정의 |
|---|---|
| `supports_compression()` | `capabilities().compression() != Support::None` |
| `supports_compression_read()` | `capabilities().compression_read != Support::None` |
| `supports_compression_write()` | `capabilities().compression_write != Support::None` |
| `supports_encryption_read()` | `capabilities().encryption_read != Support::None` |
| `supports_encryption_write()` | `capabilities().encryption_write != Support::None` |
| `supports_encryption()` | `supports_encryption_read() \|\| supports_encryption_write()` |
| `supports_multipart_read()` | `capabilities().multipart_read != Support::None` |
| `supports_multipart_write()` | `capabilities().multipart_write != Support::None` |
| `supports_multipart()` | `supports_multipart_read() \|\| supports_multipart_write()` |
| `can_modify()` | `capabilities().modification != Support::None` |
| `can_create()` | 고정된 변형 목록; 아래 표 참조. |

불리언 형태는 `Full`과 `Partial`을 동일하게 `true`로 축소합니다. `can_create`는 `FormatCapabilities`에서 도출되지 않습니다: `Archive::create`가 수용하는 변형들의 명시적인 `matches!` 목록입니다.

## 기능 매트릭스

생성 연산 지원 및 `can_create` 플래그 설명.

*열기 / 검사* 및 *추출*은 `Archive::open`이 변형을 백엔드로 라우팅하는지 여부와 해당 백엔드가 추출을 제공하는지 여부를 기록합니다. *생성*은 `ArchiveFormat::can_create`입니다. 나머지 열은 `ArchiveFormat::capabilities`의 `Support` 값입니다.

| 변형 | 열기 / 검사 | 추출 | `Archive::create`를 통한 생성 | `modification` | `encryption_read` | `encryption_write` | `multipart_read` | `multipart_write` | `compression_read` | `compression_write` |
|---|---|---|---|---|---|---|---|---|---|---|
| `SevenZip` | 예 | 예 | 예 | `Partial` | `Full` | `None` | `None` | `None` | `Full` | `Full` |
| `Zip` | 예 | 예 | 예 | `Partial` | `Full` | `None` | `Partial` | `None` | `Full` | `Full` |
| `Rar` | 예 (기능 게이트) | 예 (기능 게이트) | 아니요 | `None` | `Full` | `None` | `Full` | `None` | `Full` | `None` |
| `Rar5` | 예 (기능 게이트) | 예 (기능 게이트) | 아니요 | `None` | `Full` | `None` | `Full` | `None` | `Full` | `None` |
| `Tar` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `None` | `None` |
| `TarGzip` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarBzip2` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarXz` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarZst` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarLz4` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `TarLzma` | 예 | 예 | 예 | `None` | `None` | `None` | `None` | `None` | `Full` | `Full` |
| `Gzip` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Bzip2` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Xz` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Zst` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Lz4` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Lzma` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `Full` | `None` |
| `Iso` | 예 | 예 | 아니요 | `None` | `None` | `None` | `None` | `None` | `None` | `None` |

`Tar`와 `Iso`는 두 컨테이너 모두 압축 필터를 적용하지 않기 때문에 `compression_read: None` 및 `compression_write: None`을 보고합니다; 둘 다 완전히 읽기 가능합니다.

`encryption_write`는 모든 변형에 대해 `None`입니다. `Archive::create`는 모든 형식에 대해 비밀번호를 보유한 모든 `CompressionOptions`를 `ArchiveError::OperationBlocked`로 거부합니다.

`multipart_write`는 모든 변형에 대해 `None`입니다. `CompressionOptions::split_size`가 필드로 존재하지만 `Archive::create`는 모든 형식에 대해 `Some(_)`을 거부합니다.

## 확장자, 표준 접미사 및 백엔드

`ArchiveFormat::extensions()`는 선두 점 **없이** 인식된 확장자 문자열을 반환합니다. `ArchiveFormat::suffix()`는 선두 점을 **포함하여** 표준 스테이징 접미사를 반환합니다; 이는 크레이트 내부용(`pub(crate)`)이며 자가 추출 아카이브의 페이로드가 임시 파일로 스테이징될 때 사용되어 스테이징된 파일이 동일한 필터 체인을 통해 라우팅되도록 합니다.

| 변형 | `extensions()` | `suffix()` | 읽기 백엔드 | 생성 백엔드 |
|---|---|---|---|---|
| `SevenZip` | `7z` | `.7z` | `sevenz-rust2` | libarchive (`7zip` 라이터) |
| `Zip` | `zip` | `.zip` | `zip` 크레이트 | `zip` 크레이트 |
| `Rar` | `rar` | `.rar` | UnRAR | 파사드를 통해 없음 |
| `Rar5` | `rar` | `.rar` | UnRAR | 파사드를 통해 없음 |
| `Tar` | `tar` | `.tar` | libarchive | libarchive (`pax_restricted`) |
| `TarGzip` | `tar.gz`, `tgz` | `.tar.gz` | libarchive | libarchive (`pax_restricted` + gzip 필터) |
| `TarBzip2` | `tar.bz2`, `tbz2`, `tb2` | `.tar.bz2` | libarchive | libarchive (`pax_restricted` + bzip2 필터) |
| `TarXz` | `tar.xz`, `txz` | `.tar.xz` | libarchive | libarchive (`pax_restricted` + xz 필터) |
| `TarZst` | `tar.zst`, `tzst` | `.tar.zst` | libarchive | libarchive (`pax_restricted` + zstd 필터) |
| `TarLz4` | `tar.lz4` | `.tar.lz4` | libarchive | libarchive (`pax_restricted` + lz4 필터) |
| `TarLzma` | `tar.lzma`, `tlz` | `.tar.lzma` | libarchive | libarchive (`pax_restricted` + lzma 필터) |
| `Gzip` | `gz` | `.gz` | libarchive | 없음 |
| `Bzip2` | `bz2` | `.bz2` | libarchive | 없음 |
| `Xz` | `xz` | `.xz` | libarchive | 없음 |
| `Zst` | `zst` | `.zst` | libarchive | 없음 |
| `Lz4` | `lz4` | `.lz4` | libarchive | 없음 |
| `Lzma` | `lzma` | `.lzma` | libarchive | 없음 |
| `Iso` | `iso` | `.iso` | libarchive | 없음 |

읽기 측 라우팅은 `Archive::open_as_format`에 상주합니다: `Rar`/`Rar5`는 UnRAR로, `Zip`은 `zip` 크레이트로, `SevenZip`은 `sevenz-rust2`로, 나머지 모든 변형은 libarchive로 라우팅됩니다. 생성 측 라우팅은 `Archive::create`에 상주합니다: `Zip`은 `zip`-크레이트 라이터로, 다른 모든 생성 가능한 형식은 libarchive 라이터로 라우팅됩니다 — `SevenZip`을 포함하며, 따라서 이의 읽기 및 쓰기 백엔드는 서로 다릅니다.

수정(Modification)은 `Zip`과 `SevenZip` 모두에 대해 libarchive를 통해 읽기 측을 엽니다; `Rar`/`Rar5`는 해당 지점 이전에 거부됩니다.

이 분할의 배경은 [여러 백엔드를 아우르는 하나의 API](../../../explanation/user/ko/one-api-many-backends.md)를 참조하십시오.

## 형식별 참고 사항

코드가 강제하는 주의 사항만 여기에 나열됩니다.

**`Rar` / `Rar5`는 `rar-support` Cargo 기능을 필요로 합니다.** 기본적으로 켜져 있습니다. 해당 기능이 없는 빌드에서 `Archive::open` 및 `Archive::open_encrypted`는 해당 기능을 명시하는 이유와 함께 두 변형에 대해 `ArchiveError::Unsupported`를 반환합니다. 기능 표는 조건이 없습니다: 기능이 컴파일에 포함되어 있는지 여부에 관계없이 동일한 값을 보고합니다.

**`Rar` / `Rar5`는 파사드를 통해 생성될 수 없습니다.** `can_create`는 `false`이고 `compression_write`는 `None`입니다. 별도의 생성기인 `unified_archive::external::RarCreator`는 `all(target_os = "windows", feature = "external-rar-create")` 하에서만 컴파일됩니다. 라이선스가 부여된 WinRAR `rar.exe`를 셸 실행하며 `Archive`를 통해서는 접근할 수 없습니다.

**`Rar` 및 `Rar5`는 `rar` 확장자를 공유합니다.** 따라서 확장자 파생 추측으로는 둘을 구별할 수 없으며, `format_from_extension`은 선택하는 대신 `.rar`에 대해 `None`을 반환합니다. 매직 바이트 감지만이 둘을 구별합니다.

**`Zip` `multipart_read`는 `Partial`입니다.** `Archive::detect_multipart` 및 `Archive::multipart_layout`은 `supports_multipart()`가 참인 형식에 대해 형제 볼륨(`.zip` 및 `.zNN`)을 열거하므로 분할된 ZIP 세트가 *보고*됩니다. 해당 볼륨에 걸친 추출은 종단 간으로 구현되지 않았습니다. `Rar`/`Rar5`는 `Full`입니다: 해당 볼륨 세트는 전반에 걸쳐 읽힙니다.

**`Zip` 및 `SevenZip` `modification`은 `Full`이 아니라 `Partial`입니다.** 수정은 기록 시 복사(copy-on-write) 재작성입니다: 항목이 새 아카이브로 복사되므로 심볼릭 링크, 하드 링크 및 특수 항목이 삭제되고 메타데이터와 레이아웃이 정규화됩니다. `SevenZip`의 경우 재작성은 솔리드/블록 레이아웃, 암호화 및 여러 7z 전용 메타데이터 필드도 삭제합니다. [수정이 재작성인 이유](../../../explanation/developer/ko/modification-is-a-rewrite.md)를 참조하십시오.

**`TarZst`, `TarLz4`, `TarLzma` 생성은 연결된 libarchive에 의존합니다.** 라이터는 `archive_write_add_filter_zstd`, `archive_write_add_filter_lz4`, 또는 `archive_write_add_filter_lzma`를 호출합니다. 일치하는 코덱 라이브러리 없이 빌드된 libarchive는 필터를 외부 프로그램 폴백으로 등록하고 경고 상태를 반환하며, 라이터는 외부 압축기를 셸 실행하는 대신 라이터 생성 시점에 이를 `ArchiveError::Format`으로 변환합니다. 이러한 형식을 읽으려면 해당 읽기 필터만 필요합니다.

**`TarZst`는 `CompressionLevel::Store`를 레벨 1로 취급합니다.** zstd에는 "압축 없음" 레벨이 없으므로 라이터는 해당 형식에 대해 `0` 대신 `1`을 보냅니다; 다른 모든 생성 가능한 형식은 변경되지 않은 레벨 매핑을 받습니다.

**독립형 압축 스트림은 읽기 전용입니다.** `Gzip`, `Bzip2`, `Xz`, `Zst`, `Lz4`, 및 `Lzma`는 `compression_read: Full` 및 `compression_write: None`을 보고하며 `can_create`는 `false`입니다. 단일 파일 압축 스트림을 생성하는 것은 범위 외입니다; 이러한 코덱은 `Tar*` 복합 형식을 통해서만 생성할 수 있습니다.

**`Iso`는 읽기 전용입니다.** libarchive의 ISO 지원은 쓰기를 수행하지 않으므로 `can_create`는 `false`입니다.

**libarchive 기반 형식에 대해서는 `verify_crc32`가 거부됩니다.** 백엔드가 libarchive인 핸들(`Tar*` 패밀리, 독립형 압축 스트림 및 `Iso`)에서 `ExtractionOptions::verify_crc32 = true`를 설정하면 libarchive가 항목별 CRC32를 노출하지 않기 때문에 `ArchiveError::Unsupported`가 반환됩니다. 백엔드별 세부 정보는 [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md)을 참조하십시오.

## 감지

### `ArchiveFormat::detect(path)`

콘텐츠 우선, 확장자 후순위.

1. 파일을 열고 하나의 버퍼를 읽습니다. 버퍼는 512바이트이며, 경로의 소문자 확장자가 `iso`, `bin`, `img` 또는 `cd`인 경우 33 KiB로 확장됩니다 — ISO 주 볼륨 디스크립터(Primary Volume Descriptor)가 512바이트 창을 지나는 바이트 오프셋 32769에 위치하기 때문입니다.
2. 4바이트 미만으로 읽힌 경우 콘텐츠 시그널이 전혀 존재하지 않습니다. `format_from_extension`이 참조되며 답이 있는 경우 아래 설명된 `is_extension_fallback` 게이트 없이 해당 답이 반환됩니다. 그렇지 않으면 호출이 `File too small to detect format`을 읽는 형식 오류로 실패합니다.
3. `detect_from_bytes`가 읽은 바이트에 대해 실행됩니다. 성공 시 결과는 복합 tar 승격(아래 참조)을 거쳐 반환됩니다.
4. `detect_from_bytes`가 아무것도 찾지 못한 경우 `format_from_extension`이 참조되며, 해당 답은 **`is_extension_fallback()`이 참일 때만** 수용됩니다.
5. 그렇지 않으면 호출이 `Unknown archive format`을 읽는 형식 오류로 실패합니다.

콘텐츠가 우선하므로 `.zip`으로 이름이 변경된 RAR 파일은 RAR로 열리고, 손상된 `foo.zip`은 확장자를 신뢰하기보다 여전히 감지에 실패합니다.

`Archive::open`은 자체적인 한 단계를 추가합니다: `detect`가 실패하고 경로의 확장자가 `exe`, `com`, `scr`, `app`, `run`, `sh`, 또는 `bash`일 때 자가 추출 아카이브 경로로 라우팅됩니다. [SFX 감지가 결정하는 방식](../../../explanation/developer/ko/sfx-detection-pipeline.md)을 참조하십시오.

### `ArchiveFormat::detect_from_bytes(magic)`

원시 바이트를 받으며 확장자 정보를 사용하지 않습니다. 4바이트 미만은 `Buffer too small to detect format`을 읽는 형식 오류를 반환합니다. 프로브는 이 순서로 실행되며 첫 번째 일치 항목이 승리합니다:

| 순서 | 변형 | 게이트 |
|---|---|---|
| 1 | `Rar5` | 접두사 `Rar!\x1A\x07\x01\x00`. |
| 2 | `Rar` | 접두사 `Rar!\x1A\x07\x00`. |
| 3 | `Zip` | 접두사 `PK\x03\x04`, 또는 최소 22바이트가 존재하는 접두사 `PK\x05\x06`(전체 고정 중앙 디렉터리 끝 레코드). 데이터 디스크립터 시그니처 `PK\x07\x08`은 제외됩니다. |
| 4 | `SevenZip` | 접두사 `7z\xBC\xAF\x27\x1C`. |
| 5 | `Gzip` | 최소 10바이트, 바이트 0과 1은 `0x1F 0x8B`, 바이트 2는 `0x08`(deflate), 바이트 3의 예약된 플래그 비트(`0xE0`)가 클리어되어 있음. |
| 6 | `Bzip2` | 접두사 `BZh` 뒤에 블록 크기 숫자 `1`–`9`. |
| 7 | `Xz` | 접두사 `FD 37 7A 58 5A 00`. |
| 8 | `Zst` | 접두사 `28 B5 2F FD`. 건너뛸 수 있는 프레임(skippable-frame) 매직은 탐색되지 않습니다. |
| 9 | `Lz4` | 접두사 `04 22 4D 18`(최신 프레임) 또는 `02 21 4C 18`(레거시 프레임). |
| 10 | `Tar` | 최소 512바이트, 오프셋 257의 `ustar`, 오프셋 148–155의 저장된 헤더 검사합이 해당 필드를 공백으로 취한 바이트 합과 일치함. 부호 없는 바이트 합과 역사적인 부호 있는 바이트 합 변형 모두 허용됩니다. |
| 11 | `Tar` | POSIX V7 이전 폴백으로 `ustar` 프로브 실패 후에만 시도됨: 최소 512바이트, `ustar` **부재**, 동일한 헤더 검사합 유효, 비어 있지 않은 이름 필드, 8진수 크기 및 mtime 필드, NUL, `0`, `1` 또는 `2` 링크 플래그. |
| 12 | `Iso` | 최소 32775바이트, 바이트 32768은 `0x01`, 바이트 32769–32773은 `CD001`, 바이트 32774는 `0x01`. |

일치 항목이 없으면 `Unknown archive format from magic bytes`를 읽는 형식 오류를 반환합니다.

버퍼 크기에서 두 가지 결과가 따릅니다. `Iso`는 호출자가 32775바이트 이상을 제공할 때만 반환되며, 이는 `detect`의 확장된 창과 자가 추출 아카이브 스캔 버퍼는 수행하지만 512바이트 기본값은 수행하지 않습니다: `mystery.dat`로 명명된 ISO는 파일에서 감지되지 않습니다. 그리고 `Lzma`는 프로브가 전혀 없으므로 — 원시 LZMA에는 안정적인 짧은 매직 마커가 없음 — `detect_from_bytes`에 의해 결코 반환되지 않습니다.

### `format_from_extension(path)`

진단 전용 퍼블릭 헬퍼입니다. 경로의 이름이 하나의 변형에 명확하게 매핑될 때 `Some(format)`을 반환하고 그렇지 않으면 `None`을 반환합니다. 복합 접미사는 단일 확장자 전에 검사됩니다:

| 이름 종결 접미사 | 결과 |
|---|---|
| `.tar.gz`, `.tgz` | `TarGzip` |
| `.tar.bz2`, `.tbz2`, `.tb2` | `TarBzip2` |
| `.tar.xz`, `.txz` | `TarXz` |
| `.tar.zst`, `.tzst` | `TarZst` |
| `.tar.lz4` | `TarLz4` |
| `.tar.lzma`, `.tlz` | `TarLzma` |

그런 다음 소문자 확장자에 따라: `zip`은 `Zip`으로, `7z`는 `SevenZip`으로, `tar`는 `Tar`로, `gz`는 `Gzip`으로, `bz2`는 `Bzip2`로, `xz`는 `Xz`로, `zst`는 `Zst`로, `lz4`는 `Lz4`로, `lzma`는 `Lzma`로, `iso`는 `Iso`로 매핑됩니다. 다른 모든 것은 의도적인 두 가지 사례를 포함하여 `None`을 반환합니다: 확장자가 `Rar`와 `Rar5`를 구별할 수 없는 `rar`, 그리고 `detect`의 읽기 창을 넓히지만 ISO 콘텐츠를 다루지 않는 `bin`, `img`, `cd`.

`Archive::extension_format()`은 열린 아카이브 경로에 적용된 이 함수입니다. 이를 `Archive::format()`과 비교하여 확장자/콘텐츠 불일치를 관찰합니다.

### `is_extension_fallback()`

`detect`의 4단계를 게이팅하는 크레이트 내부 조건자입니다. 권위 있는 감지 시그널이 확장자인 정확히 4개 변형에 대해 참입니다:

- `Tar` — 방언 간에 일관된 매직이 없음.
- `Iso` — 주 볼륨 디스크립터가 기본 읽기 창 너머에 위치함.
- `Lzma` — 안정적인 짧은 매직 마커가 없음.
- `TarLzma` — 동일한 이유.

다른 모든 변형은 독자적으로 매직 바이트 감지를 통과해야 합니다. 따라서 `format_from_extension` 지도 전체가 폴백 세트가 아닙니다: 진단용으로 사용되는 상위 집합입니다.

### 복합 tar 승격

`detect_from_bytes`는 외부 코덱 프레임만 확인하므로 `.tar.gz`는 `Gzip`으로 보고됩니다. 성공적인 콘텐츠 감지 후 파일 이름이 일치하면 `detect`는 순수 코덱을 해당 복합 tar 변형으로 승격시킵니다:

| 감지됨 | 파일 이름 종결 접미사 | 승격됨 |
|---|---|---|
| `Gzip` | `.tar.gz`, `.tgz` | `TarGzip` |
| `Bzip2` | `.tar.bz2`, `.tbz2`, `.tb2` | `TarBzip2` |
| `Xz` | `.tar.xz`, `.txz` | `TarXz` |
| `Zst` | `.tar.zst`, `.tzst` | `TarZst` |
| `Lz4` | `.tar.lz4` | `TarLz4` |
| `Lzma` | `.tar.lzma`, `.tlz` | `TarLzma` |

감지된 다른 모든 변형은 변경 없이 통과하며 파일 이름이 없는 경로도 변경 없이 통과합니다. `Lzma` 행은 `Lzma`를 결코 반환하지 않는 `detect_from_bytes`에서 도달할 수 없습니다; `.tar.lzma` 및 `.tlz` 이름은 확장자 폴백 단계를 통해 `TarLzma`에 도달합니다. 동일한 헬퍼가 수정 경로의 형식 감지에 재사용됩니다.

이 구분은 단순 명칭뿐만 아니라 동작에서도 중요합니다: 복합 변형은 `compression_read: Full`을 가지며 생성 가능하지만 승격의 원천이었던 순수 코덱 변형은 읽기 전용입니다.

## 참고 항목

- [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md) — 모든 옵션 필드, 유형 및 기본값.
- [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md) — 이 페이지에 기재된 오류 변형.
- [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md) — 이러한 유형들이 재내보내지는 위치.
- [Cargo 기능 및 MSRV](../../../reference/developer/ko/cargo-features.md) — `rar-support`, `external-rar-create`, `v2-api`.
