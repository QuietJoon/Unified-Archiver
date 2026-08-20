---
type: Reference
title: 빌드 환경
description: 플랫폼별 unified-archive 빌드 요구 사항, build.rs의 정확한 동작, 링크 라인에 포함되는 라이브러리 및 현재 플랫폼 지원 상태.
tags: [build, platform, config]
audience: operator
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/reference/operator/en/build-environment.md }
synced_hash: f45aac4c4ad66900347555a252cb0058dc9501481ad29eeb82f64b1c015085bb
language: ko
---
# 빌드 환경

## 타겟별 요구 사항

| 타겟 | Rust | libarchive | pkg-config | C++ 툴체인 |
|---|---|---|---|---|
| macOS | 1.85 이상 | 필수; `lib/libarchive.dylib`을 포함하는 Homebrew keg 프리픽스이거나 pkg-config에서 접근 가능한 설치 | pkg-config 폴백 경로에만 필수 | `rar-support`가 활성화된 경우 필수(기본값); `cc`가 플랫폼 컴파일러를 선택함 |
| Linux 및 기타 비-Windows 타겟 | 1.85 이상 | 필수 (개발 패키지에 포함된 pkg-config 메타데이터 `libarchive.pc` 필요) | 필수 | `rar-support`가 활성화된 경우 필수 |
| Windows | 1.85 이상 | 빌드 스크립트에 의해 구성되지 않음 — 링크 지시문이 출력되지 않음 ([플랫폼 지원 상태](#플랫폼-지원-상태) 참조) | 사용되지 않음 | `rar-support`가 활성화된 경우 필수; `msvc` 타겟 환경에서는 `cc`가 MSVC를 선택함 |

툴체인 최소 사양은 모든 타겟에서 동일합니다: 크레이트가 `edition = "2024"` 및 `rust-version = "1.85"`를 선언하므로 Rust 1.85 이상이어야 합니다.

`make`는 빌드의 일부가 아닙니다. `build.rs`는 직접 어떠한 외부 프로세스도 호출하지 않습니다; 유일한 자식 프로세스는 `pkg-config` 및 `cc` 크레이트가 시작하는 프로세스들(`pkg-config` 바이너리, C++ 컴파일러 및 아카이버)입니다. 번들된 UnRAR `makefile`은 `src/ffi/native/unrar`에 여전히 존재하지만, 빌드 입력이 아니며 변경 사항을 감시하지 않습니다. `cc`는 Cargo의 `TARGET`, `CC`, `CXX`, `AR`을 준수합니다.

libarchive 헤더는 컴파일되지 않습니다: `src/ffi/libarchive.rs`의 바인딩은 직접 작성된 Rust `unsafe extern "C"` 선언이므로, libarchive는 컴파일 타임 포함 요구 사항이 아니라 링크 타임 및 런타임 요구 사항입니다. pkg-config 검사에는 여전히 개발 패키지가 설치하는 `.pc` 파일이 필요합니다.

플랫폼별 설치 명령은 [네이티브 빌드 의존성 충족 방법](../../../how-to/operator/ko/install-native-dependencies.md)을 참조하세요. 어떤 기능이 빌드 대상을 변경하는지 확인하려면 [Cargo 기능 및 MSRV](../../../reference/developer/ko/cargo-features.md)를 참조하세요.

## `build.rs`가 수행하는 작업

스크립트는 이 순서대로 4가지 작업을 실행합니다.

### 1. 자체 변경 사항 추적

`cargo:rerun-if-changed=build.rs`를 출력합니다.

### 2. libarchive 탐색

탐색 블록은 `#[cfg(target_os = …)]`에 의해 선택됩니다. 빌드 스크립트에서 이러한 `cfg` 값은 Cargo의 `--target`이 아닌 **빌드 호스트**를 설명합니다; 호스트와 타겟 간의 이 불일치는 OI-0080-001의 미결 항목입니다.

**macOS 호스트.** 두 가지 후보 프리픽스가 순서대로 검사됩니다: `/opt/homebrew/opt/libarchive` (Apple Silicon 레이아웃) 및 `/usr/local/opt/libarchive` (Intel 레이아웃). `{prefix}/lib/libarchive.dylib`가 존재하는 경우에만 프리픽스가 인정되므로, 비어 있거나 부분적으로 제거된 keg는 나중에 링커에서 실패하는 검색 경로를 생성하는 대신 다음으로 넘어갑니다. 매칭 시 스크립트는 `cargo:rustc-link-search=native={prefix}/lib` 및 `cargo:rustc-link-lib=dylib=archive`를 출력하고 stderr에 `unified-archive: using Homebrew libarchive from {prefix}`를 출력합니다. 매칭되지 않으면 `pkg_config::probe_library("libarchive")`를 호출합니다; 여기서 에러를 반환하면 스크립트는 `libarchive not found. Install it with: brew install libarchive`와 함께 패닉을 발생시킵니다.

**기타 비-Windows 호스트.** `pkg_config::probe_library("libarchive")`가 유일한 메커니즘입니다. 성공 시 `pkg-config` 크레이트는 도출된 링크 검색 경로 및 `-l` 지시문을 출력합니다. 에러 시 스크립트는 배포판 패키지 이름을 명시하는 메시지와 함께 패닉을 발생시킵니다:

```text
libarchive not found via pkg-config. Install libarchive development files: `sudo apt-get install libarchive-dev` (Ubuntu/Debian) or `sudo dnf install libarchive-devel` (Fedora/RHEL).
```

**Windows 호스트.** 스크립트는 `cargo:warning=Windows libarchive linking will be configured in future implementation`을 출력하고 `rustc-link-search` 및 `rustc-link-lib`를 출력하지 않습니다. 따라서 libarchive 링크 구성은 빌드 스크립트 외부에서 제공되어야 합니다; `README.md`에서는 vcpkg 설치본에 대한 `RUSTFLAGS="-L native=<path>"`를 제공하거나 사전 빌드된 `.lib`를 번들링하여 이를 공급하는 방법을 설명합니다.

**크로스 빌드.** `pkg-config` (`Cargo.lock`의 0.3.32)는 `PKG_CONFIG_ALLOW_CROSS`가 `0`이 아닌 다른 값으로 설정되거나 `PKG_CONFIG` / `PKG_CONFIG_SYSROOT_DIR`이 설정되지 않은 한 `TARGET`이 `HOST`와 다를 때 탐색을 건너땁니다. 건너뛴 탐색은 `build.rs`에 에러로 전달되며 결과적으로 동일한 "libarchive not found" 패닉이 발생합니다.

### 3. 번들된 UnRAR 빌드

기본 기능 세트에 포함된 `#[cfg(feature = "rar-support")]`에서만 컴파일됩니다. 이 단계는 호스트가 아닌 기능에 의해서만 조건 제어되며, stderr에 `unified-archive: building vendored UnRAR (cc) for target_os={target_os}`를 출력합니다.

타겟 선택은 `CARGO_CFG_TARGET_OS` 및 `CARGO_CFG_TARGET_ENV`를 읽으므로 소스 세트, 전처리기 정의 및 플래그 선택은 빌드 호스트가 아닌 Cargo의 타겟을 따릅니다.

| 측면 | Windows 타겟 | 기타 모든 타겟 |
|---|---|---|
| 소스 목록 | 업스트림 `UnRARDll.vcxproj` `<ClCompile>` 목록을 반영하는 50개 파일의 `WINDOWS_SOURCES` 세트 — `isnt.cpp`, `motw.cpp`, `rarpch.cpp`, `rs.cpp`를 추가하고 `resource.cpp` 및 `list.cpp`를 제외함 | 번들된 makefile의 `lib` 타겟의 `OBJECTS` + `LIB_OBJ`를 반영하는 48개 파일의 `UNIX_SOURCES` 세트 |
| 정의 | `RARDLL`, `UNRAR`, `SILENT` | `RARDLL`, `_FILE_OFFSET_BITS=64`, `_LARGEFILE_SOURCE`, `RAR_SMP` |
| 플래그 | `CARGO_CFG_TARGET_ENV`가 `msvc`일 때는 추가되지 않음 | `-std=c++11`, `-Wno-logical-op-parentheses`, `-Wno-switch`, `-Wno-dangling-else` (각각 컴파일러가 지원하는 경우에만 적용됨), 위치 독립적 코드 및 `-pthread` |

수많은 번들된 `.cpp` 파일(`unpack15/20/30/50`, `crypt1`–`crypt5`, `recvol3/5`, `blake2s_sse`, `win32*` 헬퍼, `ulinks`, `uowners`, `hardlinks` 등)은 다른 번역 단위에 의해 `#include`되며 의도적으로 단독 컴파일되지 않습니다; 단독 컴파일 시 심볼 중복으로 링크에 실패합니다. `cc`의 추가 경고는 이 타사 코드에 대해 비활성화됩니다.

`cc::Build::compile("unrar")`는 모든 오브젝트 파일과 결과 정적 라이브러리를 `OUT_DIR` 아래에 작성하며 — 번들된 소스 트리는 절대 수정되지 않습니다 — `cargo:rustc-link-search=native=<OUT_DIR>` 및 `cargo:rustc-link-lib=static=unrar`를 출력합니다.

### 4. 번들 소스 변경 사항 추적

스크립트는 `src/ffi/native/unrar`를 재귀적으로 순회하고 모든 `.cpp`, `.hpp`, `.h` 파일(하위 디렉토리 파일 및 `cc`가 자체적으로 추적하지 않는 헤더와 `#include`된 소스 포함)에 대해 `cargo:rerun-if-changed=<path>`를 출력합니다. 파일 시스템 에러는 묵인되지 않습니다: 열거할 수 없는 디렉토리, 읽을 수 없는 항목 또는 stat을 실행할 수 없는 파일은 이전 네이티브 아티팩트가 재사용되도록 허용하는 대신 메시지에 문제가 되는 경로를 담아 패닉을 발생시킵니다.

## 환경 변수

`build.rs`는 UnRAR 단계에서 정확히 두 개의 변수를 직접 읽습니다:

| 변수 | 효과 |
|---|---|
| `CARGO_CFG_TARGET_OS` | UnRAR 소스 목록, 정의, 플래그 세트(`windows` 대 기타 전체) 및 pthread 링크 지시문(`linux` 및 `android`만 해당)을 선택합니다. `unwrap_or_default`로 읽히므로 설정되지 않은 값은 빈 문자열이 되어 비-Windows 경로를 타게 됩니다. |
| `CARGO_CFG_TARGET_ENV` | 값이 `msvc`일 때 GNU/clang 플래그를 억제합니다. `unwrap_or_default`로 읽힙니다. |

Cargo가 두 변수를 모두 설정합니다. 스크립트는 다른 환경 변수를 읽지 않으며 크레이트는 자체 빌드 타임 구성 변수를 정의하지 않습니다: 코드 트리 어디에도 `UNIFIED_ARCHIVE_*` 빌드 노브가 없습니다.

두 빌드 의존성에 의해 간접적으로 소비되는 변수로, 이들도 자체 `cargo:rerun-if-env-changed` 지시문을 출력합니다 (`build.rs`는 아무것도 출력하지 않음):

- `pkg-config`: `env_metadata`가 활성화된 상태로 실행되는 `probe_library`를 통함: `PKG_CONFIG`, `PKG_CONFIG_PATH`, `PKG_CONFIG_LIBDIR`, `PKG_CONFIG_SYSROOT_DIR`, `PKG_CONFIG_ALL_STATIC`, `PKG_CONFIG_ALL_DYNAMIC`, `PKG_CONFIG_ALLOW_CROSS`, 라이브러리 전용 오버라이드 `LIBARCHIVE_NO_PKG_CONFIG`, `LIBARCHIVE_STATIC`, `LIBARCHIVE_DYNAMIC`, 그리고 이 이름들에 타겟 접미사 및 `HOST_`/`TARGET_` 접두사가 붙은 형태들.
- `cc`: 컴파일러 및 아카이버 선택과 플래그 변수(`CC`, `CXX`, `AR`, `CFLAGS`, `CXXFLAGS` 및 해당 타겟 접미사 형태들).

## 링크 라인의 라이브러리

| 라이브러리 | 종류 | 지시문의 출처 | 시점 |
|---|---|---|---|
| `archive` | 동적 | macOS Homebrew 경로에서 명시적으로 제공되거나 `pkg-config` 크레이트가 출력하는 지시문에서 제공됨. `src/ffi/libarchive.rs`의 extern 블록에 설정된 `#[link(name = "archive")]`로도 선언됨 | 모든 비-Windows 빌드; Windows에서는 아무것도 기여하지 않음 |
| `unrar` | 정적, `OUT_DIR` 출처 | `cargo:rustc-link-lib=static=unrar` 및 `cc`가 제공하는 `OUT_DIR` 검색 경로. `src/ffi/unrar.rs`의 `#[link(name = "unrar")]`로도 선언됨 | `rar-support` 활성화 시에만 |
| C++ 표준 라이브러리 | `cc`가 선택 | 빌더가 `cpp(true)`를 설정하므로 `cc` 자체가 출력함. `build.rs`에는 플랫폼별 C++ 런타임 분기가 포함되어 있지 않음; 주석에는 `cc`가 적용하는 매핑(macOS에서는 libc++, Linux에서는 libstdc++, MSVC에는 필요 없음)이 기록되어 있음 | `rar-support` 활성화 시에만 |
| `pthread` | 동적 | `RAR_SMP` 스레드풀을 위해 `CARGO_CFG_TARGET_OS`가 `linux` 또는 `android`일 때만 출력되는 `cargo:rustc-link-lib=dylib=pthread`. macOS는 pthread가 libSystem에 존재하므로 지시문이 필요 없으며, 이는 스크립트에서 유일하게 명시적인 플랫폼별 링크 분기입니다 | `rar-support` 활성화 시에만 |

## 링크된 libarchive의 코덱

libarchive는 동적으로 링크되므로 작동하는 압축 필터는 이 크레이트가 아닌 설치된 라이브러리의 속성입니다. 쓰기 경로는 요청된 포맷에 따라 `archive_write_add_filter_gzip`, `_bzip2`, `_xz`, `_zstd`, `_lz4`, `_lzma`를 호출하며, `ARCHIVE_OK` 이외의 반환값은 하드 `Format` 에러로 처리합니다 — 이 검사는 모든 필터 분기에 적용됩니다. libzstd 또는 liblz4 없이 빌드된 libarchive는 해당 두 필터를 외부 프로그램 폴백으로 등록하고 `ARCHIVE_WARN`을 반환하며, 이 크레이트는 외부 압축기를 묵인하고 실행하는 대신 이를 에러로 변환합니다. 따라서 TAR.ZST, TAR.LZ4, TAR.LZMA를 생성하려면 일치하는 코덱이 포함되어 컴파일된 libarchive가 필요합니다. 포맷별 세부사항은 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)에 있습니다.

## 빌드 출력물

모든 네이티브 아티팩트(UnRAR 오브젝트 파일 및 정적 `unrar` 라이브러리)는 `OUT_DIR` 아래에 작성됩니다. 번들된 소스 트리는 빌드에 의해 손대지 않은 상태로 유지됩니다.

리포지토리는 `[build]` `target-dir`이 절대 경로인 `/Volumes/Common/QJoon/Unified-Archiver/target`으로 지정된 추적 파일 `.cargo/config.toml`을 제공합니다. 따라서 다른 머신에서 클론하면 Cargo 아티팩트가 `./target` 대신 해당 경로에 작성되며, 해당 경로에 쓰기 권한이 없으면 Cargo는 아무것도 컴파일하기 전에 실패합니다.

## 빌드가 출력하는 진단 메시지

| 채널 | 텍스트 | 조건 |
|---|---|---|
| stderr | `unified-archive: using Homebrew libarchive from {prefix}` | macOS 호스트, dylib을 포함하는 Homebrew 프리픽스가 발견됨 |
| stderr | `unified-archive: building vendored UnRAR (cc) for target_os={target_os}` | `rar-support` 활성화됨 |
| `cargo:warning` | `Windows libarchive linking will be configured in future implementation` | Windows 호스트 |
| panic | `libarchive not found. Install it with: brew install libarchive` | macOS 호스트, Homebrew dylib이 없고 pkg-config 탐색 실패함 |
| panic | `libarchive not found via pkg-config. Install libarchive development files: …` | 비-macOS 비-Windows 호스트, pkg-config 탐색 실패함 |
| panic | `Failed to enumerate vendored UnRAR sources in {dir}`, `Failed to read a directory entry in vendored UnRAR sources {dir}`, 또는 `Failed to stat vendored UnRAR source {path}` | `rar-support` 활성화 상태에서 번들 트리를 순회할 수 없음 |

두 stderr 줄은 `cargo build -vv`를 통해 확인할 수 있습니다.

## 플랫폼 지원 상태

macOS와 Linux는 검증을 거친 플랫폼입니다. Windows 지원은 소스상에 존재하지만 릴리스 검증을 거치지 않았습니다:

- 번들된 UnRAR 부분은 완료되었습니다. `build.rs`는 MSVC 소스 세트를 네이티브 방식으로 컴파일하며, 이전의 `make` 전용 빌드 및 임시 대안이었던 Windows `panic!`은 제거되었습니다.
- libarchive 부분은 미결 상태입니다. Windows에서 탐색 블록은 경고 전용 no-op이므로 아무것도 libarchive를 링크하지 않습니다: libarchive 기반 포맷(TAR 패밀리, ISO, 단독 압축 스트림 및 비-ZIP 생성)은 외부에서 링크 구성을 제공하지 않는 한 빌드할 수 없습니다. 이는 `docs/project/open-issues.md`의 OI-0065-001에 해당하며, 남은 필수 작업은 vcpkg 기반 탐색, README 수정 및 Windows CI 작업입니다. 탐색 메커니즘 선택은 `docs/backlog.md`에 소유자 결정으로 기록되어 있습니다.
- 크로스 컴파일은 v2 이전에는 지원을 주장하거나 문서화하거나 검증하지 않습니다. OI-0080-001의 UnRAR 부분은 빌드가 `cc`로 이동할 때 해결되었습니다; libarchive 탐색 부분(위에서 설명한 호스트 `cfg` 분기 및 크로스 컴파일 스모크 작업 부재)은 여전히 미결 상태입니다.

## 빌드 스크립트 외부의 환경 입력

라이브러리는 런타임에 환경 변수를 읽지 않습니다. 임시 스테이징은 플랫폼의 임시 디렉토리 관례를 따르는 `tempfile` 및 `std::env::temp_dir()`를 거칩니다.

통합 테스트 하네스는 `tests/common/config.rs`에 자체 해결 체인을 가지고 있으며 다음 순서로 참조됩니다:

1. `UNIFIED_ARCHIVE_TEMP_DIR` (경로가 존재하거나 생성 가능한 경우).
2. 현재 디렉토리부터 상위로 순회하여 찾은 첫 번째 `.unified-archive.toml`의 `temp_dir` (해당 경로가 존재하거나 생성 가능한 경우). 리포지토리 루트의 추적 대상 `.unified-archive.toml`은 이를 `/Volumes/Temp/claude`로 설정합니다.
3. `std::env::temp_dir()`.

깨끗한 체크아웃 상태에서 스위트를 빌드하고 실행하는 방법은 [소스에서 unified-archive 빌드 및 테스트하기](../../../tutorials/developer/ko/build-and-test-from-source.md)에서 다룹니다.
