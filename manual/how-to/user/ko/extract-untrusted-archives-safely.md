---
type: How-To Guide
title: 신뢰할 수 없는 아카이브를 안전하게 추출하는 방법
description: 신뢰할 수 없는 출처에서 받은 아카이브를 추출하기 전 추출 제한, 덮어쓰기 정책 및 체크섬 검증을 설정하고 실행 결과 반환되는 경고 항목을 검사합니다.
tags: [security, extraction, config]
audience: user
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/extract-untrusted-archives-safely.md }
synced_hash: bf568a86b9d85996e8d56845bfea2e7d35005f4f23091e72eef6004f2337caa1
language: ko
---
# 신뢰할 수 없는 아카이브를 안전하게 추출하는 방법

업로드, 다운로드, 이메일 첨부 파일 등 직접 제어하지 않는 출처로부터 전달받은 아카이브가 있으며, 아카이브가 풀릴 위치나 디스크 소비량을 자체적으로 결정하지 못하도록 막으면서 내용을 디스크에 저장하고자 합니다.

기본 `ExtractionOptions`는 이미 리소스 상한선과 경로 정규화를 적용하므로 일반 `extract_all`도 무방비 상태는 아닙니다. 이 레시피에서 추가하는 것은 기본값이 아닌 *사용자의* 워크로드에 맞춘 상한선 크기 조정, 폭발 반경을 제한하는 대상 디렉토리 설정, 그리고 체크섬 검증에 대한 명시적 결정입니다.

## 사전 조건

- 아카이브가 `Archive::open`(비밀번호로 보호된 경우 `Archive::open_encrypted`)을 통해 읽기 전용으로 열려 있어야 합니다.
- 데이터 작성을 허용하고 실패 시 통째로 삭제할 의향이 있는 전용 대상 디렉토리가 있어야 합니다. 중요하게 다루는 기존 파일이 있는 트리에 추출 대상을 지정하지 마세요.
- 합당한 콘텐츠 크기가 대략 어느 정도여야 하는지 알고 있어야 합니다. 아래의 모든 상한선은 사용자가 직접 선택하는 숫자입니다; 크레이트의 기본값은 폴백일 뿐 사용자의 상황을 측정한 결과가 아닙니다.

## 1. 제한 설정 생성

`ExtractionLimits`는 퍼블릭 필드가 없습니다. 기본 제공 설정값으로 시작하는 `ExtractionLimits::builder()`부터 시작하여 변경하고자 하는 상한선만 재정의하세요.

```rust
use unified_archive::{Cap, CompressionRatio, ExtractionLimits};

let limits = ExtractionLimits::builder()
    // 추출되는 모든 엔트리의 누적 압축 해제 바이트 수.
    .max_total_size(2u64 * 1024 * 1024 * 1024)
    // 단일 엔트리의 압축 해제 바이트 수.
    .max_file_size(128u64 * 1024 * 1024)
    // 아카이브 내부의 엔트리 수.
    .max_entry_count(20_000u64)
    // 정확한 유리수 형태의 Zip-bomb 차단 기준.
    .max_compression_ratio(CompressionRatio::whole(200)?)
    // 임시 파일에 스테이징되는 SFX 페이로드의 상한선.
    .max_sfx_payload_size(Cap::Limited(1024 * 1024 * 1024))
    // 엄격한 경로 거부 선택 (아래 주의사항 참조).
    .reject_unsafe_paths(true)
    .build();
```

`?` 연산자는 실제 필요합니다: `CompressionRatio::whole`은 실패할 수 있으므로, 이 코드 조각은 `Result<_, ArchiveError>`를 반환하는 함수 내에 위치해야 합니다.

각 설정자에 대한 설명:

- `max_total_size`, `max_file_size`, `max_entry_count` 및 `max_sfx_payload_size`는 모두 `impl Into<Cap>`을 받으므로 단순 `u64` 값을 사용하면 자동으로 `Cap::Limited`가 됩니다.
- `max_compression_ratio`는 숫자가 아닌 `CompressionRatio`를 받습니다. `CompressionRatio::whole(n)`은 `n : 1`을 생성하고, `CompressionRatio::new(num, den)`은 임의의 유리수를 생성합니다. 둘 다 `Result`를 반환하며 분자나 분모가 0이면 `ArchiveError::OperationBlocked`로 거부하므로 위의 `?` 연산자가 필요합니다.
- `reject_unsafe_paths(true)`는 의도를 기록하지만 **아직 동작을 변경하지는 않습니다**. 이 플래그는 `ExtractionLimits`에 타입화된 자리가 있으며 기본값은 `false`입니다; 엄격 거부 코드 경로는 여전히 보류 상태이므로 안전하지 않은 경로 구성 요소는 어느 쪽이든 거부되기보다 정규화(수정)됩니다. 이 값을 설정하는 것은 안전하며 향후 호환성을 대비하는 것이지, 현재 의존할 수 있는 보호 기능은 아닙니다.
- `max_sfx_payload_size` 역시 기록되지만 아직 소비되지 않습니다. 기본값은 16 GiB이며, 모든 SFX 스테이징 경로(`Archive::open_sfx`, `Archive::open_with_sfx_progress`, `Archive::open_at_offset`, 및 SFX 대상 `Archive::open_encrypted`)는 이러한 엔트리 포인트가 제한 인자를 받지 않기 때문에 기본 내장 상수를 직접 읽습니다.

모든 상한선에는 명시적인 제외 방법이 있습니다. `Cap::Unlimited`는 하나의 상한선을 제거하고(`.max_total_size(Cap::Unlimited)`), `ExtractionLimitsBuilder::unlimited_compression_ratio()`는 비율 게이트를 완전히 비활성화합니다. 모든 상한선을 한 번에 비활성화하는 퍼블릭 생성자는 없습니다 — 모든 상한선 비활성화 프리셋은 크레이트 내부 전용입니다. `Cap::Unlimited`는 편의 기능이 아니라 다른 바운드가 적용되어 있음을 나타내는 선언으로 다루어야 합니다.

## 2. 덮어쓰기 정책 선택

`overwrite`는 기본적으로 `false`이며, 신뢰할 수 없는 입력에 대해서는 false가 올바른 답입니다: 임의 엔트리의 출력 경로가 이미 존재하는 경우 사전 검사에서 시작을 거부하므로, 악의적인 아카이브가 기존 파일을 조용히 교체할 수 없습니다.

`overwrite: true`는 기존 *파일*만 교체합니다. "덮어쓰기"는 파일을 교체하는 것을 의미하며 트리를 삭제하는 것이 아니므로 두 가지 케이스는 플래그와 상관없이 거부됩니다:

- 출력 경로가 이미 디렉토리로 존재하는 파일 엔트리;
- 출력 경로가 이미 파일(비디렉토리)로 존재하는 디렉토리 엔트리.

## 3. `verify_crc32` 사용 여부 결정

`verify_crc32`는 기본적으로 `false`이며 그 의미는 포맷을 처리하는 백엔드에 따라 다릅니다:

| Format family | Effect of `verify_crc32` |
|---|---|
| ZIP | 적용됨. 추출 중 엔트리별 CRC32가 검증됩니다. |
| 7z | CRC32가 항상 검증됩니다; 플래그가 아무것도 변경하지 않습니다. |
| RAR / RAR5 | CRC32가 항상 검증됩니다; 플래그가 아무것도 변경하지 않습니다. |
| Libarchive 기반 (TAR 계열, ISO, 단독 압축 스트림) | `true` 설정 시 I/O 실행 전 `ArchiveError::Unsupported`를 반환합니다. 래핑하는 코덱 자체 무결성 검사는 여전히 실행됩니다. |

따라서: ZIP을 추출하고 검사를 원할 때는 `verify_crc32: true`를 설정하세요; libarchive 기반 포맷의 경우 `true`가 no-op이 아닌 하드 오류가 되므로 `false`로 남겨두세요. 포맷을 사전에 알 수 없다면 `archive.format()`으로 분기하거나 플래그를 `false`로 두고 별도로 검증하세요. 불일치는 `ArchiveError::Corruption { path, details }`로 표출됩니다.

CRC32 일치가 증명하는 것과 증명하지 못하는 것에 대해서는 [아카이브 체크섬이 실제 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)을 참조하고, 포맷 전반에 걸쳐 작동하는 검증 패스에 대해서는 [아카이브 무결성을 검증하는 방법](verify-archive-integrity.md)을 참조하세요.

## 4. 전용 대상 디렉토리에 추출

`destination`을 다른 용도로 사용하지 않는 디렉토리로 지정하세요. 추출 과정에서 디렉토리가 없으면 생성하고, 한 번 해석(resolve)한 뒤, 해석된 출력 경로가 해당 디렉토리를 벗어나는 엔트리는 거부합니다.

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let destination = PathBuf::from("/var/tmp/incoming-42");

let options = ExtractionOptions {
    destination: destination.clone(),
    overwrite: false,
    verify_crc32: true,   // 이 예제에서는 ZIP 소스
    limits: limits.clone(),
    ..Default::default()
};

let archive = Archive::open("incoming/payload.zip")?;
let result = archive.extract_all(options)?;
```

`ExtractionOptions`는 박스형 콜백을 가지고 있어 `Clone`이 아니며 모든 추출 호출은 이를 소유권으로(by value) 받기 때문에 호출할 때마다 자체 값이 필요합니다. 위의 클론 구문은 다음 호출을 위해 `destination`과 `limits`(둘 다 `Clone` 가능)를 사용 가능한 상태로 유지합니다.

실패 후 정리할 때 알아두어야 할 두 가지 순서적 상세 사항: 리소스 제한 게이트는 대상 디렉토리가 생성되기 전에 실행되므로 거부된 압축 폭탄(bomb)은 아무것도 남기지 않는 반면, 덮어쓰기 충돌 거부는 비어 있는 대상 디렉토리를 그대로 남겨둘 수 있습니다. 어떤 에러든 발생 시 대상 트리를 삭제하는 것이 가장 단순한 정책입니다.

전체가 아닌 일부 항목만 추출하는 경우 `extract_by_ids`를 선호하세요 — 그리고 libarchive 기반 압축 포맷(TAR.GZ, TAR.BZ2, TAR.XZ)의 경우 아카이브 수준의 비율 게이트가 전체 아카이브의 디스크 크기를 분모로 사용하므로 소량 선택 추출 시 해당 게이트가 더 관대해진다는 점에 주의하세요. 이러한 경우에는 비율에 의존하는 대신 `max_total_size`와 `max_file_size`를 조이세요. [올바른 추출 호출을 선택하는 방법](choose-an-extraction-api.md)에서 엔트리 포인트 간의 선택을 다룹니다. 추가적인 함정: 인자 없는 단순 `Archive::extract_to_memory`는 항상 *기본* 제한을 적용하고 사용자의 제한을 무시합니다. 소스를 신뢰할 수 없을 때는 항상 `extract_to_memory_with_options`(또는 `extract_to_stream_with_options`)를 사용하세요.

## 5. 경고 항목 검사

`extract_all`, `extract_some`, `extract_files` 및 `extract_by_ids`는 `ResultWithWarnings<()>`를 반환합니다. 비어 있지 않은 `warnings` 벡터와 함께 반환된 `Ok`는 실행이 완료되었고 아카이브가 요청한 일부 내용이 적용되지 않았음을 의미합니다. 심볼릭 링크와 하드 링크는 재창성되는 대신 건너뛰어지며, 이는 오류가 아닌 경고로 여기서 보고됩니다.

```rust
use unified_archive::error::ArchiveWarning;

for warning in &result.warnings {
    match warning {
        ArchiveWarning::SkippedSymlink { path, target } => {
            eprintln!("symlink not created: {path} -> {target:?}");
        }
        ArchiveWarning::SkippedHardLink { path } => {
            eprintln!("hard link not created: {path}");
        }
        ArchiveWarning::OutputPathCaseCollision { first, second } => {
            eprintln!("case collision: {first} vs {second}");
        }
        other => eprintln!("{other}"),
    }
}
```

`ArchiveWarning`은 `#[non_exhaustive]`이므로 와일드카드 암을 유지하세요. 모든 변리언트는 `Display`를 구현하므로 단순히 로그를 남기고자 할 때는 이것으로 충분합니다.

무언가를 쓰기 *전에* 링크의 존재 여부가 결정을 바꾸어야 한다면 먼저 `Archive::check_symlinks()`를 호출하세요 — 목록을 스캔하고 추출 없이 링크 경고(`SkippedSymlink`, `SkippedHardLink`)를 반환합니다. 이는 엄격한 부분집합입니다: `OutputPathCaseCollision`은 덮어쓰기 충돌 사전 검사에 의해 생성되므로 실제 추출 시에만 나타납니다.

## 6. 거부 사유에 따른 매칭

이 레시피의 모든 게이트는 에러를 반환하여 거부하므로 호출자는 악의적인 아카이브와 손상된 아카이브를 구별할 수 있습니다:

```rust
use unified_archive::{ArchiveError, ExtractionOptions, Operation};

// 섹션 4의 호출이 옵션을 소비했으므로 여기서 새로운 값을 생성합니다.
let options = ExtractionOptions {
    destination,
    verify_crc32: true,
    limits,
    ..Default::default()
};

match archive.extract_all(options) {
    Ok(result) => { /* ... result.warnings 검사 ... */ }

    // 제한 위반, 또는 사전 검사에서의 네임스페이스/덮어쓰기 충돌.
    Err(ArchiveError::OperationBlocked { operation, reason }) => {
        if operation == Operation::ExtractAll.to_string() {
            eprintln!("extract_all refused: {reason}");
        } else {
            eprintln!("refused during {operation}: {reason}");
        }
    }

    // 엔트리의 해석된 출력 경로가 대상을 벗어났거나, 대상 경로 자체가 심볼릭 링크임.
    Err(ArchiveError::InvalidPath { path, reason }) => {
        eprintln!("rejected path {path}: {reason}");
    }

    // 이를 준수할 수 없는 백엔드에 대해 verify_crc32 = true가 설정됨.
    Err(ArchiveError::Unsupported { operation, format, details }) => {
        eprintln!("{operation} unsupported for {format:?}: {details:?}");
    }

    // CRC32 불일치.
    Err(ArchiveError::Corruption { path, details }) => {
        eprintln!("corrupt entry {path}: {details}");
    }

    Err(other) => eprintln!("{other}"),
}
```

`operation` 필드는 엔트리 포인트의 안정적인 라벨을 담고 있는 `String`입니다. 첫 번째 암과 같이 직접 작성한 문자열 리터럴 대신 `Operation::ExtractAll.to_string()` 등과 비교하세요; `Operation` 역시 `#[non_exhaustive]`입니다. `ArchiveError`도 `#[non_exhaustive]`이므로 와일드카드 암이 필수적입니다.

어떤 제한이 발동되었는지는 `reason`에 텍스트 형태로 들어있습니다: 너무 많은 엔트리, `max_file_size` 초과 파일, `max_total_size` 초과 누적 합계, 또는 바운드 초과 압축률. 허위 긍정처럼 보이지만 실제로는 정당한 형태 한 가지를 언급할 가치가 있습니다: 0 압축 바이트를 보고하지만 비 0 압축 해제 크기를 갖는 엔트리는 비율 위반으로 처리됩니다. 잘 형식화된 아카이브는 빈 페이로드에 대해서만 0 압축 바이트를 보고하기 때문입니다.

## 다음 단계

- 모든 필드, 타입 및 배포된 기본값: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).
- 정확한 오류 및 경고 형태: [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md).
- 이러한 게이트가 존재하는 이유, 실행 위치, 및 의도적으로 다루지 않는 범위: [추출 안전 모델](../../../explanation/user/ko/extraction-safety-model.md).
