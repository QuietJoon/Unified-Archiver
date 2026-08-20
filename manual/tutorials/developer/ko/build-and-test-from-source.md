---
type: Tutorial
title: 소스에서 unified-archive 빌드 및 테스트하기
description: 기여자 워크플로 첫 패스: 필수 조건, cargo build, 테스트 수트, 린트 게이트, 예제 실행 가이드입니다.
tags: [build, platform, getting-started]
audience: developer
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/tutorials/developer/en/build-and-test-from-source.md }
synced_hash: e46c1633db36188ac7f2837280aec471decb164cf61f31f3f158234f0352d302
---
# 소스에서 unified-archive 빌드 및 테스트하기

새로운 클론에서 `unified-archive` 0.4.0을 컴파일하고 테스트 수트 및 린트를 실행하는 단계를 안내합니다.

macOS 또는 Linux 환경이 필요합니다. 프로젝트에서 테스트하는 두 플랫폼이며 이 튜토리얼도 해당 환경을 기준으로 작성되었습니다. Windows에서도 빌드할 수 있지만 다른 단계가 필요합니다 — [네이티브 빌드 종속성을 충족하는 방법](../../../how-to/operator/ko/install-native-dependencies.md)에서 다룹니다.

또한 크레이트가 2024 에디션을 사용하므로 Rust 1.85 이상이 필요합니다. 확인:

```bash
rustc --version
```

```
rustc 1.85.0 (4d91de4e4 2025-02-17)
```

`1.85.0` 이상의 버전이면 됩니다. 더 이전 버전이라면 계속 진행하기 전에 툴체인을 업데이트하세요.

## 1단계 — 네이티브 사전 요구사항 설치

이 크레이트는 시스템 libarchive와 링크하고 번들로 제공되는 UnRAR C++ 소스 트리를 컴파일하므로, libarchive 개발 파일, `pkg-config`, 및 C++ 컴파일러가 필요합니다. 사용 중인 시스템에 맞는 명령어를 실행하세요:

```bash
# Debian / Ubuntu
sudo apt-get install libarchive-dev pkg-config g++

# Fedora / RHEL
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++

# macOS
brew install libarchive pkg-config
```

`make`는 필요하지 않습니다: `build.rs`가 C++ 컴파일러를 직접 구동하는 `cc` 크레이트를 통해 번들 UnRAR 소스를 컴파일합니다.

## 2단계 — 저장소 클론

```bash
git clone https://github.com/QuietJoon/unified-archive
cd unified-archive
```

## 3단계 — 빌드

```bash
cargo build
```

첫 번째 실행에서는 종속성 트리와 약 50개의 번들 C++ 번역 단위를 컴파일하므로 시간이 다소 걸립니다. 정상적인 빌드 완료는 다음과 같이 표시됩니다:

```
   Compiling unified-archive v0.4.0 (/home/you/unified-archive)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 47s
```

Cargo는 빌드가 실패하지 않는 한 빌드 스크립트의 자체 진행 상황 메시지를 숨깁니다. 이를 확인하려면 `cargo build -vv`를 실행하고 `unified-archive: building vendored UnRAR (cc) for target_os=linux`와 같은 라인을 확인하세요.

libarchive가 누락된 경우 빌드 스크립트는 설치할 패키지 이름을 안내하는 메시지와 함께 중단됩니다(예: macOS에서는 `libarchive not found. Install it with: brew install libarchive`). 1단계로 돌아가서 다시 `cargo build`를 실행하세요.

## 4단계 — 전체 테스트 수트 실행

```bash
cargo test
```

이것은 규모가 큰 테스트 수트입니다. `tests/` 바로 아래에 27개의 독립형 테스트 파일이 존재하고, 두 개의 추가 엔트리 포인트(`tests/integration_tests.rs` 및 `tests/contract_tests.rs`)가 전체 `tests/integration/` 및 `tests/contract/` 트리를 가져오며, 그 위에 cargo가 `src/`에 컴파일된 단위 테스트와 모든 문서 예제를 실행합니다. 몇 분 정도 소요될 수 있습니다.

각각은 자체 테스트 바이너리가 되어 별도로 보고합니다: `running N tests` 라인, 각 테스트가 끝날 때마다 출력되는 문자 하나, 그리고 다음 형태의 요약 라인:

```
test result: ok. N passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.42s
```

모든 바이너리가 `0 failed`와 함께 `test result: ok.`로 끝나고 `cargo test` 명령어 자체가 종료 코드 `0`으로 종료되면 성공적인 실행입니다.

성공적인 실행 중 두 종류의 메시지는 정상적이며 실패가 아닙니다. `zip`, `unzip`, `7z`, `tar` 또는 `rar.exe`를 외부 실행하는 테스트는 해당 툴이 설치되어 있지 않을 때 스킵 라인을 출력합니다. 시간을 측정하는 테스트는 요청하지 않는 한 비활성화 상태를 유지합니다: 성능 베이스라인은 환경 변수 `UA_PRINT_PERF_BASELINE=1`이 필요하고, 계약 수트의 하나의 성능 단언은 `UA_RUN_PERF_CONTRACT_TEST=1`이 필요합니다.

## 5단계 — 단일 테스트 파일만 실행

변경 작업을 진행하는 동안 전체 수트를 실행하는 경우는 드뭅니다. `tests/` 바로 아래의 모든 파일은 `.rs`가 제거된 파일 이름에 따라 자체 cargo 테스트 타겟이 됩니다:

```bash
cargo test --test extraction_test
```

```
running 7 tests
.......

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
```

해당 파일에는 7개의 테스트가 들어 있으며 모두 RAR 케이스이므로 기본 기능 세트가 적용되는 한 7개 모두 실행됩니다. 실행 간에는 타이밍만 변경됩니다.

`tests/integration/` 하위의 엔드투엔드 수트는 모두 단일 `integration_tests` 타겟을 통해 실행되고, `tests/contract/` 하위의 퍼블릭 API 계약 수트는 `contract_tests`를 통해 실행됩니다:

```bash
cargo test --test integration_tests
```

더 구체적으로 좁히려면 `--` 뒤에 필터를 추가하세요(예: `cargo test --test integration_tests -- extraction`).

## 6단계 — 린트 게이트 통과

```bash
cargo clippy --all-targets
```

`--all-targets`는 clippy가 라이브러리뿐만 아니라 테스트, 예제, 벤치마크도 검사하도록 합니다. 성공적인 실행은 `Finished`를 출력하며 `warning:`으로 시작하는 라인이 없습니다.

## 7단계 — 포맷팅 게이트 통과

```bash
cargo fmt --check
```

성공적인 실행은 아무것도 출력하지 않고 `0`으로 종료됩니다. diff가 출력되면 `cargo fmt`를 실행하여 적용하세요.

## 8단계 — 예제 실행

크레이트는 자체 바이너리가 없는 라이브러리이지만 `examples/`에 실행 가능한 프로그램들이 있습니다. 저장소에 이미 존재하는 픽스처 아카이브를 검사기에 전달하세요:

```bash
cargo run --example inspect_archive -- tests/fixtures/test.zip
```

```
Archive: tests/fixtures/test.zip
Format:  Zip
Entries: 1

Path                                                       Size   Compressed      CRC32
----------------------------------------------------------------------------------------
test_file.txt                                                18           18   054607BC
```

저장된 단일 18바이트 엔트리가 픽스처의 전체 내용입니다. 포맷이 `Zip`으로 감지되고 중앙 디렉토리에서 CRC32를 읽어내는 것은 라이브러리, 방금 빌드한 백엔드, 그리고 libarchive와의 링크가 모두 함께 정상 작동함을 의미합니다.

## 9단계 — RAR 지원 없이 빌드

`rar-support`는 크레이트의 기본 기능 세트에 포함되어 있으며, 이것이 3단계에서 번들 UnRAR 소스를 컴파일한 이유입니다. 이 기능 없이 한 번 빌드해 보세요:

```bash
cargo build --no-default-features
```

이 컴파일은 C++ 소스를 완전히 건너뛰므로 눈에 띄게 빠릅니다. 결과 빌드에서 RAR 또는 RAR5 파일에 대해 `Archive::open`을 호출하면 이 빌드에서 RAR/RAR5 지원이 비활성화되었다는 `Unsupported` 에러를 반환합니다. 다른 모든 포맷은 이전과 동일하게 동작합니다. libarchive는 여전히 필요합니다 — `--no-default-features` 플래그는 libarchive에 영향을 주지 않습니다.

이제 정상적으로 동작하는 개발 루프가 준비되었습니다. 무언가를 변경했을 때의 순서는 3단계, 수정한 파일에 대한 5단계, 커밋 전 4단계, 그 후 6단계 및 7단계입니다.

## 다음 단계

- 모든 기능 플래그, 활성화하는 항목 및 MSRV 정책: [Cargo 기능 및 MSRV](../../../reference/developer/ko/cargo-features.md).
- 빌드가 참조하는 도구, 라이브러리 및 환경 변수의 전체 목록: [빌드 환경](../../../reference/operator/ko/build-environment.md).
- Windows 및 libarchive 코덱 검사를 포함한 플랫폼별 사전 요구사항 가이드: [네이티브 빌드 종속성을 충족하는 방법](../../../how-to/operator/ko/install-native-dependencies.md).
- 작성한 변경 사항이 기록되는 방식, 그리고 결정 기록과 검토 결과가 보관되는 위치: [이 프로젝트가 결정을 기록하는 방법](../../../explanation/developer/ko/decision-records-and-reviews.md).
