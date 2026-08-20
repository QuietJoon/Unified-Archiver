---
type: Reference
title: Cargo 기능 및 MSRV
description: 플랫폼 게이트, 라이선스 결과, 비활성화 시 동작 및 에디션, MSRV, 의존성 세트를 포함한 unified-archive의 모든 Cargo 기능 설명서입니다.
tags: [build, api, platform]
audience: developer
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/reference/developer/en/cargo-features.md }
synced_hash: 1b57d99eea64de44c8f6792702819672afe0f2d3ac5d30b61459944146975ed4
---
# Cargo 기능 및 MSRV

## 패키지 정체성

매니페스트의 `[package]` 테이블이 선언하는 10개 키 전체(매니페스트 순서):

| 매니페스트 키 | 값 |
|---|---|
| `name` | `unified-archive` |
| `version` | `0.4.0` |
| `edition` | `2024` |
| `rust-version` | `1.85` |
| `authors` | `YongJoon Joe <developer@yongjoon.net>` |
| `license` | `MIT` |
| `description` | `Unified archive library for Rust` |
| `repository` | `https://github.com/QuietJoon/unified-archive` |
| `keywords` | `archive`, `7zip`, `compression`, `extraction`, `unified` |
| `categories` | `compression`, `filesystem` |

`description` 문자열은 매니페스트 자체의 텍스트입니다. 해당 생성 목록은 `ArchiveFormat::can_create`와 일치합니다: TAR.ZST, TAR.LZ4 및 TAR.LZMA 생성이 2026-08-04에 적용되었으며 설명은 2026-08-06에 일치하도록 업데이트되었습니다.

매니페스트는 `[[bin]]` 타겟 및 `[lints]` 테이블을 선언하지 않습니다. 저장소에는 `rust-toolchain` / `rust-toolchain.toml` 파일이 없으므로 사용되는 툴체인은 Cargo를 호출하는 툴체인입니다.

## 에디션 및 MSRV

`edition = "2024"` 및 `rust-version = "1.85"`입니다. 에디션 2024는 모든 `extern` 블록이 `unsafe extern`으로 작성될 것을 요구하며, `unsafe fn` 본문 내부에서 명시적인 `unsafe` 블록을 요구합니다; `src/ffi/libarchive.rs` 및 `src/ffi/unrar.rs`의 FFI 선언은 이 형태로 작성되어 있습니다. `rust-version`은 Cargo 수준의 하한선입니다: 1.85보다 오래된 툴체인은 컴파일 전 Cargo에 의해 거부되며, 에디션 2024 자체도 1.85 이전에는 사용할 수 없습니다.

그 어떤 기능(feature)도 MSRV를 올리거나 내리지 않습니다 — 선택적 종속성을 추가하는 기능이 없기 때문입니다([기능 선언](#feature-declarations) 참조).

## 기능 선언

매니페스트의 `[features]` 테이블에는 정확히 4개의 항목이 있습니다:

| Feature | In default set | Declared value | Compiled when |
|---|---|---|---|
| `default` | — | `["rar-support"]` | 소비자가 `default-features = false`를 설정하지 않는 한 항상 |
| `rar-support` | yes | `[]` | 모든 타겟에서 해당 기능이 활성화될 때 |
| `external-rar-create` | no | `[]` | 해당 기능이 활성화되고 **또한** `target_os = "windows"`일 때 |
| `v2-api` | no | `[]` | 모든 타겟에서 해당 기능이 활성화될 때 |

`default` 이외의 모든 기능은 비어 있는 값 목록을 가집니다. 선언된 선택적 종속성이 없으므로 선택적 종속성을 활성화하는 기능은 없습니다: 모든 기능 조합에 대해 크레이트 그래프는 동일합니다. 기능은 자사 코드의 `cfg` 컴파일을 전환하며, `rar-support`에 한해 빌드 스크립트의 한 단계를 전환합니다.

### `rar-support` (기본 세트에 포함됨)

다음 항목을 활성화합니다:

- `build.rs`의 번들 UnRAR 빌드. `build_unrar` 단계는 `#[cfg(feature = "rar-support")]` 하에서 컴파일됩니다; `src/ffi/native/unrar` 하위의 C++ 소스를 `cc` 크레이트로 컴파일하여 `OUT_DIR`에 정적 `unrar` 라이브러리를 생성하고 `cargo:rustc-link-search=native=<OUT_DIR>` 및 `cargo:rustc-link-lib=static=unrar`를 출력합니다. 게이트는 호스트가 아닌 기능 전용이므로 이 단계는 Windows, macOS, Linux 타겟 모두에서 동일하게 실행됩니다.
- FFI 모듈 `crate::ffi::unrar`(원시 바인딩) 및 `crate::ffi::wrapper`(`UnrarArchive`), 둘 다 `src/ffi.rs`에서 기능 하에 선언되어 있습니다.
- `src/archive.rs`의 내부 `ArchiveBackend::Unrar` 변리언트, `src/backend.rs`의 `UnrarArchive`용 `ReadBackend` 구현체, 및 파사드 디스패치 사다리의 RAR 암(arm)들: `Archive::open`/`open_as_format`, `Archive::open_encrypted`, `is_solid`, `has_recovery_record`, `recovery_percentage`, `src/extraction.rs`의 단일 파일 추출 경로, 및 쓰기 마무리 사다리.

플랫폼 게이트: 없음.

종속성: 크레이트가 추가되지 않습니다. 번들 소스에 대해 빌드 시 C++ 툴체인이 필요합니다; 자세한 내용은 [빌드 환경](../../../reference/operator/ko/build-environment.md)에 있습니다.

라이선스 결과: 크레이트 자체는 MIT(`LICENSE`)입니다. 번들로 제공되는 UnRAR 소스는 UnRAR 라이선스(`src/ffi/native/unrar/license.txt`)를 따릅니다. 라이선스 약관에 따르면 RAR 아카이브를 처리하는 모든 소프트웨어에서 소스를 무료로 사용할 수 있지만, RAR(WinRAR) 호환 아카이버를 개발하거나 RAR 압축 알고리즘을 재창성하는 데 사용할 수 없으며, 소스 배포 시(별도 또는 다른 소프트웨어의 일부로서) 라이선스, 문서 및 소스 주석에 해당 단락을 재현해야 합니다. `rar-support`로 빌드하면 해당 소스에서 파생된 오브젝트 코드가 소비 바이너리에 정적으로 링크됩니다.

기능이 비활성화된 경우:

- UnRAR 소스가 컴파일되지 않으며 `unrar` 라이브러리가 링크되지 않습니다.
- `Archive::open`(`open_as_format`을 통함) 및 `Archive::open_encrypted`는 `ArchiveFormat::Rar` 및 `ArchiveFormat::Rar5`에 대해 `ArchiveError::unsupported`를 반환하며, 사유는 `RAR/RAR5 support is disabled in this build (enable the rar-support Cargo feature to include UnRAR)`로 표시됩니다. 이 두 암은 크레이트 내 유일한 `cfg(not(feature = "rar-support"))` 블록입니다.
- 다른 모든 항목은 형태가 유지됩니다. `ArchiveFormat::Rar` / `ArchiveFormat::Rar5`는 여전히 존재하며, `src/format.rs`의 매직 바이트 및 확장자 감지는 여전히 RAR을 식별하고, `src/sfx.rs`의 SFX 감지는 여전히 RAR 페이로드를 분류합니다 — 이러한 모듈 중 어느 것도 기능으로 게이트되지 않습니다. 거부는 아카이브를 열 때 발생합니다.

### `external-rar-create`

`external` 모듈과 그 단일 퍼블릭 타입을 활성화하며, 둘 다 `src/lib.rs` 및 `src/external.rs`에서 `#[cfg(all(target_os = "windows", feature = "external-rar-create"))]`로 게이트되어 있습니다:

- WinRAR 커맨드라인 툴을 생성(spawn)하여 RAR 아카이브를 생성하는 `unified_archive::external::RarCreator`(`src/external/rar.rs`). 생성자 `RarCreator::new`(`rar.exe`를 먼저 찾은 후 기존 출력 경로를 거부함) 및 `RarCreator::with_rar_exe_path`(탐색을 우회하고 지정된 경로가 일반 파일이어야 함). 메서드 `set_compression_level`, `set_password`, `add_file`, `add_directory`, `entry_count`, `rar_exe_path`, 및 소유형 `create`.
- `rar.exe` 탐색 순서: 현재 `PATH`에 대한 `where rar.exe`, 그 다음 `C:\Program Files\WinRAR\rar.exe`, 그 다음 `C:\Program Files (x86)\WinRAR\rar.exe`. 커스텀 및 포터블 설치는 자동 감지되지 않습니다.
- 생성된 인자 벡터는 `a`, `-mN` 레벨 플래그(`Store` → `-m0`, `Fastest` → `-m1`, `Fast` → `-m2`, `Normal` → `-m3`, `Maximum` → `-m4`, `Ultra` → `-m5`), 비밀번호 설정 시 `-hp<password>`, `-r`, 스위치 끝 알림인 `--`, 출력 경로, 그 다음 엔트리 경로들이며, 선두 대시가 있는 엔트리는 `.` 하위로 재지정됩니다.

플랫폼 게이트: Windows 타겟 전용. 다른 타겟에서 기능을 활성화해도 아무것도 컴파일되지 않습니다 — 해당 타겟에는 `unified_archive::external`이 존재하지 않습니다.

종속성: 크레이트가 추가되지 않습니다.

라이선스 결과: 크레이트는 RAR 압축 코드를 포함하지 않습니다. 이 경로를 통해 RAR 아카이브를 생성하려면 해당 프로그램을 실행하는 머신에 라이선스가 있는 WinRAR이 설치되어 있어야 하며, WinRAR의 자체 라이선스 약관을 따릅니다.

소스에 기록된 보안 결과: 비밀번호가 설정된 경우 `-hp<password>`는 자식 프로세스의 인자 벡터에 포함되므로 `rar.exe`가 활성화되어 있는 동안 실행 중인 프로세스를 나열할 수 있는 모든 것에 노출됩니다. `Password` 타입은 이 프로세스의 주소 공간 내부 값만 보호합니다.

이 표면은 RAR을 전혀 생성하지 않고 모든 포맷에 대해 암호화된 생성을 거부하는 `Archive::create`와 독립적입니다.

기능이 비활성화된 경우(또는 Windows가 아닌 타겟의 경우): `unified_archive::external`이 존재하지 않으며, 크레이트는 외부 프로세스를 생성하지 않습니다.

### `v2-api`

타입화된 핸들 모듈을 활성화합니다:

- `src/lib.rs`에서 `#[cfg(feature = "v2-api")]`로 선언되어 `crate::archive::mode_split`으로부터 `ModifyArchive`, `ReadArchive`, `WriteArchive`를 재노출하는 `unified_archive::v2`.
- 모듈 선언 및 내부 `#![cfg(feature = "v2-api")]`에 의해 게이트된 `crate::archive::mode_split` 자체(`src/archive/mode_split.rs`).
- `finish` 없이 드롭된 `WriteArchive`가 정확히 한 번 마무리되고 내부 `Archive`의 자체 `Drop`이 두 번째로 마무리되거나 경고하지 않도록 `WriteArchive`의 `Drop`에서 사용하는 `pub(crate)` `Archive::finalize_write_on_drop`.

세 타입은 내부 `Archive`를 감싸고 그에 위임합니다; 이들은 동작을 변경하는 대신 컴파일 타임에 모드별로 작업 세트를 좁힙니다. `WriteArchive::finish`는 핸들을 소비합니다. 메서드별 표면은 [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md)에 있습니다.

플랫폼 게이트: 없음. 종속성: 추가되지 않음. 라이선스 결과: 없음.

버전 상태: 0.3.0에서 추가적(additive)임 — 기존 `Archive` 파사드는 기능 활성화 여부와 관계없이 동일합니다. 매니페스트 주석과 `unified_archive::v2`의 rustdoc 모두 0.4에서 이 기능이 기본으로 활성화될 것임을 기록하고 있습니다.

기능이 비활성화된 경우: `unified_archive::v2`가 존재하지 않으며 `mode_split`이 컴파일되지 않습니다.

### `default-features = false`의 효과

기본 세트에는 `rar-support`만 포함되어 있습니다. `default-features = false`로 선언되고 명시적인 기능 목록이 없는 종속성은 세 기능 모두 끌고 크레이트를 컴파일하며, 아래의 전체 종속성 세트는 여전히 컴파일됩니다.

## 종속성 세트

### 항상 컴파일됨

이들은 `[dependencies]`에 무조건 선언되어 있습니다; 그 어떤 기능도 이들을 추가하거나 제거하지 않습니다.

| Crate | Version requirement | Declared features | Role in the crate |
|---|---|---|---|
| `once_cell` | `1.20` | default | 지연 초기화되는 핸들별 상태(ZIP 목록 캐시, 수정 엔트리 캐시 및 유사한 메모이제이션된 메타데이터)를 위한 `OnceCell` |
| `crc32fast` | `1.4` | default | CRC32 계산 및 검증 |
| `secstr` | `0.5` | default | `Password`의 백킹 스토어 |
| `walkdir` | `2.4` | default | `add_directory_recursive`를 위한 디렉토리 순회 |
| `sevenz-rust2` | `0.19` | default | 7z 읽기/추출/생성 백엔드 |
| `zip` | `2.2` | `default-features = false`, `deflate`, `aes-crypto` | 단독 ZIP 읽기, 추출 및 생성기 |
| `tempfile` | `3.0` | default | 원자적 출력 파일 및 수정 임시 경로 |
| `fs4` | `1` | default | Modify 모드를 위한 자문 파일 잠금 |

`zip` 기능 선택은 소비자가 아닌 이 크레이트에 의해 고정됩니다: ZIP 백엔드는 `deflate` 및 `aes-crypto`와 함께 빌드되며 `zip`의 다른 선택적 코덱 없이 빌드됩니다.

### 타겟 게이트됨

| Crate | Target predicate | Version | Reason recorded in the manifest |
|---|---|---|---|
| `libc` | `cfg(windows)` | `0.2` | libarchive 쓰기 생성자(`src/ffi/libarchive_wrapper/writer.rs`; 매니페스트 주석은 상위 모듈 `libarchive_wrapper.rs`를 명시)의 Windows 암이 `libc::_open_osfhandle` 및 `libc::_close`를 호출합니다. Windows libarchive 빌드가 연기된 동안 기본 CI에 의해 해당 분기가 실행되지 않더라도 Windows 타겟 컴파일 프로브가 `E0432`로 실패하지 않도록 종속성이 선언되었습니다. |

### 빌드 종속성

`build.rs`에 의해서만 사용되며 라이브러리에 링크되지 않습니다:

| Crate | Version | Use |
|---|---|---|
| `pkg-config` | `0.3` | Windows가 아닌 타겟에서 `probe_library("libarchive")` |
| `cc` | `1` | `rar-support` 하에서 번들 UnRAR C++ 소스 컴파일 |

### 개발 종속성

소스 트리의 자체 테스트, 벤치마크, 예제를 빌드할 때만 컴파일되며, 게시된 크레이트의 소비자에게는 컴파일되지 않습니다:

| Crate | Version | Declared features |
|---|---|---|
| `criterion` | `0.5` | default |
| `proptest` | `1.4` | default |
| `serial_test` | `3` | `file_locks` |
| `zip` | `2.2` | `default-features = false`, `deflate` |

`zip`은 `[dependencies]`와 `[dev-dependencies]` 모두에 나타납니다; dev 항목은 ZIP 메타데이터를 직접 검사하는 통합 테스트를 위해 존재합니다.

### 네이티브 라이브러리

libarchive와 UnRAR 모두 Cargo 종속성이 아닙니다. libarchive는 빌드 스크립트에 의해 검색되고 링크되는 시스템 라이브러리입니다; UnRAR은 `rar-support` 하에서 번들 소스로부터 컴파일됩니다. 둘 다 [빌드 환경](../../../reference/operator/ko/build-environment.md)에 설명되어 있으며, 시스템 요구사항은 [네이티브 빌드 종속성을 충족하는 방법](../../../how-to/operator/ko/install-native-dependencies.md)에 나열되어 있습니다.

## 매니페스트 타겟

- 라이브러리: 크레이트 루트는 `src/lib.rs`입니다. 바이너리 타겟이 없으므로 크레이트는 라이브러리로 소비됩니다.
- 예제: 4개의 `[[example]]` 섹션이 명시적으로 선언되어 있습니다 — `inspect_archive`, `extract_archive`, `create_archive`, `modify_archive`. `examples/` 디렉토리에는 `archive_crc.rs`, `detect_sfx.rs`, `stream_checksum.rs`, 및 `streaming_extract.rs`도 들어 있으며, 매니페스트가 `autoexamples = false`를 설정하지 않기 때문에 Cargo의 타겟 자동 감지가 이를 선택합니다.
- 벤치마크: 3개의 `[[bench]]` 섹션(각각 `harness = false` 지정) — `archive_operations`, `integrity_validation_bench`, `sfx_detection`. 해당 소스는 `benches/archive_operations.rs`, `benches/integrity_validation_bench.rs`, 및 `benches/sfx_detection.rs`입니다.

## 기능별 포맷 커버리지

RAR 및 RAR5 읽기 및 추출에는 `rar-support`가 필요합니다. RAR 생성은 `Archive::create`를 통해서는 제공되지 않으며 `external-rar-create` 하의 Windows에서 `external::RarCreator`를 통해서만 존재합니다. 다른 모든 지원 포맷 — ZIP, 7z, TAR 계열, 단독 압축 스트림, 및 ISO — 은 빌드가 링크하는 네이티브 libarchive에 따라 모든 기능 조합에서 사용 가능합니다. 포맷별 세부사항은 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)에 있습니다.
