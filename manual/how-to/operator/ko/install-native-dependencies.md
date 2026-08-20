---
type: How-To Guide
title: 네이티브 빌드 의존성을 설정하는 방법
description: 플랫폼별 libarchive, pkg-config, C++ 툴체인 설정 방법 및 zstd/lz4/lzma 쓰기 필터 지원 여부 확인, RAR 지원 비활성화 방법을 설명합니다.
tags: [build, platform, formats, OI-0065-001]
audience: operator
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/how-to/operator/en/install-native-dependencies.md }
synced_hash: bf41e0723e6e992efb8ec360d7ce6d487de21d3a51a947fed285d0687486ea1f
---
# 네이티브 빌드 의존성을 충족하는 방법

`unified-archive` 0.4.0은 시스템 libarchive 및 번들된 UnRAR C++ 소스 코드를 링크합니다. 플랫폼별 설치법을 제공합니다.

## 모든 플랫폼에서 유지되는 전제 조건

- Rust 1.85 이상 (크레이트는 에디션 2024를 사용함).
- libarchive는 필수 항목입니다. `src/ffi/libarchive.rs`는 임의의 `cfg` 외부에서 `#[link(name = "archive")]`를 포함하며, `src/ffi.rs`는 조건 없이 `libarchive` 모듈을 선언하므로 선택한 Cargo 기능에 관계없이 모든 빌드에서 `archive`를 링크합니다. `--no-default-features`를 지정해도 이 요구 사항이 제거되지는 않습니다.
- 기본적으로 켜져 있는 `rar-support` 기능이 활성화되어 있는 경우 항상 C++ 컴파일러가 필요합니다.
- `make`는 필요하지 **않습니다**. `build.rs`는 C++ 컴파일러를 직접 구동하는 `cc` 크레이트를 통해 번들된 UnRAR 소스를 컴파일합니다.

## Debian 및 Ubuntu

```bash
sudo apt-get update
sudo apt-get install libarchive-dev pkg-config g++
```

`build.rs`는 Linux에서 `pkg_config::probe_library("libarchive")`를 통해 libarchive를 탐색하며 해당 탐색이 실패하면 설치 힌트와 함께 패닉을 일으키므로, 빌드 전에 `.pc` 파일을 볼 수 있는지 확인하세요:

```bash
pkg-config --modversion libarchive
```

libarchive가 기본 검색 경로 외부에 위치한 경우 `PKG_CONFIG_PATH`에 해당 `pkgconfig` 디렉토리를 추가하세요. pkg-config 크레이트는 env 메타데이터가 활성화된 상태로 실행되므로 cargo는 `PKG_CONFIG_PATH`, `PKG_CONFIG_LIBDIR`, `PKG_CONFIG_SYSROOT_DIR` 또는 `LIBARCHIVE_*` 오버라이드가 변경될 때 빌드 스크립트를 다시 실행합니다.

## Fedora 및 RHEL

```bash
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++
```

그런 다음 위와 동일하게 `pkg-config --modversion libarchive` 검사를 수행합니다. `build.rs` 내부의 Linux 탐색 경로는 두 패밀리 모두 동일합니다.

## Homebrew를 사용하는 macOS

```bash
brew install libarchive pkg-config
```

Homebrew에서 libarchive는 keg-only이므로 도움 없이 `pkg-config`가 이를 찾을 수 없습니다. `build.rs`가 이 케이스를 직접 처리합니다: `/opt/homebrew/opt/libarchive` 및 `/usr/local/opt/libarchive` 아래에서 `lib/libarchive.dylib`를 찾고, dylib을 찾으면 링크 검색 경로와 `archive` 링크 지시문을 직접 출력하며 `unified-archive: using Homebrew libarchive from …`을 출력합니다. 따라서 순정 `brew install` 레이아웃에는 환경 변수가 필요하지 않습니다.

알아둘 가치가 있는 두 가지 결과:

- 검사는 디렉토리가 아닌 dylib을 대상으로 합니다. 프리픽스 디렉토리는 남아 있지만 `lib/libarchive.dylib`가 없는 비어 있거나 부분적으로 제거된 keg는 의도적으로 무시되며, 나중에 링커 에러를 발생시킬 검색 경로를 출력하는 대신 pkg-config로 넘어갑니다.
- libarchive가 다른 위치에 완전히 존재하는 경우, 해당 폴백 경로를 이용하세요: `PKG_CONFIG_PATH`에 `pkgconfig` 디렉토리를 지정합니다. Homebrew 탐색과 pkg-config 모두 성공하지 못하면 `build.rs`는 `libarchive not found. Install it with: brew install libarchive`와 함께 패닉을 발생시킵니다.

## Windows

Windows 환경 관련 주의사항 및 빌드 안내.

**libarchive 파트는 연결되어 있지 않습니다.** `build.rs` 의 Windows 분기는 `cargo:warning=Windows libarchive linking will be configured in future implementation`을 출력하고 `rustc-link-search` 및 `rustc-link-lib`를 전혀 출력하지 않습니다. 이는 `docs/project/open-issues.md`에서 OI-0065-001로 추적되며, 여기서 탐색 메커니즘(vcpkg 자동 감지 vs 명시적 환경 변수 vs 번들링)은 여전히 미결 상태의 소유자 결정 항목입니다.

`src/ffi/libarchive.rs`가 `#[link(name = "archive")]`를 선언하므로 rustc는 이미 링커에 이름으로 `archive.lib`를 요청합니다. 검색 경로만 제공하면 됩니다. 다음 중 하나:

```cmd
vcpkg install libarchive:x64-windows
set RUSTFLAGS=-L native=C:\vcpkg\installed\x64-windows\lib
cargo build
```

또는 libarchive를 직접 번들링하고 결과로 나온 `archive.lib`를 고유한 디렉토리에 두고 동일한 방식으로 `RUSTFLAGS`가 해당 디렉토리를 가리키도록 설정하세요. 세션 내 모든 cargo 호출에 대해 `RUSTFLAGS`를 설정 상태로 유지하세요 — 변경 시 빌드 캐시가 무효화되므로 한 번 설정하고 그대로 두세요.

**RAR 파트는 Windows에서 빌드됩니다.** `build.rs`는 `CARGO_CFG_TARGET_OS` 및 `CARGO_CFG_TARGET_ENV`로부터 UnRAR 소스 목록과 전처리기 정의를 선택하며 Windows 세트는 업스트림의 `UnRARDll.vcxproj`를 반영하므로 `cc`가 MSVC로 컴파일합니다. 전제 조건은 `make`가 아닌 MSVC C++ 툴체인(Visual Studio Build Tools)입니다.

따라서 `--no-default-features`는 Windows에서 기권책이 아닌 크기와 라이선스 목적의 선택 사항입니다. 이 옵션도 libarchive에 대해서는 아무것도 해결해 주지 않습니다.

**SDK 없는 RAR 생성.** 선택 항목인 `external-rar-create` 기능은 위의 어떤 사항에도 영향을 받지 않습니다. 이 기능은 Windows 타겟에서만 컴파일되며 사용자가 직접 설치하고 라이선스를 취득한 WinRAR `rar.exe`를 실행합니다:

```cmd
cargo build --no-default-features --features external-rar-create
```

전제 조건은 라이선스가 있는 WinRAR 설치본과 탐색 가능한 `rar.exe`입니다: `RarCreator::new`는 먼저 `PATH`에 대해 `where rar.exe`를 실행한 후 `C:\Program Files\WinRAR\rar.exe` 및 `C:\Program Files (x86)\WinRAR\rar.exe`를 시도합니다. 포터블 또는 커스텀 설치본은 자동 감지되지 않으며 — `PATH`에 디렉토리를 추가하거나 탐색을 우회하는 `RarCreator::with_rar_exe_path`로 크리에이터를 만드세요.

## libarchive가 zstd, lz4, lzma를 쓸 수 있는지 확인

`TAR.ZST`, `TAR.LZ4`, `TAR.LZMA` 생성에는 각각 libzstd, liblz4, liblzma를 대상으로 빌드된 libarchive가 필요합니다. `build.rs`에서는 이를 검증하지 않으므로, 이 포맷들에 의존하기 전에 실제 링크 대상 라이브러리를 확인하세요.

libarchive에 번들되어 나오는 `bsdtar`는 컴파일되어 들어간 라이브러리들을 보고합니다:

```bash
# macOS: build.rs가 링크하는 Homebrew keg의 사본을 사용
"$(brew --prefix libarchive)/bin/bsdtar" --version

# Linux
bsdtar --version
```

```text
bsdtar 3.8.9 - libarchive 3.8.9 zlib/1.2.12 liblzma/5.8.3 bz2lib/1.0.8 liblz4/1.10.0 libzstd/1.5.7 expat/expat_2.7.4 CommonCrypto/system libb2/system
```

의미하는 바: `libzstd`가 포함되어 있으면 `TAR.ZST` 생성이 작동함을 의미하고, `liblz4`는 `TAR.LZ4`, `liblzma`는 `TAR.LZMA` 및 `TAR.XZ` 모두를 커버합니다. 나열되지 않은 이름은 컴파일 시 포함되지 않은 필터입니다.

macOS의 경우 이를 확인하기 위해 `/usr/bin/tar --version`을 실행하지 마세요. Apple 사본은 `build.rs`가 링크하는 keg가 아니라 시스템 libarchive를 보고합니다.

공유 라이브러리 자체를 대상으로 하는 동등한 검사:

```bash
# macOS
otool -L "$(brew --prefix libarchive)/lib/libarchive.dylib"

# Linux
ldd "$(pkg-config --variable=libdir libarchive)/libarchive.so"
```

의존성 목록에서 `libzstd`, `liblz4`, `liblzma`를 찾으세요.

### 실패 시 어떻게 보이는가

필터가 누락된 경우 `Archive::create`는 타겟 포맷과 libarchive 자체 메시지를 담은 `ArchiveError::Format`과 함께 생성 시점에 실패합니다. 외부 압축기로 폴백하지 않습니다. libarchive는 외부 프로그램 폴백을 등록하고 `archive_write_add_filter_zstd`(또는 `_lz4`, `_lzma`)에서 `ARCHIVE_WARN`을 반환합니다; `src/ffi/libarchive_wrapper/writer.rs`의 생성 경로는 `ARCHIVE_OK` 이외의 모든 결과를 치명적 실패로 다루며, 핸들을 해제하고 에러를 반환합니다. 따라서 불완전한 libarchive가 연결된 빌드는 하위 프로세스에 의해 작성된 아카이브 대신 아무 아카이브도 생성하지 않습니다.

특히 이름에도 불구하고 `ArchiveError::CodecUnavailable`을 보지 못할 것입니다. 그 변형은 이 경로에서 절대 생성되지 않습니다.

이 포맷들을 읽는 것은 쓰는 것과는 별개의 문제입니다; 포맷별 읽기 및 생성 컬럼은 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)에 나와 있습니다.

## 라이선스 사유로 RAR 지원 제외

`src/ffi/native/unrar` 아래 번들된 UnRAR 소스는 RARLAB 코드이며 해당 라이선스는 `src/ffi/native/unrar/license.txt`에 있습니다. 이 라이선스는 소스가 "제약 없이 무료로 RAR 아카이브를 취급하는 모든 소프트웨어에 사용"되는 것(상용 소프트웨어 포함)을 허용하지만, "RAR (WinRAR) 호환 압축기를 개발하거나 RAR 압축 알고리즘을 재현하는 데 사용할 수 없습니다". 소스를 단독으로 또는 다른 소프트웨어의 일부로 배포하려면 해당 단락의 전체 텍스트가 사용자의 라이선스(또는 라이선스를 배포하지 않는 경우 문서)와 결과 패키지의 소스 주석에 재현되어야 합니다. `rar-support`로 빌드하면 해당 소스에서 파생된 오브젝트 코드가 바이너리에 정적으로 링크되며, 이것이 재현 요구 사항이 사용자의 의무가 되는 이유입니다.

리포지토리는 `LICENSE`("Third-party: UnRAR" 섹션) 및 `src/ffi/unrar.rs` 상단의 모듈 주석에서 해당 요구 사항을 이행합니다. 재배포하는 경우 고유한 라이선스 또는 문서에 동일한 내용이 재현되어야 합니다.

이 요구 사항을 아예 부담하고 싶지 않다면 해당 소스 없이 빌드하세요:

```bash
cargo build --no-default-features
```

변경되는 사항:

- `build.rs`가 `build_unrar`를 완전히 건너뛰므로 C++ 소스가 컴파일되지 않고 `unrar` 정적 라이브러리가 링크에 들어가지 않습니다. 그러면 의존성 트리의 다른 요소가 요구하지 않는 한 C++ 컴파일러가 필요하지 않습니다.
- `src/ffi/unrar.rs` 및 `src/ffi/wrapper.rs`가 컴파일되지 않습니다.
- RAR 또는 RAR5 파일에서 `Archive::open`을 실행하면 해당 빌드에서 RAR/RAR5 지원이 비활성화되었음을 명시하는 `ArchiveError::Unsupported`를 반환합니다.
- libarchive는 여전히 필요하며, 다른 모든 포맷은 변경되지 않습니다.

Windows에서는 위에서 보여준 대로 `--features external-rar-create`를 추가하여 SDK를 배제하면서도 RAR *생성*을 유지할 수 있습니다: 해당 경로는 RAR 압축 코드를 포함하지 않으며 라이선스 문제를 사용자가 제공하는 WinRAR 설치본으로 넘깁니다.

## 다음에 볼 내용

- 플랫폼별 도구, 라이브러리, 환경 변수 및 `build.rs`가 출력하는 지시문의 전체 목록: [빌드 환경](../../../reference/operator/ko/build-environment.md).
- 각 Cargo 기능이 켜는 항목 및 MSRV 정책: [Cargo 기능 및 MSRV](../../../reference/developer/ko/cargo-features.md).
- 빌드, 테스트, 린트 및 예제에 대한 첫 엔드 투 엔드 과정: [소스에서 unified-archive 빌드 및 테스트하기](../../../tutorials/developer/ko/build-and-test-from-source.md).
