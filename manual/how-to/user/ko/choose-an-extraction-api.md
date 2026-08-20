---
type: How-To Guide
title: 적절한 추출 API 선택 방법
description: 추출 목표를 일치하는 Archive 메서드로 라우팅하고 잘못 선택하기 쉬운 두 가지 항목을 보여줍니다.
tags: [extraction, api, streaming]
audience: user
language: ko
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-12T11:20:31Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/choose-an-extraction-api.md }
synced_hash: 11d4b3bc34a7ee7215f54dd656537886538b4920b80c8168cbccb30d6d0292cb
---
# 적절한 추출 API 선택 방법

열려 있는 `Archive`와 거기서 꺼내고 싶은 항목이 있습니다. 10개의 퍼블릭 추출 엔트리 포인트는 선택 대상, 바이트가 도착하는 위치, 반환값, 그리고 전달하는 `ExtractionOptions` 값 중 읽히는 양에서 차이가 납니다. 이 페이지는 목표를 적절한 호출로 라우팅합니다.

전제 조건: 핸들이 읽기 모드에 있고 — 임의의 `Archive::open*` 읽기 모드 생성자(`Archive::open`, `Archive::open_encrypted`, `Archive::open_sfx`, `Archive::open_with_sfx_progress`, `Archive::open_at_offset`) 출처 — 쓰기가 허용된 대상 디렉토리가 있습니다. 아래의 두 조각은 모두 `Result<(), ArchiveError>`를 반환하는 함수 내에 들어갑니다.

## 목표별 라우팅

- 모든 항목을 디스크로 추출 — `extract_all`.
- 알려진 하나의 경로를 디스크로 추출 — `extract_file`.
- 알려진 소수의 경로를 디스크로 추출 — `extract_files`.
- 중복 경로에서도 안전해야 하는 선택 항목 추출 — `extract_by_ids`.
- 규칙(확장자, 크기, 접두사, 예산)으로 표현된 선택 항목 추출 — `extract_some`.
- 이미 이를 호출하고 있는 코드에서의 동일 작업 — `extract_filtered` (`extract_some`의 별칭).
- 메모리 상의 바이트로 된 소형 파일 — `extract_to_memory`, 또는 제한, 비밀번호, CRC 검사가 필요한 경우 `extract_to_memory_with_options`.
- 점진적으로 소비되는 대용량 파일 — `extract_to_stream`, 또는 동일한 세 가지 이유로 `extract_to_stream_with_options`. 전체 스트리밍 절차는 [대용량 엔트리 스트리밍 방법](../../../how-to/user/ko/stream-a-large-entry.md)을 참조하세요.

각 호출의 시그니처, 반환 타입, 실제로 읽히는 `ExtractionOptions` 하위 집합은 [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md) 및 [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md)에 나와 있습니다. 필드가 적용된다고 가정하기 전에 먼저 해당 문서를 확인하세요: 메모리 및 스트림 변형은 `limits`, `password`, `verify_crc32`만 읽으며, `extract_file`에는 경고 채널이 전혀 없습니다.

## 모든 호출에 상통하는 두 가지 규칙

`ExtractionOptions`는 `Clone`이 아니며 디스크 쓰기 호출은 이를 **값(by value)**으로 받습니다. 옵션을 재사용하려 하지 말고 매 호출마다 새 옵션 값을 구축하세요.

`verify_crc32: true`는 모든 libarchive 기반 포맷(TAR 패밀리, ISO, 단독 압축 스트림)에서 사전에 `ArchiveError::Unsupported`로 거부되는데, 이러한 포맷에는 엔트리별 CRC32가 포함되어 있지 않기 때문입니다. ZIP, 7z, RAR에 대해서만 설정하세요.

## 중복 경로가 있는 엔트리 선택: extract_by_ids

ZIP 및 7z 중앙 디렉토리는 동일한 경로를 두 번 나열하는 것이 허용됩니다. `extract_file` 및 `extract_files`는 추측하는 대신 해당 아카이브를 `OperationBlocked`로 거부하며, `extract_some`에 전달된 경로 매칭 조건식은 두 엔트리를 모두 선택한 다음 하나의 출력 경로에 두 엔트리를 매핑하려고 하여 사전 덮어쓰기 검사에 실패합니다. ID는 이들을 다르게 구별해 줍니다.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.zip")?;
let ids: Vec<usize> = archive
    .list_files()?
    .iter()
    .filter(|entry| entry.is_file())
    .map(|entry| entry.id)
    .collect();

let result = archive.extract_by_ids(
    &ids,
    ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    },
)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

ID는 `list_files()`에서 엔트리의 0 기반 위치이며, 그것이 나온 목록 외부에서는 의미가 없습니다. 동일한 핸들로 목록을 읽고, 선택하고, 추출하세요: 범위를 벗어난 ID는 `OperationBlocked`이지만, 오래된 범위 내 ID는 다른 엔트리를 침묵 속에 지정하게 됩니다. 중복된 ID는 하나로 합쳐지며, 빈 슬라이스는 성공(no-op)으로 처리됩니다.

## 규칙으로 엔트리 선택: extract_some

`extract_some`은 다른 선택적 호출들이 위임받는 선택적 원시 타입입니다 (AD 0029): 선택 크기에 관계없이 하나의 핸들로 아카이브를 한 번 순회합니다. 조건식은 `FnMut`이므로 누적 바이트 예산과 같은 상태를 가질 수 있습니다.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("sources.tar.gz")?;
let mut budget: u64 = 64 * 1024 * 1024;
let result = archive.extract_some(
    |entry| {
        let size = entry.size.unwrap_or(0);
        if !entry.path.ends_with(".rs") || size > budget {
            return false;
        }
        budget -= size;
        true
    },
    ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    },
)?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

입력을 신뢰할 수 없을 때 적용되는 한 가지 보안 주의사항: 압축률 가드는 *선택된* 비압축 크기를 디스크 상의 전체 아카이브 크기로 나누므로, 압축된 컨테이너(TAR.GZ, TAR.BZ2, TAR.XZ)에서 적은 양을 선택하면 진정한 엔트리별 비율보다 더 관대한 비율을 계산합니다. 비율을 신뢰하는 대신 `max_total_size`와 `max_file_size`를 한정하세요; [신뢰할 수 없는 아카이브를 안전하게 추출하는 방법](../../../how-to/user/ko/extract-untrusted-archives-safely.md)을 참조하세요. 빈 선택 항목은 경고 없이 성공을 반환하며 대상을 생성하지 않습니다.

## 다음에 볼 내용

- 모든 필드, 기본값, 제한: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).
- 모든 시그니처 및 반환 타입: [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md).
- 경고 및 에러 변형의 의미: [에러 및 경고](../../../reference/user/ko/errors-and-warnings.md).
- 어떤 포맷이 어떤 엔진에 의해 지원되며 비용이 얼마나 드는지: [여러 백엔드를 아우르는 단일 API](../../../explanation/user/ko/one-api-many-backends.md).
- 대량 경로에서만 지원되는 진행률 보고 및 취소: [진행률 보고 및 작업 취소 방법](../../../how-to/user/ko/report-progress-and-cancel.md).
