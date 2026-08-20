---
type: Reference
title: 옵션 및 기본값
description: unified-archive의 모든 옵션 및 제한사항 타입에 대한 필드별 설명, 타입, 기본값 및 백엔드별 의미 체계입니다.
tags: [config, api, extraction, creation, modification, security]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/reference/user/en/options-and-defaults.md }
synced_hash: 266489f25cc2ca405b013734f44a8eb6c6301049d41c42811da65e2ce825ac69
---
# 옵션 및 기본값

공개 API가 허용하는 모든 구성 타입, 그 필드, 타입, 기본값 및 코드가 기록하는 의미론입니다. 포맷 수준의 기능에 관한 질문은 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)를 참조하세요. 여기에 언급된 오류 변형은 [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md)에 설명되어 있습니다.

## `ExtractionOptions`

`src/options.rs`에 정의되어 있습니다. 모든 필드가 `pub`이고 구조체가 `#[non_exhaustive]`가 아니므로 구조체 리터럴 구성을 사용할 수 있습니다. 파생(derive) 항목이 없습니다: `filter`와 `progress`가 박싱된 트레이트 객체를 가지고 있기 때문에 `Debug`와 `Clone`이 제공되지 않습니다.

| 필드 | 타입 | 기본값 |
|---|---|---|
| `destination` | `PathBuf` | `PathBuf::from(".")` |
| `password` | `Option<Password>` | `None` |
| `overwrite` | `bool` | `false` |
| `preserve_permissions` | `bool` | `true` |
| `preserve_times` | `bool` | `true` |
| `verify_crc32` | `bool` | `false` |
| `limits` | `ExtractionLimits` | `ExtractionLimits::default()` |
| `filter` | `Option<EntryFilter>` | `None` |
| `progress` | `Option<Box<dyn ProgressCallback>>` | `None` |

하나의 빌더 메서드가 존재합니다: `ExtractionOptions::password(self, impl Into<String>) -> Self`는 인자를 `Password`로 감싸고 `ExtractionOptions::default()`에서 체이닝할 수 있도록 값을 반환합니다.

`overwrite: false`는 출력 파일이 이미 존재하는 경우 이를 교체하지 않고 추출이 오류와 함께 실패함을 의미합니다.

### `preserve_permissions`

추출된 파일의 Unix 모드 비트입니다. 필드 문서에 기록된 백엔드별 동작:

- **ZIP** — 항목이 Unix 스타일 호스트에 의해 작성된 경우 중앙 디렉터리의 Unix 모드 비트가 권한 비트에 마스킹되어 적용됩니다. Unix 메타데이터가 없는 항목은 스테이징 파일의 기본 모드를 유지합니다.
- **7z** — 아카이브가 속성 필드의 상위 절반에 Unix 모드를 저장할 때 적용됩니다 (p7zip 규칙).
- **RAR / RAR5** — 이 플래그는 참조되지 않습니다. UnRAR 라이브러리가 항목 속성을 직접 적용하므로 권한은 항상 보존됩니다.
- **libarchive 기반** (TAR 패밀리, ISO, 단일 압축 스트림) — `ARCHIVE_EXTRACT_PERM`을 통해 준수됩니다.

Unix 이외의 플랫폼에서는 네이티브 Rust 백엔드(ZIP 및 7z)에 대해 이 플래그가 아무런 영향을 미치지 않습니다.

### `preserve_times`

추출된 파일의 수정 시간:

- **ZIP** — DOS 정밀도(2초) 수정 시간이 추출된 각 파일에 적용됩니다.
- **7z** — 아카이브가 NT-time 수정 타임스탬프를 가지고 있을 때 적용됩니다.
- **RAR / RAR5** — 이 플래그는 참조되지 않습니다. UnRAR 라이브러리가 항목 시간을 직접 적용하므로 항상 보존됩니다.
- **libarchive 기반** — `ARCHIVE_EXTRACT_TIME`을 통해 준수됩니다.

네이티브 Rust 백엔드는 수정 시간만 복원합니다. 접근 시간 및 생성 시간은 해당 백엔드에서 조회 전용 메타데이터입니다.

### `verify_crc32`

추출 중 항목별 CRC32 검증:

- **ZIP** — 명시적인 항목별 CRC32 검증이 준수됩니다.
- **7z** — 코덱이 무조건 무결성을 검사하므로 이 플래그와 관계없이 CRC32가 항상 검증됩니다. `false`로 설정하는 것은 동작 변경 없이 \"CRC32를 요구하지 않음\"을 표현합니다.
- **RAR / RAR5** — 7z와 동일: CRC32가 항상 검증됩니다.
- **libarchive 기반** — 항목별 CRC32가 노출되지 않습니다. `verify_crc32 = true`는 `ArchiveError::Unsupported`를 반환합니다; 감싸고 있는 코덱 자체의 무결성 검사는 여전히 실행됩니다.

이 게이트는 모든 추출 진입점에서 파일이 작성되기 전에 모든 I/O에 앞서 실행되므로 지원되지 않는 요청은 파일이 기록되기 전에 실패합니다. CRC32 일치가 증명하는 것과 증명하지 못하는 것에 대한 논의는 [아카이브 체크섬이 실제로 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)을 참조하세요.

## `CompressionLevel`

선언 순서대로 6개의 변형을 가진 열거형입니다: `Store`, `Fastest`, `Fast`, `Normal`, `Maximum`, `Ultra`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. 자체 기본값은 없으며 이를 포함하는 옵션 타입은 `Normal`을 기본값으로 합니다.

백엔드는 이 변형들을 자체 스케일에 매핑합니다:

| 변형 | ZIP 작성기 | libarchive 작성기 (`compression-level`) |
|---|---|---|
| `Store` | 무압축(stored) 방식, deflate 레벨 없음 | `0` (`TarZst`의 경우 `1`) |
| `Fastest` | deflate, 레벨 1 | `1` |
| `Fast` | deflate, 레벨 3 | `3` |
| `Normal` | deflate, 레벨 6 | `6` |
| `Maximum` | deflate, 레벨 8 | `8` |
| `Ultra` | deflate, 레벨 9 | `9` |

libarchive 작성기의 경우 이 값은 `Tar*` 포맷에서는 필터 옵션으로, ZIP 및 7z에서는 포맷 옵션으로 설정됩니다. 옵션을 무시하는 libarchive는 경고 상태를 반환하며, 작성기는 코덱 기본값으로 암묵적 폴백하는 대신 이를 `ArchiveError::Format`으로 변환합니다.

## `CompressionOptions`

`src/options.rs`에 정의되어 있습니다. 모든 필드가 `pub`이고 구조체는 의도적으로 **`#[non_exhaustive]`가 아님**으로써 0.3 버전과의 구조체 리터럴 소스 호환성을 유지합니다.

| 필드 | 타입 | 기본값 (`new` / `Default`) |
|---|---|---|
| `format` | `ArchiveFormat` | `new`: 인자 값. `Default`: `ArchiveFormat::Zip` |
| `level` | `CompressionLevel` | `CompressionLevel::Normal` |
| `password` | `Option<Password>` | `None` |
| `split_size` | `Option<u64>` | `None` |
| `progress` | `Option<Box<dyn ProgressCallback>>` | `None` |

`CompressionOptions::new(format)`은 포맷을 설정하고 위와 같은 기본값을 가져옵니다. `CompressionOptions::default()`는 `new(ArchiveFormat::Zip)`입니다.

메서드:

- `format(&self) -> ArchiveFormat` — 필드를 읽어옵니다.
- `password(self, impl Into<String>) -> Self` — `Password`를 저장하는 빌더 설정자입니다.
- `strip_progress(&self) -> Self` — 아래를 참조하세요.
- `validate_for_format(&self) -> Result<()>` — 아래를 참조하세요.

직접 작성된 `Debug` 구현이 존재합니다. `format`, `level`, `split_size`, 그리고 `Some("***")` 또는 `None` 형태의 `password`, `is_some()` 불리언 형태의 `progress`를 출력합니다.

### `Clone`이 구현되지 않은 이유

`progress`는 복제할 수 없는 `Box<dyn ProgressCallback>`입니다. 이전의 `Clone` 구현은 콜백을 조용히 삭제하여 복제된 옵션 값이 호출자 모르게 진행 상황을 내보내지 않는 현상이 발생했습니다. 이에 손실을 허용하는 대신 구현이 제거되었습니다.

### `strip_progress`

`progress`가 `None`으로 설정된 상태에서 동일한 `format`, `level`, `password`, `split_size`를 가지는 새로운 `CompressionOptions`를 반환합니다. 반환된 값이 다시 할당되지 않는 한 원본 콜백은 `self`에 유지됩니다. 이는 제거된 `Clone`을 대체하는 명시적 방법입니다.

### `validate_for_format`

`Archive::create`가 거부할 구성을 사전에 보고하는 선택적 사전 검사입니다. `Archive::create`는 작성기를 구축하기 전에 내부적으로 동일한 메서드를 호출하므로 두 표면이 달라질 수 없습니다. 검사하는 순서대로 적용되는 규칙:

1. `format`은 `ArchiveFormat::can_create()`를 만족해야 합니다. 그렇지 않으면 사유가 `format {:?} is not supported for creation via Archive::create`로 읽힙니다.
2. `password`는 모든 포맷에 대해 `None`이어야 합니다. 그렇지 않으면 사유가 `encrypted creation for {:?} is not supported (MADR-0027); password must be None`으로 읽힙니다.
3. `split_size`는 모든 포맷에 대해 `None`이어야 합니다. 그렇지 않으면 사유가 `split_size is not supported by any backend yet (DEF-002); leave it None`으로 읽힙니다.

세 항목 모두에 적용되는 작업 레이블은 크레이트의 `create` 작업 상수입니다.

### 포맷별 빌더

세 가지 더 좁은 범위의 빌더가 존재합니다. 각각은 해당 포맷이 지원하는 필드만 노출하므로 지원되지 않는 조합은 런타임에 거부되는 대신 표현조차 불가능합니다. 필드는 `pub(crate)`이며 값은 메서드를 통해 설정됩니다.

| 타입 | 노출 필드 | `new` | `Default` | 쌍을 이루는 생성자 |
|---|---|---|---|---|
| `ZipCompressionOptions` | `level`, `progress` | `new()` | 예, `new()`와 동일 | `Archive::create_zip` |
| `SevenZCompressionOptions` | `level`, `progress` | `new()` | 예, `new()`와 동일 | `Archive::create_seven_zip` |
| `LibarchiveCompressionOptions` | `format`, `level`, `progress` | `new(format)` | 없음 | `Archive::create_libarchive` |

세 빌더 모두 `level(self, CompressionLevel) -> Self` 및 `progress(self, Box<dyn ProgressCallback>) -> Self`를 제공합니다. `new()`(및 `new(format)`)는 진행률 콜백 없이 `CompressionLevel::Normal`에서 시작합니다.

ZIP 및 7z 빌더는 `password` 필드를 노출하지 않습니다: 모든 포맷에 대해 생성 시 암호화가 지원되지 않으므로 암호화된 생성 상태를 작성할 수 없습니다. `LibarchiveCompressionOptions`는 포맷을 처음에 받으며 사전 검증하지 않습니다; `can_create()`가 거부하는 포맷은 `Archive::create_libarchive` 호출 시 거부됩니다.

각 빌더는 `From` 구현을 통해 `CompressionOptions`로 변환됩니다:

- `From<ZipCompressionOptions>`는 `format: ArchiveFormat::Zip`을 설정하고, `level`과 `progress`를 전달하며, `password: None`, `split_size: None`을 설정합니다.
- `From<SevenZCompressionOptions>`는 `format: ArchiveFormat::SevenZip`을 설정하고 나머지는 동일합니다.
- `From<LibarchiveCompressionOptions>`는 `format`, `level`, `progress`를 전달하며 `password: None`, `split_size: None`을 설정합니다.

`Archive::create_zip` 및 `Archive::create_seven_zip`은 정확히 `Archive::create(path, opts.into())`와 동일합니다. `Archive::create_libarchive`는 먼저 `ArchiveFormat::Zip`을 `OperationBlocked`로 거부한 다음 동일한 작업을 수행합니다.

## `ModificationOptions`

`src/modification.rs`에 정의되어 있습니다. 모든 필드는 `pub`입니다. `Debug`만 파생하며 `Clone`은 파생하지 않습니다. `compression`이 `CompressionOptions`를 포함하므로 복제할 수 없는 진행률 콜백을 가지고 있기 때문입니다.

| 필드 | 타입 | 기본값 (`new` / `Default`) |
|---|---|---|
| `preserve_metadata` | `bool` | `true` |
| `create_backup` | `bool` | `false` |
| `backup_suffix` | `String` | `".bak"` |
| `compression` | `Option<CompressionOptions>` | `None` |

`ModificationOptions::default()`는 `ModificationOptions::new()`입니다.

### `preserve_metadata`

`true`인 경우, 커밋 시 소스 목록이 이를 노출하는 영역에서 `modified`, `accessed`, `created` 시간과 이를 지원하는 백엔드의 Unix 권한을 다시 내보냅니다. libarchive 작성기는 `archive_entry_set_atime` 및 `archive_entry_set_birthtime`을 사용합니다; ZIP 작성기는 `0x5455` \"Universal Time\" 추가 필드 블록을 부착하여 재작성 중에도 접근 및 생성 시간이 유지되도록 합니다.

이 옵션은 **일반 파일 항목에만 적용됩니다**. 보존되는 디렉터리 항목은 이 플래그와 관계없이 백엔드 기본 권한(libarchive의 경우 `0o755`)과 재작성 시점의 현재 시간으로 다시 내보내집니다.

### `compression`

`None`은 포맷 기본값(`CompressionLevel::Normal`, 비밀번호 없음)을 의미합니다. `Some`인 경우 `level`과 `progress`가 재작성에 참여합니다; `password`는 생성 파사드에서 거부되고 `split_size`는 미사용 상태로 남습니다. 재작성은 소스 컨테이너 포맷을 유지해야 합니다: 커밋은 `format` 필드가 아카이브의 감지된 포맷과 일치하지 않는 재정의를 대체하지 않고 거부합니다.

### `with_backup(suffix)`

`create_backup = true`를 설정하고 `suffix`를 기록합니다. `suffix`는 `.bak` 또는 `.orig`와 같은 동종 파일 접미사이어야 합니다; 백업은 항상 소스 옆에 `<archive_path><suffix>`로 작성됩니다.

비어 있거나 `/` 또는 `\`를 포함하는 접미사는 유효하지 않습니다. 이 경우 제공된 값은 무시되고 `.bak`이 대신 사용됩니다. 디버그 빌드에서는 추가로 `debug_assert!`가 발동합니다; 릴리스 빌드는 프로그래밍 실수가 패닉으로 확대되지 않도록 암묵적 폴백을 유지합니다. 커밋 경로는 접미사를 재검증하며, 이것이 핵심 검사입니다.

### `without_metadata_preservation()`

`preserve_metadata = false`를 설정하고 `self`를 반환합니다.

## `ExtractionLimits`

`src/security.rs`에 정의되어 있습니다. `Debug` 및 `Clone`을 파생합니다. 모든 필드는 **비공개(private)**입니다; `ExtractionLimits::default()` 또는 `ExtractionLimits::builder()`로 생성하고 접근자(accessor)를 통해 읽습니다. 각 상한선은 센티널 값을 가진 원시 숫자가 아닌 타입화된 `Cap` 또는 `CompressionRatio`입니다.

| 접근자 | 반환 타입 | 기본값 |
|---|---|---|
| `max_total_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_TOTAL_SIZE)` |
| `max_file_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_FILE_SIZE)` |
| `max_compression_ratio()` | `Option<CompressionRatio>` | `Some(1000 / 1)` |
| `max_entry_count()` | `Cap` | `Cap::Limited(DEFAULT_MAX_ENTRY_COUNT as u64)` |
| `max_sfx_payload_size()` | `Cap` | `Cap::Limited(DEFAULT_MAX_SFX_PAYLOAD_SIZE)` (보고되지만 강제되지 않음) |
| `reject_unsafe_paths()` | `bool` | `false` (보고되지만 강제되지 않음) |

`max_total_size`는 추출된 모든 항목의 누적 압축 해제 크기를 제한합니다; `max_file_size`는 단일 항목을 제한합니다; `max_entry_count`는 항목 수를 제한합니다.

6개 필드 중 2개는 보고되지만 강제되지는 않습니다.

`max_sfx_payload_size()`는 구성된 값을 반환하지만 어떤 코드 경로도 이를 참조하지 않습니다. SFX 페이로드 스테이징은 컴파일 타임 `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE`(`DEFAULT_MAX_SFX_PAYLOAD_SIZE`(16 GiB)의 `pub(crate)` 별칭)에 의해 제한되며, SFX 진입점인 `Archive::open_sfx`, `Archive::open_with_sfx_progress`, `Archive::open_at_offset`은 `ExtractionLimits` 인자를 전혀 받지 않으므로 여기서 설정한 값이 스테이징 게이트에 도달할 수 없습니다. 상한을 낮추어도 SFX 열기가 강화되지 않으며 높여도 더 큰 페이로드가 허용되지 않습니다.

`reject_unsafe_paths`는 구성된 플래그를 보고하지만 엄격한 거부 추출 경로는 아직 연결되지 않았습니다: 기본값 `false`는 손실이 발생하는 경로 복구 베이스라인을 유지하고, `true`로 설정하는 것은 오늘날 추출 동작을 변경하지 않고 의도를 기록합니다. 그 배경 정책은 [추출 보안 모델](../../../explanation/user/ko/extraction-safety-model.md)을 참조하세요.

### 명명된 기본 상수

`src/security.rs`에 선언되어 있으며 `unified_archive::security::<NAME>`으로 접근할 수 있습니다.

| 상수 | 타입 | 값 | 바이트 단위 값 |
|---|---|---|---|
| `DEFAULT_MAX_TOTAL_SIZE` | `u64` | `10 * 1024 * 1024 * 1024` | 10 737 418 240 |
| `DEFAULT_MAX_FILE_SIZE` | `u64` | `1024 * 1024 * 1024` | 1 073 741 824 |
| `DEFAULT_MAX_COMPRESSION_RATIO` | `u64` | `1000` | 바이트 수가 아님; `1000:1` 비율의 분자 |
| `DEFAULT_MAX_ENTRY_COUNT` | `usize` | `100_000` | 바이트 수가 아님; 항목 개수 |
| `DEFAULT_MAX_SFX_PAYLOAD_SIZE` | `u64` | `16 * 1024 * 1024 * 1024` | 17 179 869 184 |
| `MAX_COMMENT_SIZE` | `u32` | `64 * 1024` | 65 536 |

`MAX_COMMENT_SIZE`는 `ExtractionLimits`의 일부가 아니며 `src/`의 어떤 코드에 의해서도 읽히지 않습니다. 선언된 상한일 뿐입니다: UnRAR 헤더 경로는 주석 버퍼를 작성하지 않으므로 주석 길이가 이에 비교되지 않습니다.

### `unlimited()`

`ExtractionLimits::unlimited()`는 모든 `Cap`을 `Cap::Unlimited`로 설정하고 비율을 `None`으로 설정합니다. 이것은 **`pub(crate)`**입니다: 크레이트 외부에서 호출할 수 없습니다. 이것은 센티널 숫자를 사용하던 제거된 공개 생성자의 타입화된 후속작이며, 모든 상한을 우회해야 하는 내부 호출자(각 항목의 선언된 크기에 의해 스스로 제한되는 콘텐츠 멀티셋 다이지스트)를 위해 존재합니다. 모든 게이트를 비활성화하는 단일 공개 호출은 없으며; 이를 원하는 호출자는 빌더를 통해 각 상한을 설정해야 합니다.

### `Cap`

`unified_archive::Cap`은 단일 리소스 상한선입니다. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. 두 변형: `Cap::Limited(u64)` 및 `Cap::Unlimited`. 과거 `u64::MAX` 센티널을 대체하므로 \"제한 없음\"은 극단적인 숫자가 아닌 별개의 상태입니다.

| 메서드 | 시그니처 | 동작 |
|---|---|---|
| `get` | `const fn get(self) -> u64` | 원시 상한선; `Unlimited`의 경우 `u64::MAX`. |
| `to_option` | `const fn to_option(self) -> Option<u64>` | 제한된 경우 `Some(v)`, 제한 없는 경우 `None`. |
| `as_usize` | `const fn as_usize(self) -> usize` | 32비트 타겟에서 포화(saturating) 연산되는 `usize` 상한선; `Unlimited`의 경우 `usize::MAX`. |
| `exceeded_by` | `const fn exceeded_by(self, value: u64) -> bool` | 제한된 경우 `value > v`; `Unlimited`의 경우 항상 `false`. |
| `is_unlimited` | `const fn is_unlimited(self) -> bool` | `Unlimited`인 경우에만 `true`. |

`From<u64> for Cap`은 `Cap::Limited(value)`를 생성합니다. `Cap::Unlimited`는 명시적으로 작성해야 합니다. 5개 메서드 모두 `#[must_use]`입니다.

### `CompressionRatio`

`unified_archive::CompressionRatio`는 양수 형태의 정확한 유기수 `분자 / 분모`로 유지되는 최대 압축 비율입니다. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. 두 필드 모두 비공개입니다.

| 항목 | 시그니처 | 노트 |
|---|---|---|
| `new` | `fn new(numerator: u64, denominator: u64) -> Result<Self>` | 어느 한 피연산자가 0일 때 작업명이 `extraction_limits`인 `ArchiveError::OperationBlocked`를 반환합니다. |
| `whole` | `fn whole(ratio: u64) -> Result<Self>` | `new(ratio, 1)`과 동일; 제공되는 기본값은 `1000:1`입니다. |
| `numerator` | `const fn numerator(self) -> u64` | `#[must_use]`. |
| `denominator` | `const fn denominator(self) -> u64` | `#[must_use]`. |

생성자가 진입할 수 있는 유일한 방법이고 둘 다 0을 거부하므로 유효하지 않은 비율은 존재할 수 없습니다. 게이트는 `u128`에서 `uncompressed * denominator > numerator * compressed`를 비교하므로 비교는 모든 `u64` 입력에 대해 정확합니다; 비교의 어떤 부분에도 부동소수점 값이 존재하지 않습니다. \"제한 없음\"은 극단적인 비율이 아닌 `ExtractionLimits::max_compression_ratio()`가 `None`인 것으로 표현됩니다.

비율 검사는 압축 해제 크기가 0이 아닌 상태에서 압축 크기가 0인 것을 통과시킬 정의되지 않은 비율이 아닌 위반으로 처리합니다; 둘 다 0이면 통과합니다.

### `ExtractionLimitsBuilder`

기본 제공 기본값에서 시작하는 `ExtractionLimits::builder()`에 의해 반환됩니다. `Debug` 및 `Clone`을 파생합니다. 모든 메서드는 `#[must_use]`이며 성공이 보장되고 체이닝 가능합니다; 유일한 실패 가능 단계는 사전에 `CompressionRatio`를 생성하는 과정입니다.

| 메서드 | 시그니처 | 효과 |
|---|---|---|
| `max_total_size` | `(self, cap: impl Into<Cap>) -> Self` | 누적 크기 상한을 설정합니다. |
| `max_file_size` | `(self, cap: impl Into<Cap>) -> Self` | 단일 항목 상한을 설정합니다. |
| `max_entry_count` | `(self, cap: impl Into<Cap>) -> Self` | 항목 수 상한을 설정합니다. |
| `max_compression_ratio` | `(self, ratio: CompressionRatio) -> Self` | `ratio`에서 비율 게이트를 활성화합니다. |
| `unlimited_compression_ratio` | `(self) -> Self` | 비율을 `None`으로 설정하여 해당 게이트를 비활성화합니다. |
| `max_sfx_payload_size` | `(self, cap: impl Into<Cap>) -> Self` | 스테이징된 페이로드 상한을 기록합니다; 동작이 연결되어 있지 않습니다. |
| `reject_unsafe_paths` | `(self, reject: bool) -> Self` | 플래그를 기록합니다; 동작이 연기되었습니다. |
| `build` | `(self) -> ExtractionLimits` | 완료된 값을 반환합니다. 항상 유효합니다. |

재정의되지 않은 상한선은 기본값을 유지합니다. `Cap` 설정자가 `impl Into<Cap>`을 취하므로 단순 `u64`와 `Cap::Unlimited`가 모두 허용됩니다.

## `EntryFilter`

```rust
pub type EntryFilter = Box<dyn FnMut(&ArchiveEntry) -> bool + Send>;
```

`true`를 반환하면 항목이 선택됩니다. 바운드는 `Fn`이 아닌 `FnMut`이므로 카운터, 누적기, 중복 제거 테이블과 같은 상태 저장 클로저가 내부 가변성 래퍼 없이 동작합니다. 필터는 선택적 추출 중 동기적으로 호출되므로 바운드는 `Send + Sync`가 아닌 `Send` 전용입니다; 따라서 non-`Sync` 값을 캡처하는 클로저가 허용됩니다.

일반 `Fn` 클로저는 `FnMut`를 만족하므로 직접적인 `Box::new(closure)` 호출 코드가 그대로 컴파일됩니다. `Box<dyn Fn>`은 `Box<dyn FnMut>`로 강제 변환되지 않으므로 이미 박싱된 `Fn`을 가지고 있는 호출자는 다시 박싱해야 합니다.

`entry_filter_from_fn`은 `Box::new` 절차 없이 알려진 `Fn` 클로저나 함수 포인터를 해당 타입으로 변환합니다:

```rust
pub fn entry_filter_from_fn<F>(f: F) -> EntryFilter
where
    F: Fn(&ArchiveEntry) -> bool + Send + 'static;
```

## `ProgressCallback`

```rust
pub trait ProgressCallback: Send {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> std::ops::ControlFlow<()>;
}
```

`processed`는 지금까지 처리된 바이트 수입니다; `total`은 알려진 경우 처리할 전체 바이트 수입니다. `ControlFlow::Continue(())`를 반환하면 작업이 계속되며; `ControlFlow::Break(())`는 작업을 취소합니다.

슈퍼트레이트 바운드는 `Send` 전용입니다. 콜백은 아카이브 작업당 단일 스레드에서 `&mut self`를 통해 호출되므로 `Sync`가 필요하지 않으며, `Cell`이나 `RefCell`을 캡처하는 클로저에 `Mutex` 래퍼가 필요하지 않습니다.

블랭킷 구현이 클로저를 다룹니다:

```rust
impl<F> ProgressCallback for F
where
    F: FnMut(u64, Option<u64>) -> std::ops::ControlFlow<()> + Send
```

**재진입 제약 조건.** 콜백은 `on_progress` 내부에서 열기, 목록 조회, 추출, 생성 등 아카이브 작업을 수행해서는 안 됩니다. 백엔드는 백엔드 내부 상태를 유지한 채 동기적으로 호출하며, UnRAR의 경우 프로세스 전역 UnRAR 잠금이 유지되는 동안 호출이 발생합니다. UnRAR 작업 중 동일한 스레드에서 아카이버에 다시 진입하는 것은 감지되어 거부됩니다. 그렇지 않으면 해당 비재진입 잠점이 교착 상태에 빠져 프로세스의 모든 UnRAR 작업이 멈추기 때문입니다.

## `RateLimiter`

`src/options.rs`에 정의되어 있습니다. 크레이트 루트에서 재노출되지 않습니다; 경로명은 `unified_archive::options::RateLimiter`입니다. `std::time::Instant`를 사용하여 진행률 콜백 빈도를 조절합니다.

| 항목 | 시그니처 | 동작 |
|---|---|---|
| `new` | `fn new() -> Self` | 16ms 간격, 초당 약 60회 업데이트. |
| `with_interval` | `fn with_interval(interval: Duration) -> Self` | 사용자 지정 간격. |
| `should_update` | `fn should_update(&mut self) -> bool` | 간격이 지났을 때 `true`; 새 시점을 기록합니다. |
| `should_call` | `fn should_call(&mut self) -> bool` | `should_update`의 별칭. |

`Default`는 `new()`와 동일합니다. 첫 번째 호출은 항상 `true`를 반환합니다: 마지막 업데이트 시점이 `None`으로 시작하여 \"첫 번째 호출은 통과해야 함\" 플래그로 사용되므로 `with_interval(Duration::MAX)`는 첫 번째 호출에서 기본 생성자와 동일하게 동작하며 `Instant`가 부팅 시부터 단조 증가하는 플랫폼에서 언더플로가 발생하지 않습니다.

## `StreamBound`

`src/streaming.rs`에 정의되어 있습니다. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`를 파생합니다. `Archive::extract_to_stream` 및 `Archive::extract_to_stream_with_options`가 반환된 `StreamingExtractor`를 제한하는 방식을 선택합니다. 기본값은 없으며 필수로 제공해야 하는 인자입니다.

| 필드 | 설명 |
|---|---|
| `max_file_size` | 단일 파일 최대 크기 |
| `max_total_size` | 전체 추출 최대 크기 |

추출 제한 옵션 상세 설명.

보안 임계치 적용 가이드.

## `SfxStagingProgress`

`src/options.rs`에 정의되어 있습니다. 자동 추출 아카이브를 열면 일반 백엔드가 오프셋 0에서 다시 열 수 있도록 임베드된 페이로드를 임시 파일로 복사합니다. 이 타입은 해당 복사를 관찰하고 중단할 수 있는 훅입니다. `Archive::open_with_sfx_progress`에 전달됩니다.

| 생성자 | 시그니처 | 동작 |
|---|---|---|
| `new` | `fn new(cb: impl FnMut(u64) + Send + 'static) -> Self` | 관찰 전용. 클로저는 누적 복사 바이트를 받으며 취소할 수 없습니다. |
| `with_cancel` | `fn with_cancel(cb: impl FnMut(u64) -> bool + Send + 'static) -> Self` | 클로저는 누적 복사 바이트를 받습니다; `false`를 반환하면 취소를 요청합니다. |

`with_cancel` 클로저가 `false`를 반환하면 스테이징이 중단되고 `Archive::open_with_sfx_progress`가 `ArchiveError::Cancelled { operation: "sfx_staging" }`을 반환합니다. 일부 작성된 임시 파일은 자동으로 삭제됩니다. 스테이징된 페이로드는 `ExtractionLimits` 값이 아닌 크레이트의 기본 내장 16 GiB 상한선(`DEFAULT_MAX_SFX_PAYLOAD_SIZE`, `pub(crate)` 별칭 `crate::sfx::limits::MAX_SFX_PAYLOAD_SIZE`를 통해 도달)에 의해 별도로 제한됩니다: 이 진입점은 limits 인자를 받지 않습니다.

## `Password`

`src/password.rs`에 정의되어 있습니다. 공개 시그니처에 절대 나타나지 않는 구현 세부 사항인 `secstr::SecStr`에 대한 뉴타입(newtype)입니다. `Clone`, `PartialEq`, `Eq`를 파생합니다.

| 항목 | 시그니처 | 노트 |
|---|---|---|
| `new` | `fn new(password: impl Into<String>) -> Self` | 유일한 생성자. |
| `as_str` | `fn as_str(&self) -> &str` | 실패 불가: 구조상 UTF-8 유효성이 유지됨. |

`From<String>`, `From<&str>`, `From<&String>`이 모두 구현되어 있으며 모두 `new`를 거칩니다.

생성은 `String` 또는 `&str`만 허용하므로 저장된 바이트는 항상 유효한 UTF-8입니다 — 이는 모든 래핑된 백엔드가 기대하는 바입니다. 바이트는 `Password`가 드롭될 때 0으로 지워집니다. `Debug`는 `Password(***)`를 출력하고 `Display`는 `***`를 출력합니다; 둘 다 평문을 절대 출력하지 않습니다.

## 참고 항목

- [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md) — 어떤 포맷이 어떤 작업을 수용하는지 설명합니다.
- [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md) — `OperationBlocked`, `Unsupported`, `Cancelled` 및 경고 목록입니다.
- [공개 API 표면](../../../reference/user/ko/public-api-surface.md) — 이 페이지의 각 타입에 대한 내보내기 경로입니다.
- [신뢰할 수 없는 아카이브를 안전하게 추출하는 방법](../../../how-to/user/ko/extract-untrusted-archives-safely.md) — `ExtractionLimits` 적용 방법입니다.
- [진행 상황 보고 및 작업 취소 방법](../../../how-to/user/ko/report-progress-and-cancel.md) — `ProgressCallback` 및 `RateLimiter` 적용 방법입니다.
