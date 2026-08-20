---
type: Reference
title: 에러 및 경고
description: ArchiveError의 모든 변체, 필드, Display 텍스트 및 오퍼레이션 라벨, ArchiveWarning 변체 및 ResultWithWarnings에 대한 참조 문서입니다.
tags: [api, archive, extraction, security]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/reference/user/en/errors-and-warnings.md }
synced_hash: f3396a1519038bc791079e9549e9bbb426a9d57e676db5ea0e70bcd3c3a2b9ee
---
# 에러 및 경고 (Errors and warnings)

별도로 명시하지 않는 한 이 페이지의 모든 항목은 `src/error.rs`에 존재합니다. 세 가지 타입이 에러 표면을 구성합니다: `ArchiveError` (단일 실패 타입), `ArchiveWarning` (비치명적 조건), 및 `ResultWithWarnings<T>` (경고를 동반하는 성공 값).

## `Result` 별칭

```rust
pub type Result<T> = std::result::Result<T, ArchiveError>;
```

크레이트 루트에서 재노출되므로 `unified_archive::Result<T>`와 `unified_archive::error::Result<T>`는 동일한 별칭입니다. 공개 API의 모든 오류 발생 가능한 함수는 이를 반환합니다.

## `ArchiveError`

`#[derive(Debug)]` 및 `#[non_exhaustive]`입니다. `std::fmt::Display` 및 `std::error::Error`를 구현합니다. `Clone`, `PartialEq`, `Copy`, 또는 `serde::Serialize`를 구현하지 **않으며**, `impl From<std::io::Error> for ArchiveError`가 없으므로 `?` 연산자가 `io::Error`를 `ArchiveError`로 변환하지 않습니다. `src/error.rs`에 있는 유일한 `From` 구현체는 `impl From<Operation> for String`입니다. `ArchiveError::io`는 `io::Error`를 래핑하는 생성자입니다.

Enum이 `#[non_exhaustive]`이므로 다운스트림 코드에서 이에 대해 `match` 구문을 사용할 때 와일드카드 암(wildcard arm)이 필요합니다.

선언 순서에 따른 변체 목록:

| 변체 | 필드 |
|---|---|
| `Io` | `operation: String`, `path: PathBuf`, `source: std::io::Error` |
| `Format` | `format: Option<ArchiveFormat>`, `message: String` |
| `Corruption` | `path: String`, `details: String` |
| `Password` | `message: String` |
| `Unsupported` | `operation: String`, `format: ArchiveFormat`, `details: Option<String>` |
| `CodecUnavailable` | `codec: String`, `format: ArchiveFormat`, `install_instructions: String` |
| `WriteModeOnly` | `operation: String` |
| `ReadOnlyBackend` | `operation: String` |
| `NotImplemented` | `operation: String`, `reason: String` |
| `OperationBlocked` | `operation: String`, `reason: String` |
| `InvalidPath` | `path: String`, `reason: String` |
| `Cancelled` | `operation: &'static str` |

아래 `Display` 텍스트에서 포맷을 보간하는 경우 파일 확장자가 아닌 `ArchiveFormat` 변체의 `Debug` 이름 (`Zip`, `Rar5`, `SevenZip`, `TarGzip`, `TarXz`, …)을 사용하여 `{:?}`로 보간합니다.

### `Io`

```text
I/O error during {operation}: {path} ({source})
```

`path`는 `Path::display()`로 렌더링됩니다. `operation`은 호출 지점에서 선택한 짧은 동사 — `"open"`, `"read"`, `"write"`, `"copy"`, `"flush"` 등 — 이며 아래의 `Operation` 레이블 중 하나가 아닙니다.

파일시스템 또는 디스크립터 호출이 실패하는 모든 곳에서 발생합니다: 포맷 감지 중 아카이브 열기 또는 읽기 (`src/format.rs`의 `ArchiveFormat::detect`), 추출 중 대상 디렉터리 생성 및 엔트리 데이터 쓰기, 스테이징 파일로 임베디드 SFX 페이로드 복사 (`src/archive.rs`), 및 모든 백엔드 읽기/쓰기 경로.

이 변체는 `source`를 전달하는 유일한 변체입니다.

### `Format`

```text
{format:?} format error: {message}
```

`format`이 `Some`일 때 위와 같이 출력되고,

```text
Archive format error: {message}
```

`None`일 때 위와 같이 출력됩니다.

파일을 아카이브로 전혀 식별할 수 없을 때 발생합니다 — 매직 바이트도 확장자 폴백도 일치하지 않을 때 `ArchiveFormat::detect`가 `Format { format: None, message: "Unknown archive format" }`을 반환함 — 그리고 백엔드가 식별에 성공했으나 헤더나 컨테이너 구조를 거부할 때 발생합니다.

### `Corruption`

```text
Corruption detected in '{path}': {details}
```

CRC32 검증 실패 시 발생하며, 이 때 `details`는 `CRC32 mismatch: expected {expected:08X}, got {actual:08X}`로 읽히고 (`src/security.rs`의 `security::verify_crc32_value` 참조), ZIP, libarchive, UnRAR 및 7z 백엔드에서 보고하는 엔트리 내부 디코드 실패 시 발생합니다.

`path`는 손상된 것으로 감지된 객체의 이름을 지정합니다 — 보통 아카이브 내부 엔트리 경로이지만, 몇몇 백엔드 지점은 아카이브 자체의 파일시스템 경로를 대신 보고합니다 (손상된 스트림 맵 또는 중앙 디렉터리는 특정 한 엔트리가 아닌 아카이브에 속함). 문구가 의도적으로 중립적인 이유는 그러한 이유 때문입니다; 어떤 종류의 경로가 제공되었는지 결정하기 위해 이를 파싱하지 마십시오.

ZIP 및 7z 경로는 `src/ffi/common.rs`에 있는 공유 도우미를 통해 도달합니다: `read_entry_to_memory_bounded` (및 `read_entry_to_memory_capped` 래퍼)는 선언된 엔트리 크기보다 더 많이 생성하는 디코더를 보고하고, `copy_with_optional_crc_bounded`는 권위 있는 선언 크기에 대해 짧은 디코드를 보고하며, `map_entry_read_error`는 디코더가 보고한 체크섬 실패를 `Io` 대신 `CRC32 mismatch reported by the entry decoder` 상세 정보와 함께 `Corruption`으로 변환합니다.

추출 중 CRC32 검사는 `ExtractionOptions::verify_crc32`가 설정되었을 때만 발생합니다; `Archive::validate_integrity`는 항상 검사합니다. [아카이브 무결성 검증 방법](../../../how-to/user/ko/verify-archive-integrity.md)을 참조하십시오.

### `Password`

```text
Password error: {message}
```

엔트리 또는 헤더가 암호화되었으나 암호가 제공되지 않았거나 제공된 암호가 거부되었을 때 ZIP, 7z, RAR 및 libarchive 읽기 경로에 의해 발생합니다. `message` 텍스트는 백엔드 전용입니다.

### `Unsupported`

```text
Operation '{operation}' not supported for {format:?} format: {details}
```

`details`가 `Some`일 때 위와 같이 출력되고,

```text
Operation '{operation}' not supported for {format:?} format
```

`None`일 때 위와 같이 출력됩니다 — 해당 경우에는 후미 구분 기호가 배출되지 않습니다.

요청된 작업이 감지된 포맷에 대해 구현되어 있지 않을 때 발생합니다.

### `CodecUnavailable`

```text
{codec} codec not available for {format:?} format. {install_instructions}
```

크레이트 내 어떤 코드 경로도 이 변체를 생성하지 않습니다; `ArchiveError::codec_unavailable` ([편의 생성자](#편의-생성자) 참조) 및 자체 단위 테스트에 의해서만 생성됩니다. 누락된 압축 코덱은 대신 백엔드로부터 `Format` 또는 `Io` 에러로 표출됩니다.

### `WriteModeOnly`

```text
Operation '{operation}' cannot be performed: archive is in write mode
```

`Archive::create` (또는 타입 지정 `create_*` 생성자 중 하나)가 반환한 핸들 상에서 읽기 측 호출이 이루어질 때 발생합니다. 구체적인 지점으로는 ZIP 라이터 백엔드에 대한 `Archive::has_recovery_record`, `Archive::recovery_percentage`, `Archive::is_solid`, `Archive::detect_multipart`, 및 `Archive::extract_file`이 있습니다.

### `ReadOnlyBackend`

```text
Operation '{operation}' cannot be performed: backend is read-only
```

라이터가 없는 핸들 상에서 쓰기 측 호출이 이루어질 때 발생합니다: `src/creation.rs`에 있는 `as_write` 및 `require_write_mode` 게이트는 읽기 핸들 상의 모든 `add_*` 호출을 거부합니다.

`src/archive.rs`에 있는 공유 `finalize_write_backend` 도우미도 UnRAR, 7z 및 ZIP-리더 백엔드용 `ReadOnlyBackend` 암을 가지고 있으나, 공개 API에서는 도달 불가능합니다: `Archive::finish`는 쓰기 모드에서만 도우미를 호출하며, 쓰기 모드는 ZIP-라이터 또는 libarchive 백엔드를 구축하는 `Archive::create` (및 이에 위임하는 타입 지정 `create_*` 생성자들)을 통해서만 진입합니다. 읽기 모드 핸들 상의 `finish`는 `Ok(())`를 반환합니다.

### `NotImplemented`

```text
Operation '{operation}' is not yet implemented: {reason}
```

`NotImplemented` 발생 조건 설명: 내부 `ReadBackend` 트레이트 기본 구현체에서 발생하며, 백엔드별 오버라이드 동작 방식을 안내합니다.

공개 API에 노출되지 않으며 다이제스트 순회 내부에서 소비되는 특성을 안내합니다.

### `OperationBlocked`

```text
Operation '{operation}' cannot be performed: {reason}
```

정책 거부를 위한 포괄(catch-all) 변체입니다. 타입 지정된 하위 종류는 없으며 — 구별은 `reason` 문자열에 있습니다. 다음 항목 등에 의해 발생합니다:

- `CompressionOptions::validate_for_format` (`src/options.rs`) 및 이를 통한 `Archive::create`: `ArchiveFormat::can_create`가 false인 포맷 (`format {:?} is not supported for creation via Archive::create`), `None`이 아닌 `password` (`encrypted creation for {:?} is not supported (MADR-0027); password must be None`), 및 `None`이 아닌 `split_size` (`split_size is not supported by any backend yet (DEF-002); leave it None`).
- `src/security.rs`에 있는 모든 `ExtractionLimits` 게이트: 총 크기, 단일 파일 크기, 압축률, 및 엔트리 수.
- 공유 `link_extract_blocked` 도우미를 통해 단일 엔트리 추출이 도달하는 심볼릭 및 하드 링크이며, 이때 이유(reason)는 `Entry '{path}' is a symbolic link; refusing to materialize per FR-022 link-skip policy` (또는 `hard link`)로 읽힙니다.
- `invalid_id_reason`에 의해 포맷팅되는 범위를 벗어난 엔트리 ID: `Invalid ID {id}: archive has {n} entries (valid IDs: 0-{n-1})`, 또는 비어있는 아카이브에 대한 `Invalid ID {id}: archive has 0 entries; no valid IDs`.
- 쓰기 모드 네임스페이스 충돌 (중복 경로, 파일 대 디렉터리 충돌) 및 이전 실패로 오염된 쓰기 핸들 상의 추가적인 `add_*` 호출.
- 피연산자가 0일 때의 `CompressionRatio::new` 및 `CompressionRatio::whole`.

제한 관련 이유는 제한 자체와 함께 [옵션 및 기본값](options-and-defaults.md) 및 [추출 안전 모델](../../../explanation/user/ko/extraction-safety-model.md)에 기술되어 있습니다.

### `InvalidPath`

```text
Invalid path '{path}': {reason}
```

`src/security.rs`에 있는 경로 정책 — `sanitize_entry_path` 및 `validate_archive_internal_path`가 대상을 벗어나는 순회 컴포넌트, 절대 경로, 및 심볼릭 링크 상위 경로를 거부함 — 에 의해 발생하고, 비어있는 경로에 대한 `ArchiveEntry::try_file` / `ArchiveEntry::try_dir_at` (`ArchiveEntry path must not be empty`)에 의해 발생합니다.

### `Cancelled`

```text
Operation '{operation}' was cancelled by caller
```

`operation`은 `&'static str`이므로 리터럴과 직접 비교할 수 있습니다. 호출자가 제공한 콜백이 취소를 신호할 때 발생합니다: `ControlFlow::Break(())`를 반환하는 `ProgressCallback::on_progress`, 또는 `false`를 반환하는 `SfxStagingProgress` 취소 콜백.

정확히 세 개의 레이블이 배출됩니다: `src/archive.rs`에서의 SFX 페이로드 스테이징용 `"sfx_staging"`, 백엔드 extract-all 루프용 `"extract_all"`, 및 생성 쓰기용 `"create"`. `extract_files`, `extract_by_ids` 및 `extract_some`은 공유 루프를 통해 취소되므로 `"extract_all"`을 보고합니다; 단일 엔트리 `extract_file`은 쓰기 경로에 취소 후크를 전달하지 않으므로 이 변체를 절대 생성하지 않습니다. [진행률 보고 및 작업 취소 방법](../../../how-to/user/ko/report-progress-and-cancel.md)을 참조하십시오.

## 편의 생성자

모두 `ArchiveError` 상의 연관 함수입니다. 모든 문자열 파라미터는 `impl Into<String>`을 취합니다.

| 생성자 | 시그니처 | 생성 항목 |
|---|---|---|
| `format` | `(format: Option<ArchiveFormat>, message: impl Into<String>)` | `Format` |
| `io` | `(operation: impl Into<String>, path: impl Into<PathBuf>, source: std::io::Error)` | `Io` |
| `corruption` | `(path: impl Into<String>, details: impl Into<String>)` | `Corruption` |
| `password` | `(message: impl Into<String>)` | `Password` |
| `invalid_path` | `(path: impl Into<String>, reason: impl Into<String>)` | `InvalidPath` |
| `unsupported` | `(operation: impl Into<String>, format: ArchiveFormat, details: Option<impl Into<String>>)` | `Unsupported` |
| `codec_unavailable` | `(codec: impl Into<String>, format: ArchiveFormat)` | `CodecUnavailable` |
| `write_mode_only` | `(operation: impl Into<String>)` | `WriteModeOnly` |
| `read_only_backend` | `(operation: impl Into<String>)` | `ReadOnlyBackend` |
| `not_implemented` | `(operation: impl Into<String>, reason: impl Into<String>)` | `NotImplemented` |
| `operation_blocked` | `(operation: impl Into<String>, reason: impl Into<String>)` | `OperationBlocked` |

`Cancelled`용 생성자는 없으며; 구조체 리터럴로 생성됩니다.

`unsupported`에 `None`을 전달하려면 생략된 `impl Into<String>`에 대한 구체적 타입(예: `None::<&str>`)이 필요합니다.

`codec_unavailable`은 코덱 이름과 크레이트가 컴파일된 `target_os`로부터 `install_instructions`를 직접 채웁니다. 매핑은 다음과 같이 빠짐없이 구성됩니다.

| `codec` | `macos` | `linux` | `windows` |
|---|---|---|---|
| `LZMA`, `LZMA2` | `Install p7zip: brew install p7zip` | `Install p7zip: sudo apt-get install p7zip-full (Debian/Ubuntu) or sudo yum install p7zip (RHEL/CentOS)` | `Install 7-Zip from https://www.7-zip.org/` |
| `BZIP2` | `Install bzip2: brew install bzip2` | `Install bzip2: sudo apt-get install bzip2 (Debian/Ubuntu) or sudo yum install bzip2 (RHEL/CentOS)` | `Install bzip2 from http://gnuwin32.sourceforge.net/packages/bzip2.htm` |
| `XZ` | `Install xz: brew install xz` | `Install xz: sudo apt-get install xz-utils (Debian/Ubuntu) or sudo yum install xz (RHEL/CentOS)` | `Install XZ Utils from https://tukaani.org/xz/` |

`RAR` 및 `RAR5`는 플랫폼과 무관한 단일 텍스트를 생성합니다:
`RAR/RAR5 support requires UnRAR library (already linked via FFI). If extraction fails, ensure UnRAR SDK is properly compiled.`

다른 모든 코덱 이름은 `Install {codec} codec for your platform. See libarchive documentation: https://libarchive.org/`로 폴백됩니다. macOS, Linux, 또는 Windows가 아닌 타깃에서 플랫폼은 `"unknown"`이므로, 모든 코덱 이름이 해당 폴백에 도달합니다.

## `std::error::Error::source`

```rust
fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
```

`Io`에 대해 `Some(source)`를 반환하고 다른 모든 변체에 대해 `None`을 반환합니다. `Io`는 또한 `source` 필드를 직접 노출하므로 다운캐스팅 없이 기본 `io::ErrorKind`에 접근할 수 있습니다. 해당 변체의 형태:

```rust
use unified_archive::ArchiveError;

match archive_result {
    Err(ArchiveError::Io { source, path, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
        eprintln!("missing: {}", path.display());
    }
    Err(other) => eprintln!("{other}"),
    Ok(_) => {}
}
```

## `Operation`

```rust
pub enum Operation { /* … */ }
```

`#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` 및 `#[non_exhaustive]`입니다. `Unsupported`, `OperationBlocked`, `WriteModeOnly`, `ReadOnlyBackend`, `NotImplemented`, 및 `Cancelled`의 `operation` 필드에서 사용되는 안정적인 레이블을 전달합니다.

`Display` (레이블 작성) 및 `From<Operation> for String`을 구현하므로 위 생성자 중 어느 곳에나 직접 전달할 수 있습니다. 원시 `as_str` 접근자는 크레이트 내부 전용입니다: 크레이트 외부에서는 `Display` 또는 `to_string()`을 사용하십시오.

24개 변체와 해당 레이블은 지칭하는 작업 제품군별로 다음과 같이 그룹화됩니다:

**추출 (Extraction)**

| 변체 | 레이블 | 명칭 |
|---|---|---|
| `Extract` | `extract` | 특정 추출 경로가 식별되기 전, 공유 엔트리별 안전성 검사 |
| `ExtractAll` | `extract_all` | `Archive::extract_all` |
| `ExtractFile` | `extract_file` | `Archive::extract_file` |
| `ExtractToMemory` | `extract_to_memory` | `Archive::extract_to_memory`, `extract_to_memory_with_options` |
| `ExtractToStream` | `extract_to_stream` | `Archive::extract_to_stream`, `extract_to_stream_with_options` |
| `ExtractFiles` | `extract_files` | `Archive::extract_files` |
| `ExtractByIds` | `extract_by_ids` | `Archive::extract_by_ids` |
| `ExtractSome` | `extract_some` | `Archive::extract_some`, `extract_filtered` |

**조사 (Inspection)**

| 변체 | 레이블 | 명칭 |
|---|---|---|
| `ListFiles` | `list_files` | `Archive::list_files` |
| `ListFilesForLimits` | `list_files_for_limits` | `Archive::list_files_for_limits` |
| `ValidateIntegrity` | `validate_integrity` | `Archive::validate_integrity` |

**수정 (Modification)**

| 변체 | 레이블 | 명칭 |
|---|---|---|
| `Modify` | `modify` | `Archive::modify`, `modify_with_options` |
| `AddEntry` | `add_entry` | `Archive::add_entry` |
| `RemoveEntry` | `remove_entry` | `Archive::remove_entry`, `remove_entry_by_id`, `replace_entry` |
| `CommitChanges` | `commit_changes` | `Archive::commit_changes` |
| `PendingOperations` | `pending_operations` | `Archive::pending_operations` |
| `ClearOperations` | `clear_operations` | `Archive::clear_operations` |
| `AddDirectoryEntry` | `add_directory_entry` | `Archive::add_directory_entry` |

**생성 (Creation)**

| 변체 | 레이블 | 명칭 |
|---|---|---|
| `AddFileFromData` | `add_file_from_data` | `Archive::add_file_from_data` |
| `AddFileFromPathAs` | `add_file_from_path_as` | `Archive::add_file_from_path_as` |
| `AddDirectory` | `add_directory` | `Archive::add_directory` |
| `AddDirectoryRecursive` | `add_directory_recursive` | `Archive::add_directory_recursive` |
| `Create` | `create` | `Archive::create` |

**수명주기 (Lifecycle)**

| 변체 | 레이블 | 명칭 |
|---|---|---|
| `Finish` | `finish` | `Archive::finish`, `Archive::close` |

Enum이 `#[non_exhaustive]`이므로 다운스트림 매칭 시 와일드카드 암이 필요합니다.

## `error::ops` 상소들

`ops` 모듈은 `pub(crate) mod ops`로 선언되어 있으므로, 이 상수들은 크레이트 외부에서 도달할 수 **없습니다**. 마이그레이션 윈도 동안 `Operation::as_str`에 대한 `const` 뷰로 존재합니다; 동일 정보의 공개 형태는 위의 `Operation` enum입니다. 세트는 `Operation` 변체당 정확히 하나의 상수이며 동일 레이블을 갖습니다:

- 추출: `EXTRACT`, `EXTRACT_ALL`, `EXTRACT_FILE`, `EXTRACT_TO_MEMORY`, `EXTRACT_TO_STREAM`, `EXTRACT_FILES`, `EXTRACT_BY_IDS`, `EXTRACT_SOME`
- 조사: `LIST_FILES`, `LIST_FILES_FOR_LIMITS`, `VALIDATE_INTEGRITY`
- 수정: `MODIFY`, `ADD_ENTRY`, `REMOVE_ENTRY`, `COMMIT_CHANGES`, `PENDING_OPERATIONS`, `CLEAR_OPERATIONS`, `ADD_DIRECTORY_ENTRY`
- 생성: `ADD_FILE_FROM_DATA`, `ADD_FILE_FROM_PATH_AS`, `ADD_DIRECTORY`, `ADD_DIRECTORY_RECURSIVE`, `CREATE`
- 수명주기: `FINISH`

이 레이블들은 `Display` 텍스트 및 `operation` 필드 내부에 나타나는 항목입니다. `ops` 모듈이 `pub(crate)`이므로 `Operation`이 생성하는 문자열이 크레이트 외부에서 도달 가능한 해당 레이블들의 유일한 형태입니다.

## `ArchiveWarning`

```rust
pub enum ArchiveWarning { /* … */ }
```

`#[derive(Debug, Clone, PartialEq, Eq)]` 및 `#[non_exhaustive]`입니다. `Display`를 구현합니다. 경고는 비치명적입니다: 작업이 계속되어 완료됩니다.

### `SkippedSymlink`

필드: `path: String`, `target: Option<String>`.

```text
Skipped symbolic link '{path}' -> '{target}' (FR-022: cross-platform symlink support not reliable)
```

`target`이 `Some`일 때 위와 같이 출력되고,

```text
Skipped symbolic link '{path}' (FR-022: cross-platform symlink support not reliable)
```

`None`일 때 위와 같이 출력됩니다.

심볼릭 링크 엔트리에 도달했을 때 libarchive, ZIP, 7z 및 UnRAR 추출 경로에 의해 배출되며, 리스팅을 조사할 때 `Archive::check_symlinks`에 의해 배출됩니다.

### `SkippedHardLink`

필드: `path: String`.

```text
Skipped hard link '{path}' (FR-022: limited cross-platform support)
```

libarchive 및 UnRAR 추출 경로에 의해 배출되며 `Archive::check_symlinks`에 의해 배출됩니다.

### `OutputPathCaseCollision`

필드: `first: String` (출력 경로를 먼저 선점한 엔트리), `second: String` (충돌하는 이후 엔트리).

```text
Entries '{first}' and '{second}' differ only by case and merge on case-insensitive destination filesystems (R0079-0036)
```

`src/extraction.rs`에 있는 추출 사전 검사에 의해 배출되며, 해석된 각 출력 경로를 유니코드 소문자화로 접어 비교합니다. 이 비교는 유니코드 정규화 형태 충돌(NFC 대 NFD)을 감지하지 않습니다. 대상 파일시스템의 대소문자 구별 여부를 포터블하게 확인할 수 없으므로 에러가 아닌 경고입니다.

## `ResultWithWarnings<T>`

```rust
pub struct ResultWithWarnings<T> {
    pub value: T,
    pub warnings: Vec<ArchiveWarning>,
}
```

`#[derive(Debug, Clone, PartialEq, Eq)]`입니다. 경고를 노출할 수 있는 다중 엔트리 추출 진입점(`extract_all`, `extract_files`, `extract_by_ids`, `extract_some`)에 의해 성공 시 반환됩니다.

| 진입점 | 반환 타입 |
|---|---|
| `Archive::extract_all` | `Result<ResultWithWarnings<()>>` |
| `Archive::extract_files` | `Result<ResultWithWarnings<()>>` |
| `Archive::extract_by_ids` | `Result<ResultWithWarnings<()>>` |
| `Archive::extract_some` | `Result<ResultWithWarnings<()>>` |
| `Archive::extract_filtered` | `Result<ResultWithWarnings<()>>` |

다른 모든 추출 진입점은 `Result<()>`, `Result<Vec<u8>>`, 또는 `Result<StreamingExtractor>`를 반환하므로 경고를 노출하지 않습니다 — [올바른 추출 호출을 선택하는 방법](../../../how-to/user/ko/choose-an-extraction-api.md)을 참조하십시오.

비어있는 `warnings` 벡터는 생략된 것이 없음을 의미합니다; 비어있지 않은 벡터는 추출이 성공했으나 일부 엔트리가 구체화되지 않았음을 의미합니다:

```rust
let result = archive.extract_all(options)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```
