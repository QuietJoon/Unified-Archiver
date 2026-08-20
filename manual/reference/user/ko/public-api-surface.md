---
type: Reference
title: 공개 API 표면
description: unified-archive가 내보내는 모든 요소 인덱스 - 크레이트 루트 재노출, 공개 모듈, 모드별 공개 Archive 메서드 및 v2 타입 핸들입니다.
tags: [api, archive, extraction, creation, modification]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/reference/user/en/public-api-surface.md }
synced_hash: bcec1c69e4bc5d64f95feb34a1e3a2cdd0005fdd01e46587570a50c435ffb8d4
---
# 퍼블릭 API 표면

`unified-archive` 0.4.0 버전의 공개 API 표면 색인 안내 문서입니다.

Rustdoc은 모든 퍼블릭 항목을 다룹니다. `v2` 모듈은 `v2-api` 기능이 활성화되었을 때만 Rustdoc에 표시됩니다.

## `#[non_exhaustive]` 타입

`#[non_exhaustive]` 속성이 부여된 공개 타입 목록 및 match 분기 시 와일드카드 필요성 안내.

`CompressionOptions`는 의도적으로 지정되지 않았으므로 구조체 리터럴 형태가 0.3 내내 소스 호환성을 유지합니다.

## 크레이트 루트 재노출

영역별로 그룹화된 `use unified_archive::…;`가 해석하는 경로들입니다. 선언문은 `src/lib.rs`에 있습니다.

### 아카이브 핸들

| Item | Kind |
|---|---|
| `Archive` | struct — 읽기, 쓰기 및 수정 세션을 위한 단일 핸들 타입 |

`Archive`는 `Send`이지만 의도적으로 `Sync`가 아닙니다: 스레드당 하나의 핸들. `Send` 요구사항은 `src/archive.rs`의 컴파일 타임 단언(assertion)에 의해 고정됩니다.

### 엔트리

| Item | Kind |
|---|---|
| `ArchiveEntry` | struct — 엔트리별 메타데이터 |
| `ArchiveEntryBuilder` | struct — `ArchiveEntry::file` / `dir_at`이 반환하는 플루언트 빌더 |
| `EntryType` | enum — `File`, `Directory`, `Symlink`, `HardLink`, `Other`; `#[non_exhaustive]` |
| `FileAttributes` | struct — `windows: Option<u32>`, `unix_xattr: Option<Vec<(String, Vec<u8>)>>`, `archive_specific: Option<String>`; `#[non_exhaustive]` |

`ArchiveEntry` 퍼블릭 필드: `path: String`, `size: Option<u64>`, `compressed_size: Option<u64>`, `modified: Option<SystemTime>`, `crc32: Option<u32>`, `entry_type: EntryType`, `permissions: Option<u32>`, `created: Option<SystemTime>`, `accessed: Option<SystemTime>`, `is_encrypted: bool`, `comment: Option<String>`, `attributes: Option<FileAttributes>`, `raw_path: Option<Vec<u8>>`, `link_target: Option<String>`, `id: usize`.

`ArchiveEntry` 연관 함수 및 메서드:

| Signature | Description |
|---|---|
| `fn file(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder` | 파일 타입 빌더 시작 (경로 검증 안 함) |
| `fn try_file(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder, ArchiveError>` | 동일하며, 빈 경로를 거부함 |
| `fn dir_at(path: impl Into<String>, id: usize) -> ArchiveEntryBuilder` | 디렉토리 타입 빌더 시작 |
| `fn try_dir_at(path: impl Into<String>, id: usize) -> Result<ArchiveEntryBuilder, ArchiveError>` | 동일하며, 빈 경로를 거부함 |
| `fn new(path: String, id: usize) -> Self` | 파일 타입 엔트리 직접 생성 (v0.4에서 사용 중단 예정) |
| `fn directory(path: String, id: usize) -> Self` | 디렉토리 타입 엔트리 직접 생성 (v0.4에서 사용 중단 예정) |
| `fn symlink(path: String, id: usize, target: String) -> Self` | `link_target`이 설정된 심볼릭 링크 타입 엔트리 |
| `fn compression_fraction(&self) -> Option<f64>` | 압축된 크기를 압축 해제된 크기로 나눈 값 |
| `fn expansion_ratio(&self) -> Option<f64>` | 압축 해제된 크기를 압축된 크기로 나눈 값 |
| `fn compression_ratio(&self) -> Option<f64>` | 기존 압축률 접근자 |
| `fn is_directory(&self) -> bool` | `entry_type == EntryType::Directory` |
| `fn is_file(&self) -> bool` | `entry_type == EntryType::File` |
| `fn is_symlink(&self) -> bool` | `entry_type == EntryType::Symlink` |
| `fn is_hardlink(&self) -> bool` | `entry_type == EntryType::HardLink` |

`ArchiveEntryBuilder` 설정자(setter)들 — 각각 `self`를 받아 반환함: `size(u64)`, `compressed_size(u64)`, `modified(SystemTime)`, `created(SystemTime)`, `accessed(SystemTime)`, `crc32(u32)`, `permissions(u32)`, `raw_path(Vec<u8>)`, `link_target(String)`, `comment(String)`, `encrypted(bool)`, `attributes(FileAttributes)`; `build(self) -> ArchiveEntry`로 종결됩니다.

### 포맷

| Item | Kind |
|---|---|
| `ArchiveFormat` | enum — 18개 변리언트: `SevenZip`, `Zip`, `Rar`, `Rar5`, `Tar`, `TarGzip`, `TarBzip2`, `TarXz`, `TarZst`, `TarLz4`, `TarLzma`, `Gzip`, `Bzip2`, `Xz`, `Zst`, `Lz4`, `Lzma`, `Iso`; `#[non_exhaustive]` |
| `FormatCapabilities` | struct — `encryption_read`, `encryption_write`, `multipart_read`, `multipart_write`, `modification`, `compression_read`, `compression_write` (각각 `Support` 타입); `#[non_exhaustive]` |
| `Support` | enum — `Full`, `Partial`, `None`; `#[non_exhaustive]` |

`ArchiveFormat` 퍼블릭 메서드:

| Signature | Description |
|---|---|
| `fn detect(path: &Path) -> Result<Self>` | 매직 바이트로 파일을 식별하며, 고정된 포맷 세트에 대해 확장자 폴백 사용 |
| `fn detect_from_bytes(magic: &[u8]) -> Result<Self>` | 매직 바이트 접두사만으로 식별 |
| `fn capabilities(&self) -> FormatCapabilities` | 작업별 지원 기록 |
| `fn supports_compression(&self) -> bool` | 어느 방향으로든 압축 지원 여부 |
| `fn supports_compression_read(&self) -> bool` | 읽기 측 압축 지원 여부 |
| `fn supports_compression_write(&self) -> bool` | 쓰기 측 압축 지원 여부 |
| `fn supports_encryption_read(&self) -> bool` | 암호화된 아카이브 읽기 가능 여부 |
| `fn supports_encryption_write(&self) -> bool` | 암호화된 아카이브 쓰기 가능 여부 |
| `fn supports_encryption(&self) -> bool` | 어느 방향으로든 암호화 지원 여부 |
| `fn supports_multipart_read(&self) -> bool` | 분할 볼륨 읽기 가능 여부 |
| `fn supports_multipart_write(&self) -> bool` | 분할 볼륨 쓰기 가능 여부 |
| `fn supports_multipart(&self) -> bool` | 어느 방향으로든 분할 볼륨 지원 여부 |
| `fn can_modify(&self) -> bool` | `Archive::modify` 수행 가능 여부 |
| `fn can_create(self) -> bool` | `Archive::create` 수행 가능 여부 (생성에 대한 결정적인 게이트) |
| `fn extensions(&self) -> &[&str]` | 포맷과 관련된 파일 확장자 목록 |

`FormatCapabilities::compression(self) -> Support`는 읽기 및 쓰기 압축 필드를 하나의 값으로 병합합니다.

위의 변리언트 목록은 열거형 자체의 선언 순서입니다. `src/lib.rs`의 크레이트 문서 테이블은 동일한 18개 포맷을 다른 순서로 나열합니다. 각 작업이 수용하는 포맷은 [포맷 지원 매트릭스](format-support-matrix.md)에 도표화되어 있습니다.

### 옵션

| Item | Kind |
|---|---|
| `ExtractionOptions` | struct — 대상 디렉토리, 비밀번호, 덮어쓰기, 권한/시간 보존, CRC 검증, 제한 사항, 필터, 진행 상황 |
| `CompressionOptions` | struct — 포맷, 압축 레벨, 비밀번호, 분할 크기, 진행 상황 |
| `CompressionLevel` | enum — `Store`, `Fastest`, `Fast`, `Normal`, `Maximum`, `Ultra` |
| `ZipCompressionOptions` | struct — `Archive::create_zip`과 페어링되는 타입화된 빌더 |
| `SevenZCompressionOptions` | struct — `Archive::create_seven_zip`과 페어링되는 타입화된 빌더 |
| `LibarchiveCompressionOptions` | struct — `Archive::create_libarchive`와 페어링되는 타입화된 빌더 |
| `EntryFilter` | type alias — `Box<dyn FnMut(&ArchiveEntry) -> bool + Send>` |
| `entry_filter_from_fn` | function — `fn entry_filter_from_fn<F>(f: F) -> EntryFilter where F: Fn(&ArchiveEntry) -> bool + Send + 'static` |
| `ProgressCallback` | trait — `pub trait ProgressCallback: Send`, `fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()>` 포함; `F: FnMut(u64, Option<u64>) -> ControlFlow<()> + Send` 조건의 `F`에 대해 일괄 구현됨 |
| `SfxStagingProgress` | struct — `new(impl FnMut(u64) + Send + 'static)`, `with_cancel(impl FnMut(u64) -> bool + Send + 'static)` |

`entry_filter_from_fn`은 반환하는 `EntryFilter` 별칭이 박스형 `FnMut`이더라도 `FnMut`이 아닌 `Fn`을 받습니다. 캡처된 상태를 변경해야 하는 클로저는 직접 박싱해야 합니다.

이 타입들의 메서드:

- `ExtractionOptions`: `password(mut self, password: impl Into<String>) -> Self` 및 `Default`. 다른 모든 필드는 public이며 직접 또는 구조체 업데이트 구문으로 설정됩니다.
- `CompressionOptions`: `new(format: ArchiveFormat) -> Self`, `format(&self) -> ArchiveFormat`, `password(mut self, password: impl Into<String>) -> Self`, `strip_progress(&self) -> Self` (복제할 수 없는 진행 상황 콜백을 제거한 복사본), `validate_for_format(&self) -> Result<()>`, 그리고 `Default`와 비밀번호를 생략하는 직접 작성된 `Debug`. `Clone`을 구현하지 않습니다.
- `ZipCompressionOptions` 및 `SevenZCompressionOptions`: `new() -> Self`, `level(mut self, l: CompressionLevel) -> Self`, `progress(mut self, p: Box<dyn ProgressCallback>) -> Self`, `Default`, 및 `Into<CompressionOptions>`. 둘 다 비밀번호 설정자를 노출하지 않습니다.
- `LibarchiveCompressionOptions`: `new(format: ArchiveFormat) -> Self`, 동일한 `level` 및 `progress` 설정자, 그리고 `Into<CompressionOptions>`. 포맷이 필수이므로 `Default`가 없습니다.

모든 필드, 기본값 및 거부 규칙: [옵션 및 기본값](options-and-defaults.md).

### 보안

| Item | Kind |
|---|---|
| `ExtractionLimits` | struct — 추출 자원 상한선; 필드는 private이며 접근자를 통해 읽음 |
| `ExtractionLimitsBuilder` | struct — `ExtractionLimits::builder()`가 반환하는 빌더 |
| `Cap` | enum — `Limited(u64)`, `Unlimited`; `From<u64>` |
| `CompressionRatio` | struct — 검증된 분자/분모 쌍 |

`ExtractionLimits` 접근자: `max_total_size() -> Cap`, `max_file_size() -> Cap`, `max_compression_ratio() -> Option<CompressionRatio>`, `max_entry_count() -> Cap`, `max_sfx_payload_size() -> Cap`, `reject_unsafe_paths() -> bool`, 그리고 `builder() -> ExtractionLimitsBuilder`.

`ExtractionLimitsBuilder` 메서드: `max_total_size(impl Into<Cap>)`, `max_file_size(impl Into<Cap>)`, `max_entry_count(impl Into<Cap>)`, `max_compression_ratio(CompressionRatio)`, `unlimited_compression_ratio()`, `max_sfx_payload_size(impl Into<Cap>)`, `reject_unsafe_paths(bool)`, 그리고 `build() -> ExtractionLimits`.

`Cap` 메서드: `get() -> u64`, `to_option() -> Option<u64>`, `as_usize() -> usize`, `exceeded_by(u64) -> bool`, `is_unlimited() -> bool` — 모두 `const`입니다.

`CompressionRatio`: `new(numerator: u64, denominator: u64) -> Result<Self>`, `whole(ratio: u64) -> Result<Self>`, `numerator() -> u64`, `denominator() -> u64`.

원시 정책 조각들(`check_extraction_safe`, `check_single_entry_safe`, `sanitize_entry_path`, `verify_crc32` 및 형제 함수들)은 `pub(crate)`이며 퍼블릭 표면의 일부가 아닙니다. 파사드(facade)가 이들의 불변성을 확립합니다. [추출 안전 모델](../../../explanation/user/ko/extraction-safety-model.md)을 참조하세요.

### SFX

| Item | Kind |
|---|---|
| `SfxDetectionResult` | struct — private 필드, 접근자를 통해 읽음; `#[non_exhaustive]` |
| `SfxConfidence` | enum — `NotSfx`, `Probable`, `Confirmed` |
| `StubType` | enum — `WindowsPE`, `LinuxELF`, `MacOSMachO`, `ScriptInterpreter`, `Unknown` |

`SfxDetectionResult` 접근자: `is_sfx() -> bool`, `archive_format() -> Option<ArchiveFormat>`, `data_offset() -> Option<u64>`, `stub_type() -> Option<StubType>`, `confidence() -> SfxConfidence`, `is_probable() -> bool`, `is_confirmed() -> bool`, `evidence() -> &[String]`, `payload_coordinates() -> Option<(ArchiveFormat, u64, StubType)>`, `summary() -> String`. 퍼블릭 생성자: `not_sfx()` 및 `probable(StubType, ArchiveFormat, u64, Vec<String>)`. `Default`는 `not_sfx()`를 생성합니다.

`src/sfx/result.rs`는 또한 `detected(StubType, ArchiveFormat, u64)`를 선언하지만, 이는 `#[cfg(test)] pub(crate)`로 지정되어 있어 테스트 빌드에서만 컴파일되고 크레이트 전용이므로 크레이트 외부에서는 호출할 수 없습니다. 이것이 `SfxConfidence::Confirmed`를 설정하는 유일한 생성자이므로 어떤 퍼블릭 API도 `Confirmed` 결과를 생성하지 않습니다. [SFX 감지 판정 방식](../../../explanation/developer/ko/sfx-detection-pipeline.md)을 참조하세요.

`StubType`: `detect(bytes: &[u8]) -> StubType`, `description() -> &'static str`, `is_native() -> bool`, `is_known() -> bool`.

### 스트림 체크섬

| Item | Kind |
|---|---|
| `StreamChecksum` | struct — `crc32: Option<u32>`, `crc64: Option<u64>`, `uncompressed_size: Option<u64>`, `check_type: CheckType` |
| `CheckType` | enum — `None`, `Crc32`, `Crc64`, `Sha256`, `Unknown` |
| `extract_gzip_stream_crc` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_bzip2_stream_crc` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_xz_stream_check` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` |
| `extract_stream_checksum` | `fn(path: impl AsRef<Path>) -> Result<StreamChecksum>` — 매직 바이트에 따라 디스패치됨 |

`StreamChecksum` 메서드: `crc32_value() -> Option<u32>` 및 `crc64_value() -> Option<u64>`, 각각 `check_type`이 일치할 때만 값을 반환합니다.

### 스트리밍

| Item | Kind |
|---|---|
| `StreamBound` | enum — `DeclaredSize`, `Cap(u64)`, `Unbounded` |
| `StreamingExtractor` | struct (`std::io::Read` 구현) |

`StreamingExtractor` 메서드: `total_size() -> Option<u64>`, `bytes_read() -> u64`, `progress() -> Option<f64>`, `take_bounded(self, fallback: u64) -> std::io::Take<Self>`. 생성자는 크레이트 전용이며 인스턴스는 `Archive::extract_to_stream` 및 `Archive::extract_to_stream_with_options`에서 반환됩니다. [스트리밍 및 메모리 동작 방식](../../../explanation/developer/ko/streaming-and-memory.md)을 참조하세요.

### 오류

| Item | Kind |
|---|---|
| `ArchiveError` | enum — 크레이트의 단일 에러 타입; `#[non_exhaustive]` |
| `Operation` | enum — 에러 필드에 포함되는 안정적인 작업 라벨; `#[non_exhaustive]` |
| `Result` | type alias — `Result<T> = std::result::Result<T, ArchiveError>` |

`ArchiveWarning`(마찬가지로 `#[non_exhaustive]`) 및 `ResultWithWarnings<T>`는 퍼블릭 시그니처에 나타남에도 불구하고 크레이트 루트에서 재노출되지 **않습니다**. 퍼블릭 `error` 모듈을 통해 접근하세요: `unified_archive::error::ArchiveWarning`, `unified_archive::error::ResultWithWarnings`. 상세 내용: [오류 및 경고](errors-and-warnings.md).

### 비밀번호

| Item | Kind |
|---|---|
| `Password` | struct — 비밀 문자열을 감쌈; `Debug` 및 `Display` 모두 재액팅(마스킹) 처리됨 |

`Password::new(impl Into<String>)`, `Password::as_str(&self) -> &str`, 그리고 `From<String>`, `From<&str>`, `From<&String>`.

### 기타 재노출 항목

| Item | Declared in | Kind |
|---|---|---|
| `MultipartLayout` | `src/inspection.rs` | enum — `Single { path: PathBuf }`, `Multi { parts: Vec<PathBuf> }` |
| `ValidationReport` | `src/inspection.rs` | struct — `total_entries: usize`, `total_files: usize`, `validated: usize`, `failed: Vec<String>` |
| `ModificationOptions` | `src/modification.rs` | struct — 4개의 pub 필드 및 `new()`, `with_backup(&str)`, `without_metadata_preservation()`, `Default` |

`inspection`과 `modification`은 `pub(crate)` 모듈이므로, 이 세 타입은 크레이트 루트 재노출을 통해서만 도달할 수 있습니다.

`ModificationOptions` 퍼블릭 필드 및 `new()`. (따라서 `Default`)가 설정하는 값:

| Field | Type | `new()` value |
|---|---|---|
| `preserve_metadata` | `bool` | `true` |
| `create_backup` | `bool` | `false` |
| `backup_suffix` | `String` | `".bak"` |
| `compression` | `Option<CompressionOptions>` | `None` |

타입에 `#[non_exhaustive]`가 없으므로 네 필드 모두 직접 또는 구조체 업데이트 구문으로 할당할 수 있습니다. `with_backup`은 `create_backup`을 `true`로, `backup_suffix`를 해당 인자로 설정하며, 인자가 비어 있거나 `/` 또는 `\`를 포함하면 `".bak"`으로 폴백합니다. 호출자가 빌더를 거치지 않고 필드를 직접 할당할 수 있으므로 `commit_changes`는 `backup_suffix`를 재검증합니다.

## 퍼블릭 모듈

| Module | Gate | Contents |
|---|---|---|
| `archive` | — | `Archive`; `v2-api`가 켜져 있을 때 `archive::mode_split`. `ArchiveMode` 및 `ArchiveBackend`는 `pub(crate)`. |
| `entry` | — | `ArchiveEntry`, `ArchiveEntryBuilder`, `EntryType`, `FileAttributes` |
| `error` | — | `ArchiveError`, `Operation`, `Result`, `ArchiveWarning`, `ResultWithWarnings`. `ops` 라벨 상수는 `pub(crate)`. |
| `format` | — | `ArchiveFormat`, `FormatCapabilities`, `Support`, 및 `format_from_extension(path: &Path) -> Option<ArchiveFormat>` |
| `options` | — | 위의 모든 옵션 타입, 및 `RateLimiter` (`new()`, `with_interval(Duration)`, `should_update()`, `should_call()`, `Default`) |
| `password` | — | `Password` |
| `security` | — | 위의 제한 타입들, 및 `pub` 상수들 `DEFAULT_MAX_TOTAL_SIZE`, `DEFAULT_MAX_FILE_SIZE`, `DEFAULT_MAX_COMPRESSION_RATIO`, `DEFAULT_MAX_ENTRY_COUNT`, `DEFAULT_MAX_SFX_PAYLOAD_SIZE`, `MAX_COMMENT_SIZE` |
| `sfx` | — | 서브모듈 `detection`, `result`, `stub_types`; `detect_sfx`, `SfxConfidence`, `SfxDetectionResult`, `StubType` 재노출. `sfx::limits` 및 `sfx::signatures`는 `pub(crate)`. |
| `stream_crc` | — | `StreamChecksum`, `CheckType`, 및 4개의 추출 함수 |
| `streaming` | — | `StreamBound`, `StreamingExtractor` |
| `v2` | `feature = "v2-api"` | `ReadArchive`, `WriteArchive`, `ModifyArchive` |
| `external` | `target_os = "windows"` **and** `feature = "external-rar-create"` | `RarCreator` — WinRAR 커맨드라인 브릿지 |
| `ffi` | `#[doc(hidden)]` | 지원 표면에 포함되지 **않음** (아래 참조) |

`backend`, `creation`, `extraction`, `fs_identity`, `inspection`, `modification`은 `pub(crate)` 모듈입니다: 이들의 `Archive` 메서드는 public이지만 모듈 자체는 아닙니다.

`sfx::detect_sfx`는 감지기의 독립형 함수 형태이며, `fn detect_sfx<P: AsRef<Path>>(path: P) -> Result<SfxDetectionResult>`입니다. `Archive::detect_sfx`가 이 함수로 위임합니다.

`external::RarCreator` 메서드: `new<P: AsRef<Path>>(output_path: P) -> Result<Self>`, `with_rar_exe_path<P, R>(output_path: P, rar_exe_path: R) -> Result<Self>`, `set_compression_level(&mut self, level: CompressionLevel)`, `set_password(&mut self, password: impl Into<String>)`, `add_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()>`, `add_directory<P: AsRef<Path>>(&mut self, path: P) -> Result<()>`, `entry_count(&self) -> usize`, `create(self) -> Result<()>`, `rar_exe_path(&self) -> &Path`. 기능 상세 내용: [Cargo 기능 및 MSRV](../../developer/ko/cargo-features.md).

### `ffi` 모듈

`ffi`는 `#[doc(hidden)] pub mod ffi`로 선언되어 있습니다. `tests/` 하위의 통합 테스트(비 테스트 빌드에 연결됨)가 백엔드 타입에 직접 도달할 수 있도록 하기 위해서만 public으로 제공됩니다. Rustdoc에서는 생략됩니다. 내부의 그 어떤 항목도 크레이트의 호환성 약속 대상이 아닙니다. 동일한 기능의 지원되는 형태는 크레이트 루트 재노출 항목과 위에 나열된 `Archive` 메서드입니다.

## `Archive` 메서드

아래의 모든 메서드는 `Archive`에 존재합니다. 시그니처는 소스에 작성된 대로이며, 읽기에 도움이 되는 경우 `Self`를 명시했습니다. `Result<T>`는 크레이트 별칭입니다.

### 열기 및 감지

| Signature | Description |
|---|---|
| `fn open(path: impl AsRef<Path>) -> Result<Self>` | 포맷을 감지하고 읽기 핸들을 염 |
| `fn open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>) -> Result<Self>` | 암호화된 콘텐츠를 위해 비밀번호와 함께 읽기 핸들을 염 |
| `fn open_at_offset(path: impl AsRef<Path>, offset: u64) -> Result<Self>` | 바이트 오프셋에 임베딩된 아카이브를 열고 페이로드를 임시 파일에 스테이징함 |
| `fn open_sfx(path: impl AsRef<Path>) -> Result<Self>` | 자가 추출 아카이브를 감지하고 임베딩된 페이로드를 염 |
| `fn open_with_sfx_progress(path: impl AsRef<Path>, progress: Option<SfxStagingProgress>) -> Result<Self>` | 동일하며, 페이로드 스테이징 진행 상황을 보고하고 선택적으로 취소함 |
| `fn detect_sfx(path: impl AsRef<Path>) -> Result<SfxDetectionResult>` | 페이로드를 열지 않고 SFX 감지 파이프라인을 실행함 |
| `fn extract_stub(path: impl AsRef<Path>, detection: &SfxDetectionResult) -> Result<Vec<u8>>` | 감지된 페이로드 앞에 있는 실행 가능한 스텁 바이트를 읽음 |

관련 가이드: [자가 추출 아카이브 감지 및 열기 방법](../../../how-to/user/ko/handle-self-extracting-archives.md) 및 [비밀번호로 보호된 아카이브 열기 방법](../../../how-to/user/ko/open-password-protected-archives.md).

### 핸들 메타데이터

| Signature | Description |
|---|---|
| `fn format(&self) -> ArchiveFormat` | 핸들이 열린 포맷 |
| `fn extension_format(&self) -> Option<ArchiveFormat>` | 파일 확장자가 시사하는 포맷 (있는 경우) |
| `fn path(&self) -> &Path` | 호출자용 경로 (SFX 핸들의 경우 외부 실행 파일 경로 유지) |
| `fn is_encrypted(&self) -> Result<bool>` | 아카이브가 암호화된 콘텐츠를 보고하는지 여부 |
| `fn has_recovery_record(&self) -> Result<bool>` | RAR 복구 레코드 존재 여부 (쓰기 핸들에서는 `WriteModeOnly`) |
| `fn recovery_percentage(&self) -> Result<Option<u8>>` | RAR 복구 레코드 크기 비율 (%) (쓰기 핸들에서는 `WriteModeOnly`) |
| `fn is_solid(&self) -> Result<bool>` | 아카이브가 솔리드 압축되었는지 여부 (쓰기 핸들에서는 `WriteModeOnly`) |

### 검사

| Signature | Description |
|---|---|
| `fn list_files(&self) -> Result<&[ArchiveEntry]>` | 첫 호출 후 핸들에 캐싱되는 전체 목록 |
| `fn list_files_for_limits(&self) -> Result<Vec<ArchiveEntry>>` | 핸들 캐시를 채우지 않는 소유형 목록 |
| `fn entry_count(&self) -> Result<usize>` | 읽기/수정 모드에서는 나열된 엔트리 수, 쓰기 모드에서는 이미 작성된 엔트리 수 |
| `fn find_entry(&self, path: &str) -> Result<Option<ArchiveEntry>>` | 경로가 정확히 일치하는 첫 번째 엔트리 |
| `fn find_entries(&self, path: &str) -> Result<Vec<ArchiveEntry>>` | 중복을 허용하는 아카이브에서 해당 경로의 모든 엔트리 |
| `fn validate_integrity(&self) -> Result<ValidationReport>` | 엔트리 체크섬을 검증하고 총계 및 실패 항목을 보고 |
| `fn calculate_archive_crc(&self) -> Result<u32>` | 엔트리들의 CRC32 값들의 래핑 합(wrapping sum) |
| `fn calculate_content_multiset_digest_and_size(&self) -> Result<(String, u64)>` | 한 번의 탐색으로 콘텐츠 식별 다이제스트와 전체 압축 해제 크기 계산 |
| `fn calculate_manifest_digest(&self) -> Result<String>` | 다이제스트만 반환하는 소수 호환용 심(shim) |
| `fn calculate_manifest_summary(&self) -> Result<(String, u64)>` | 다이제스트와 크기 쌍을 위한 소수 호환용 심(shim) |
| `fn detect_multipart(&self) -> Result<(bool, Vec<PathBuf>)>` | 기존 분할 탐색 프로브 (쓰기 핸들에서는 `WriteModeOnly`) |
| `fn multipart_layout(&self) -> Result<MultipartLayout>` | `detect_multipart`를 대체하는 타입화된 대체 메서드 |
| `fn check_symlinks(&self) -> Result<Vec<ArchiveWarning>>` | 추출 시 건너뛸 링크 엔트리에 대한 경고 |

다이제스트는 콘텐츠 멀티셋입니다: 파일 내용은 동일하고 경로는 다른 아카이브는 동일한 값을 생성합니다. [아카이브 체크섬이 실제 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)을 참조하세요.

### 추출

| Signature | Description |
|---|---|
| `fn extract_all(&self, options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | 모든 엔트리를 `options.destination`으로 추출 |
| `fn extract_file(&self, file_path: &str, options: ExtractionOptions) -> Result<()>` | 경로로 하나의 엔트리를 디스크로 추출 |
| `fn extract_files(&self, paths: &[&str], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | 지정된 엔트리 세트 추출 (빈 슬라이스는 no-op) |
| `fn extract_by_ids(&self, ids: &[usize], options: ExtractionOptions) -> Result<ResultWithWarnings<()>>` | 목록 ID로 추출 (빈 슬라이스는 no-op) |
| `fn extract_some<F>(&self, predicate: F, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> where F: FnMut(&ArchiveEntry) -> bool` | 서술자(predicate)가 수용하는 엔트리 추출 |
| `fn extract_filtered<F>(&self, predicate: F, options: ExtractionOptions) -> Result<ResultWithWarnings<()>> where F: FnMut(&ArchiveEntry) -> bool` | `extract_some`으로 위임함 |
| `fn extract_to_memory(&self, file_path: &str) -> Result<Vec<u8>>` | 기본 옵션으로 하나의 엔트리를 `Vec<u8>`로 디코드 |
| `fn extract_to_memory_with_options(&self, file_path: &str, options: &ExtractionOptions) -> Result<Vec<u8>>` | 동일하며, 비밀번호, 제한 사항, CRC 검증을 준수함 |
| `fn extract_to_stream(&self, file_path: &str, bound: StreamBound) -> Result<StreamingExtractor>` | 선택한 출력 바운드 하에 하나의 엔트리를 `Read`로 노출 |
| `fn extract_to_stream_with_options(&self, file_path: &str, options: &ExtractionOptions, bound: StreamBound) -> Result<StreamingExtractor>` | 동일하며, 비밀번호와 제한 사항을 준수함 |

상황별 선택 방법: [올바른 추출 호출을 선택하는 방법](../../../how-to/user/ko/choose-an-extraction-api.md).

### 생성

생성 핸들은 연관 함수로부터 생성됩니다. 이후 `add_*` 메서드는 `&mut self`와 쓰기 모드 핸들을 요구합니다.

| Signature | Description |
|---|---|
| `fn create(path: impl AsRef<Path>, options: CompressionOptions) -> Result<Self>` | 포맷에 대한 옵션 검증 후 쓰기 핸들을 염 |
| `fn create_zip(path: impl AsRef<Path>, opts: ZipCompressionOptions) -> Result<Self>` | 타입화된 ZIP 엔트리 포인트 |
| `fn create_seven_zip(path: impl AsRef<Path>, opts: SevenZCompressionOptions) -> Result<Self>` | 타입화된 7z 엔트리 포인트 |
| `fn create_libarchive(path: impl AsRef<Path>, opts: LibarchiveCompressionOptions) -> Result<Self>` | 타입화된 libarchive 엔트리 포인트 (`ArchiveFormat::Zip` 거부) |
| `fn add_file_from_data(&mut self, path: &str, data: &[u8]) -> Result<()>` | 바이트 슬라이스로부터 하나의 엔트리 작성 |
| `fn add_file_from_path(&mut self, path: impl AsRef<Path>) -> Result<()>` | 파일시스템 경로를 아카이브 경로로 재사용하여 하나의 엔트리 작성 |
| `fn add_file_from_path_as(&mut self, fs_path: impl AsRef<Path>, archive_path: &str) -> Result<()>` | 지정한 아카이브 경로로 하나의 엔트리 작성 |
| `fn add_directory(&mut self, path: &str) -> Result<()>` | 명시적 디렉토리 엔트리 작성 |
| `fn add_directory_recursive(&mut self, path: impl AsRef<Path>) -> Result<()>` | 파일시스템 디렉토리를 순회하며 해당 내용을 작성 |

가이드: [지정한 포맷으로 아카이브를 생성하는 방법](../../../how-to/user/ko/create-an-archive.md).

### 수정

수정 핸들은 작업을 큐에 추가하고 커밋 시점에 적용합니다.

| Signature | Description |
|---|---|
| `fn modify(path: impl AsRef<Path>) -> Result<Self>` | 파일에 권고 잠금(advisory lock)을 획득하고 수정 핸들을 염 |
| `fn modify_with_options(path: impl AsRef<Path>, options: ModificationOptions) -> Result<Self>` | 동일하며, 백업 및 메타데이터 보존 설정을 적용함 |
| `fn add_entry(&mut self, path: &str, data: &[u8]) -> Result<()>` | 바이트 슬라이스로부터 추가 작업을 큐에 넣음 |
| `fn add_entry_from_path(&mut self, archive_path: &str, fs_path: &Path) -> Result<()>` | 커밋 시점에 디스크에서 읽어올 추가 작업을 큐에 넣음 |
| `fn add_entry_from_reader<R>(&mut self, archive_path: &str, reader: R, size: Option<u64>) -> Result<()> where R: Read + Send + 'static` | 소유형 리더(reader)로부터 추가 작업을 큐에 넣음 |
| `fn add_directory_entry(&mut self, path: &str) -> Result<()>` | 명시적 디렉토리 엔트리를 큐에 넣음 |
| `fn remove_entry(&mut self, path: &str) -> Result<usize>` | 해당 경로를 가진 모든 레코드의 제거 작업을 큐에 넣음 (일치한 개수 반환) |
| `fn remove_entry_by_id(&mut self, id: usize) -> Result<()>` | 목록 인덱스로 하나의 레코드 제거 작업을 큐에 넣음 |
| `fn replace_entry(&mut self, path: &str, data: &[u8]) -> Result<()>` | 바이트 슬라이스로부터 제거 후 추가 수행 |
| `fn replace_entry_from_path(&mut self, path: &str, fs_path: &Path) -> Result<()>` | 디스크로부터 제거 후 추가 수행 |
| `fn replace_entry_from_reader<R>(&mut self, path: &str, reader: R, size: Option<u64>) -> Result<()> where R: Read + Send + 'static` | 소유형 리더로부터 제거 후 추가 수행 |
| `fn pending_operations(&self) -> Result<usize>` | 큐에 있는 추가, 제거, 디렉토리 엔트리 수 (수정 모드 외부에서는 `OperationBlocked`) |
| `fn clear_operations(&mut self) -> Result<()>` | 큐를 비움 (수정 모드 외부에서는 `OperationBlocked`) |
| `fn commit_changes(mut self) -> Result<()>` | 아카이브를 재작성하여 큐를 적용함 (핸들을 소비함) |

아카이브를 재작성하는 이유: [수정이 아카이브를 재작성하는 이유](../../../explanation/developer/ko/modification-is-a-rewrite.md).
가이드: [기존 아카이브에서 엔트리를 추가, 교체 또는 제거하는 방법](../../../how-to/user/ko/modify-a-zip-or-7z.md).

### 생명주기

| Signature | Description |
|---|---|
| `fn finish(mut self) -> Result<()>` | 쓰기 핸들을 마무리함 (핸들을 소비함) |
| `fn close(self) -> Result<()>` | `finish`로 위임하며 동일한 계약을 가짐 |

`finish`는 모드별로 동작합니다: 읽기 핸들은 no-op이며, 쓰기 핸들은 백엔드를 마무리하고, 작업이 큐에 있는 수정 핸들은 이들을 버리는 대신 대기 건수를 명시하는 `OperationBlocked`를 반환하며, 큐가 비어 있는 수정 핸들은 조용히 닫힙니다. 이전 실패로 오염된 쓰기 핸들은 마무리하는 대신 `OperationBlocked`를 반환합니다.

`Archive`는 `Drop`을 구현합니다. 마무리가 되지 않은 쓰기 모드 핸들이 드롭될 때 여전히 마무리를 시도하고 표준 오류에 경로를 명시하는 메시지를 출력합니다(조용한 마무리가 실패했거나, 오염된 핸들이 드롭되어 출력이 내구성이 있다고 간주해서는 안 된다는 정보). 수정 모드 및 읽기 모드 드롭은 아무것도 하지 않습니다: `commit_changes`가 수정 핸들의 내구성 경계입니다. Drop은 에러를 반환할 수 없으므로, `finish` 또는 `close`만이 마무리 실패를 관찰하는 유일한 방법입니다.

## v2 타입화된 핸들 (`v2-api` 기능)

`features = ["v2-api"]`를 사용하면 `unified_archive::v2`가 `Archive` 합타입(sum type)을 모드별로 분할하는 세 가지 핸들을 노출합니다. 이들은 `Archive`를 재구현하는 대신 위임하므로 동작 방식은 변경되지 않습니다. 구조적인 변경으로서 `WriteArchive`는 추출 호출에 전달될 수 없으며, `WriteArchive::finish`는 핸들을 소비합니다. 이들은 0.3에서 추가 항목이며, 0.4에서 기본값이 될 예정입니다.

### `ReadArchive`

생성자: `open(path: impl AsRef<Path>)`, `open_encrypted(path, password: impl AsRef<str>)`, `open_at_offset(path, offset: u64)`, `open_sfx(path)`, `open_with_sfx_progress(path, progress)`. 연관 함수 `detect_sfx(path) -> Result<SfxDetectionResult>` 및 `extract_stub(path, detection: &SfxDetectionResult) -> Result<Vec<u8>>`는 핸들이 필요하지 않습니다.

메서드들은 `Archive`의 읽기 측을 미러링합니다: `path`, `format`, `extension_format`, `is_encrypted`, `is_solid`, `has_recovery_record`, `recovery_percentage`, `list_files`, `list_files_for_limits`, `entry_count`, `find_entry`, `find_entries`, `validate_integrity`, `multipart_layout`, `detect_multipart`, `check_symlinks`, `calculate_archive_crc`, `calculate_manifest_digest`, `calculate_manifest_summary`, `calculate_content_multiset_digest_and_size`, `extract_all`, `extract_file`, `extract_files`, `extract_by_ids`, `extract_some`, `extract_filtered`, `extract_to_memory`, `extract_to_memory_with_options`, `extract_to_stream`, `extract_to_stream_with_options`.

### `WriteArchive`

생성자: `create(path, options: CompressionOptions)`, `create_zip(path, opts)`, `create_seven_zip(path, opts)`, `create_libarchive(path, opts)`.

메서드: `path`, `format`, `add_file_from_data`, `add_file_from_path`, `add_file_from_path_as`, `add_directory`, `add_directory_recursive`, `entry_count`, 그리고 핸들을 소비하는 `finish(mut self) -> Result<PathBuf>` / `close(self) -> Result<PathBuf>` — 둘 다 생성된 아카이브의 경로를 반환하며(`Archive::finish`는 `()`), `WriteArchive`는 `Drop`을 구현하여 마무리가 안 된 핸들을 한 번 마무리하고 경고하며 내부 `Archive`의 자체 `Drop`이 아무것도 하지 않도록 남겨둡니다.

### `ModifyArchive`

생성자: `open(path)`, `open_with_options(path, opts: ModificationOptions)`.

메서드: `path`, `format`, `add_entry`, `add_entry_from_path`, `add_entry_from_reader`, `add_directory_entry`, `replace_entry`, `replace_entry_from_path`, `replace_entry_from_reader`, `list_files`, `entry_count`, `find_entry`, `find_entries`, `remove_entry`, `remove_entry_by_id`, `clear_operations`, `pending_operations`, `try_commit_changes(&mut self) -> Result<()>`, 그리고 `commit_changes(mut self) -> Result<()>`.

`try_commit_changes`는 `Archive`에 상응하는 동등한 메서드가 없습니다: 핸들을 소비하지 않고 큐를 검증하고 적용하므로 거부된 커밋 후에도 핸들을 사용할 수 있는 상태로 유지합니다.

두 `v2` 경로 모두 동일한 타입으로 해석됩니다: `unified_archive::v2::ReadArchive` 및 `unified_archive::archive::mode_split::ReadArchive`.
