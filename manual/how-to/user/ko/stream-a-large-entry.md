---
type: How-To Guide
title: 대용량 엔트리를 스트리밍 방식으로 읽는 방법
description: 제한된 StreamingExtractor를 통해 아카이브 엔트리를 점진적으로 읽고, 아카이브 신뢰 수준에 맞는 StreamBound를 선택합니다.
tags: [streaming, extraction, api]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/stream-a-large-entry.md }
synced_hash: 6f87dddc2950193bcac531f1013fd43a2865b74e93e85a5be8ce5c5777384d30
---
# 대용량 엔트리를 스트리밍하는 방법

전체 페이로드를 `Vec<u8>`로 수신하는 대신 `Read`를 통해 하나의 엔트리를 점진적으로 소비하고 싶을 때 `Archive::extract_to_stream`을 사용하십시오: 이를 해셔, 소켓, 디코더 또는 직접 쓰는 파일에 파이프하는 경우입니다.

## 시작하기 전에

- 읽기를 위해 아카이브를 엽니다 (`Archive::open`, 또는 암호화된 경우 `Archive::open_encrypted`). 쓰기 모드의 핸들은 아래 설명된 안전 사전 점검을 건너뜁니다.
- 엔트리 경로를 `list_files`에 나타난 그대로 정확하게 준비합니다. 단일 엔트리 게이트는 신선한 목록에 대해 경로를 해소하며 경로가 누락되었거나 두 엔트리가 이를 전달하거나 일치하는 항목이 디렉토리, 심볼릭 링크 또는 하드 링크일 때 `ArchiveError::OperationBlocked`를 반환합니다.
- 어느 백엔드가 귀하의 포맷을 처리하는지 알아둡니다. libarchive 기반 포맷만 실제로 스트리밍됩니다; 아래 "사용 중인 백엔드에 대한 계획"을 참조하십시오.
- `Archive`는 `Send`이지만 `Sync`는 아니므로 스레드당 하나의 핸들을 유지하십시오.

## 고정 크기 버퍼 루프에서 엔트리 읽기

```rust
use std::io::{Read, Write};
use unified_archive::{Archive, StreamBound};

let archive = Archive::open("media.tar")?;
let mut stream = archive.extract_to_stream("video/large.bin", StreamBound::DeclaredSize)?;
let mut sink = std::fs::File::create("large.bin")?;

let mut buffer = vec![0u8; 64 * 1024];
loop {
    let n = stream.read(&mut buffer)?;
    if n == 0 {
        break;
    }
    sink.write_all(&buffer[..n])?;
}
```

해당 루프의 `?` 연산자는 두 가지 오류 유형을 혼합합니다: `Archive::extract_to_stream`은 `ArchiveError`를 발생시키는 반면, `File::create`, `read` 및 `write_all`은 `std::io::Error`를 발생시킵니다. `ArchiveError`는 `From<std::io::Error>`를 구현하지 않으므로, 이 조각은 `Result<(), ArchiveError>`를 반환하는 함수가 아니라 `Result<(), Box<dyn std::error::Error>>`를 반환하는 함수에 속합니다.

시그니처는 `extract_to_stream(&self, file_path: &str, bound: StreamBound) -> Result<StreamingExtractor>`입니다. `StreamingExtractor`가 구현하는 유일한 트레이트는 `Read`이므로, `io::copy`, `BufReader`, 다이제스트, 디코더 등 모든 `Read` 형상의 소비자가 작동합니다.

`read`의 오류를 전파하십시오. `while let Ok(n) = stream.read(..)`를 작성하지 말고 `Err`를 입력의 끝으로 취급하지 마십시오: 상한 초과, 디코드 실패 및 실제 I/O 오류가 모두 `Err`로 도착하며, 이를 삼키면 잘리거나 적대적인 페이로드가 외견상 성공으로 바뀝니다.

리더를 반환하기 전에 호출은 엔트리 목록을 다시 작성하고 엔트리별 안전 게이트를 실행합니다: `max_file_size` 및 `max_total_size`에 대한 선언된 크기, 그리고 `max_compression_ratio`에 대한 엔트리별 압축 비율입니다. 거부는 `ArchiveError::OperationBlocked`이며 바이트를 읽지 않습니다. 선언된 크기가 없는 엔트리는 게이트를 통과합니다; 이 때 당신을 보호하는 것은 상한선(bound)입니다.

## 상한선(bound) 선택하기

`StreamBound`는 3가지 변체를 가진 `Copy` enum입니다. 전달하는 변체에 따라 반환된 리더의 출력 캡과 백엔드가 준비 단계에서 구체화할 수 있는 리소스 예산이 결정됩니다.

- `StreamBound::DeclaredSize`: 기본 선택지입니다. 스트림을 아카이브 목록에 선언된 정확한 비압축 크기로 제한합니다. 선언된 크기보다 많거나 적은 바이트는 모두 오류로 처리됩니다.
- `StreamBound::Cap(n)`: `n` 바이트 상한 설정입니다.
- `StreamBound::Unbounded`: 출력 캡을 설정하지 않으며, 백엔드가 디코딩한 바이트를 그대로 전달합니다.

두 가지 캡 설정에서 초과 생산은 오버플로 오류로 처리됩니다.

## 크기 및 진행률 추적하기

`StreamingExtractor`는 세 가지 읽기 전용 접근자를 제공합니다:

- `total_size() -> Option<u64>`: 백엔드가 보고한 전체 엔트리 크기입니다.
- `bytes_read() -> u64`: 지금까지 읽은 바이트 수입니다.
- `progress() -> Option<f64>`: 진행률(0.0 ~ 1.0)입니다.

`examples/streaming_extract.rs`는 세 가지 모두를 구동합니다: 5포인트마다 백분율을 출력하는 64 KiB 청크 루프입니다. 접근자가 상한이 없는 리더에서 실행되도록 `StreamBound::Unbounded`를 전달합니다 — 신뢰할 수 없는 입력을 읽는 프로덕션 코드는 `StreamBound::DeclaredSize`를 유지해야 합니다. `cargo run --example streaming_extract -- <archive> <entry>`로 실행하십시오.

`take_bounded(self, fallback: u64) -> io::Take<Self>`도 존재하며, 추출기를 소비하고 크기를 알 수 없을 때 `total_size()` 또는 `fallback`으로 이를 클램핑합니다. 조용한 변형입니다: `io::Take`는 제한에서 EOF에 도달하고 오류를 보고하지 않으므로, 이를 `StreamBound::Unbounded` 위에 레이어링하고 잘림에 만족할 때, 또는 관찰된 바이트 수를 예상 크기와 직접 비교할 때만 사용하십시오.

## 사용 중인 백엔드에 대한 계획

`Read` 표면은 균일합니다; 메모리 프로필은 균일하지 않습니다.

- libarchive 기반 포맷(TAR 제품군, ISO 및 독립 실행형 압축 스트림)은 스트리밍합니다. 리더는 엔트리에 위치한 libarchive 핸들을 소유하고 `read` 호출당 압축 해제된 바이트를 가져오므로, 상주 메모리는 엔트리가 아닌 버퍼를 추적합니다.
- ZIP, 7z 및 RAR은 그렇지 않습니다. 해당 백엔드는 각각 먼저 엔트리를 구체화하고 — ZIP 및 7z의 경우 `Vec<u8>`로, RAR의 경우 디스크의 임시 디렉토리를 통해 — 해당 버퍼 위의 커서를 손에 쥐어줍니다. 피크 메모리는 단일 엔트리의 크기입니다.

따라서 ZIP, 7z 및 RAR에서는 `extract_to_stream`을 메모리 바운드가 아닌 인체공학적 래퍼로 취급하십시오: 먼저 `find_entry` 또는 `list_files`를 통해 엔트리의 크기를 확인하고, 감당할 수 있는 버퍼링된 페이로드가 되도록 `ExtractionLimits::max_file_size`를 충분히 엄격하게 유지하십시오. 진정한 다중 기가바이트 입력의 경우 libarchive 기반 컨테이너를 선호하십시오. 보장이 백엔드별로 다른 이유는 [스트리밍 및 메모리 동작](../../../explanation/developer/ko/streaming-and-memory.md)에서 논의됩니다.

## 제한, 암호 또는 CRC 예상값 전달하기

`extract_to_stream_with_options(&self, file_path: &str, options: &ExtractionOptions, bound: StreamBound)`는 동일한 `StreamingExtractor`를 반환하고 `ExtractionOptions`의 세 필드를 준수합니다:

- `limits`: 안전성 사전 검사를 위한 용량 제한 옵션입니다.
- `password`: 암호화된 아카이브에 접근하기 위한 비밀번호입니다.
- `verify_crc32`: 파일별 CRC32 무결성 검증 여부입니다.

`destination`, `overwrite`, `preserve_permissions`, `preserve_times`, `filter` 및 `progress`는 디스크 쓰기 또는 다중 엔트리 순회를 설명하며 이 경로에서는 무시됩니다 — `progress`를 포함하여 무시되므로 `bytes_read()`로부터 직접 진행률을 구동하십시오.

```rust
use unified_archive::{Archive, Cap, ExtractionLimits, ExtractionOptions, StreamBound};

let options = ExtractionOptions {
    limits: ExtractionLimits::builder()
        .max_file_size(Cap::Limited(256 * 1024 * 1024))
        .build(),
    ..Default::default()
};

let archive = Archive::open("untrusted.tar.gz")?;
let stream = archive.extract_to_stream_with_options(
    "payload.bin",
    &options,
    StreamBound::DeclaredSize,
)?;
```

모든 필드와 기본값: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).

`v2-api` 기능을 사용하면 `ReadArchive::extract_to_stream` 및 `ReadArchive::extract_to_stream_with_options`가 동일한 인수를 취하고 이러한 메서드로 위임합니다.

## 다른 호출을 사용해야 할 때

전체 엔트리를 바이트로 원하면 `extract_to_memory`가 직접 이를 명시하고 상한 질문을 건너뜁니다. 디스크의 파일을 원하면 `extract_all` / `extract_some` 제품군이 진행률 및 경고와 함께 대신 작성해 줍니다. 이들 사이에서 선택하기: [올바른 압축 해제 API를 선택하는 방법](choose-an-extraction-api.md).
