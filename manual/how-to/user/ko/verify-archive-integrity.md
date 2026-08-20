---
type: How-To Guide
title: 아카이브의 무결성을 검증하는 방법
description: validate_integrity, verify_crc32, 독립형 스트림 체크섬 헬퍼 및 콘텐츠 다이제스트를 사용해 아카이브를 검증하는 레시피입니다.
tags: [integrity, extraction, streaming, api]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/verify-archive-integrity.md }
synced_hash: d4e765514ca7090f6faa3fdf16bc83490b4475edf5db30f171a7072023e7be4f
---
# 아카이브의 무결성을 검증하는 방법

단일 "검증(verify)" 호출은 존재하지 않습니다. 이 크레이트는 각각 서로 다른 질문에 답하고 백엔드별로 동작이 다른 다섯 가지 고유한 무결성 작업을 제공합니다. 원하는 질문에 답하는 작업을 선택하고 아래 주의 사항에 따라 결과를 읽으십시오.

아래의 네 가지 `Archive` 작업(`validate_integrity`, `verify_crc32`를 사용한 추출, `calculate_manifest_digest`, 및 `calculate_archive_crc`)에는 읽기 모드 핸들(`Archive::open`, 또는 암호화된 아카이브의 경우 `Archive::open_encrypted`)이 필요하며, 쓰기 모드 핸들은 `ArchiveError::WriteModeOnly`로 실패합니다. `Archive`는 `Send`이지만 `Sync`는 아니므로 핸들당 하나의 스레드에서 검증하십시오. 독립형 스트림 검사합 헬퍼는 경로를 사용하며 핸들이 전혀 필요하지 않습니다.

여기에 언급된 모든 오류 변형은 [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md)에 설명되어 있습니다. 시그니처 및 재내보내기 경로는 [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md)을 참조하십시오. 이러한 보증이 중단되는 지점에 대해서는 [아카이브 검사합이 실제로 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)을 참조하십시오.

## 아카이브의 모든 파일 항목 테스트

`Archive::validate_integrity`는 아카이브를 탐색하고 각 일반 파일 항목의 페이로드를 디코딩하며 탐색 과정에서 살아남지 못한 항목을 보고합니다. 디스크에는 아무것도 쓰이지 않습니다.

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.zip")?;
let report = archive.validate_integrity()?;

if report.failed.is_empty() {
    println!("{} of {} file entries verified", report.validated, report.total_files);
} else {
    for path in &report.failed {
        eprintln!("damaged: {path}");
    }
}
```

깨끗한 보고서를 합격으로 취급하기 전에 `failed`에서 분기하되 `total_files`를 먼저 테스트하십시오: `validated`는 `total_files - failed.len()`으로 계산되므로 파일 항목이 없는 아카이브에서 비어 있는 `failed`는 아무것도 검사되지 않았음을 의미합니다. 유형이 `File`인 항목만 대상이 되므로 `total_entries - total_files`는 모든 백엔드가 건너뛰는 디렉터리, 심볼릭 링크 및 하드 링크의 수를 집계합니다. 네 가지 필드와 해당 유형은 모두 [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md)에 나와 있습니다.

결과에 따라 조치를 취하기 전에 결정해야 할 두 가지 사항:

- **`Err`와 비어 있지 않은 `failed`는 서로 다른 답입니다.** 비어 있지 않은 `failed`는 해당 항목이 손상되었음을 의미합니다. `Err`는 검사를 완료할 수 없으며 항목별 판정이 없음을 의미합니다: 아카이브 파일 자체의 I/O 실패, 누락되거나 잘못된 RAR 비밀번호, 또는 백엔드가 파일 항목 수보다 더 많은 실패를 보고하는 경우(뺄셈을 포화시키는 대신 `ArchiveError::Corruption`을 반환함).
- **`failed` 항목이 증명하는 바는 형식에 따라 다릅니다.** ZIP은 각 항목의 계산된 CRC32를 저장된 중앙 디렉터리 값과 비교합니다; 7z 및 RAR/RAR5는 디코딩할 때 자체 디코더가 무결성을 검사하도록 합니다(UnRAR은 테스트 모드를 통해 디스크에 아무것도 쓰지 않음); libarchive 기반 형식은 항목별 CRC32를 전혀 포함하지 않으므로 거기서의 실패는 저장된 검사합이 일치하지 않는 것이 아니라 감압기가 바이트를 거부했음을 의미합니다. AE-2 AES ZIP 항목은 CRC 비교에서 제외되지만(저장된 값은 사양상 `0`임) 여전히 스트리밍되므로 AES 인증 검사가 실행되고 실패는 손상된 항목으로 표시됩니다. 이러한 각 메커니즘이 증명하는 것과 증명하지 못하는 것은 [아카이브 검사합이 실제로 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)에서 논의됩니다.

## 추출 중 항목별 CRC32 요청

`ExtractionOptions::verify_crc32`의 기본값은 `false`입니다. 이를 설정한다고 해서 검증이 일률적으로 켜지고 꺼지는 것은 아닙니다 — 이에 의존하기 전에 백엔드별 실제 동작을 읽으십시오.

```rust
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("release.zip")?;
archive.extract_all(ExtractionOptions {
    destination: "out".into(),
    verify_crc32: true,
    ..Default::default()
})?;
```

이를 설정할 수 있는지 여부를 결정하는 두 가지 제약 조건:

- libarchive 기반 형식(TAR 패밀리, ISO, 원시 압축 스트림)에서는 `true`가 I/O가 실행되기 전에 `ArchiveError::Unsupported`로 거부되므로 거기서는 `false`를 전달하십시오. 감싸는 코덱 자체의 검사는 추출 중에 여전히 실행됩니다.
- 7z 및 RAR/RAR5의 경우 디코더는 요청 여부에 관계없이 CRC32를 검증하므로 `false`는 아무것도 끄지 않습니다. ZIP은 플래그가 변경되는 유일한 백엔드이며 디스크 쓰기 경로에서만 적용됩니다: ZIP의 `extract_to_memory` 및 `extract_to_stream`은 플래그와 관계없이 저장된 값과 비교합니다(어느 쪽이든 AE-2 자리 표시자 `0` 값은 제외됨).

플래그는 모든 추출 진입점에서 사전에 검증되므로 지원되지 않는 요청은 아무것도 쓰이기 전에 실패합니다. 백엔드별 표는 [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md)에 있습니다.

불일치는 항목 이름을 포함하여 세부 정보가 `CRC32 mismatch: expected <hex>, got <hex>`로 읽히는 `ArchiveError::Corruption`으로 표출됩니다. 추출은 거기서 중단됩니다; 이미 작성된 항목은 디스크에 남아 있습니다.

## 독립형 .gz, .bz2 또는 .xz의 컨테이너 검사합 읽기

단일 파일 압축 스트림의 경우 압축을 해제하지 않고 파일을 `Archive`로 열지 않고도 형식의 헤더 또는 트레일러에 저장된 검사합을 읽을 수 있습니다. 콘텐츠에 따라 디스패치하려면 `extract_stream_checksum`을 사용하거나 형식별 헬퍼를 직접 호출하십시오.

```rust
use unified_archive::stream_crc::{CheckType, extract_stream_checksum};

let sum = extract_stream_checksum("backup.sql.gz")?;
match sum.check_type {
    CheckType::Crc32 => println!("stored CRC32: {:08X}", sum.crc32_value().unwrap()),
    CheckType::Crc64 => println!("stream declares CRC64"),
    CheckType::Sha256 => println!("stream declares SHA-256"),
    CheckType::None => println!("stream declares no check"),
    CheckType::Unknown => println!("unrecognised check id"),
}
```

`extract_stream_checksum`은 매직 바이트에서만 형식을 감지합니다. 파일 확장자는 참조되지 않으므로 이름이 변경된 파일은 올바르게 라우팅되고 매직이 일치하지 않는 파일은 형식 오류로 거부됩니다. gzip, bzip2, xz를 허용합니다; 6개의 매직 바이트를 샘플링하기에 너무 짧은 파일을 포함하여 다른 모든 것은 오류입니다.

원시 필드보다 접근자(accessor)를 선호하십시오. `StreamChecksum::crc32_value`는 `check_type`이 `Crc32`일 때만 CRC32를 반환하고, `crc64_value`는 `Crc64`일 때만 반환합니다; 일반 `crc32` / `crc64` / `uncompressed_size` 필드는 선언된 검사 유형이 보증하지 않는 값을 보관할 수 있는 옵션 가방입니다.

각 헬퍼가 실제로 검색하는 내용:

- `extract_gzip_stream_crc`는 고정 헤더(매직, deflate 방법, 예약된 플래그 비트 클리어)를 검증한 다음 8바이트 트레일러를 읽습니다: `check_type`은 `Crc32`이고, `crc32`는 저장된 값이며, `uncompressed_size`는 트레일러 `ISIZE` — **모듈로 2^32** 크기이므로 4GiB 미만에서만 정확합니다. 파일 끝의 트레일러만 읽히므로 다중 멤버 gzip(`pigz`, `bgzip` 또는 `cat a.gz b.gz`)에서 값은 **마지막 멤버에 대해서만** 설명합니다.
- `extract_bzip2_stream_crc`는 `BZh` 헤더 및 해당 블록 크기 숫자를 검증한 다음 bzip2 블록이 정렬 패딩 없이 비트 팩되어 있으므로 먼저 바이트 정렬된 다음 모든 비트 오프셋에서 끝 마커를 마지막 킬로바이트에서 검색합니다. `check_type`은 `Crc32`이고 `crc32`는 마커 바로 뒤에서 빅 엔디안으로 읽은 결합된 스트림 CRC입니다. `uncompressed_size`는 항상 `None`입니다 — bzip2는 이를 기록하지 않습니다. 연결된 스트림에서 값은 **마지막** 발견된 마커에서 가져옵니다.
- `extract_xz_stream_check`는 12바이트 스트림 헤더만 읽고 예약된 플래그 비트가 설정되어 있지 않고 저장된 플래그 CRC32가 일치하지 않는 한 거부합니다. 선언된 검사 유형 — `None` (id 0x00), `Crc32` (0x01), `Crc64` (0x04), `Sha256` (0x0A), 또는 다른 id에 대한 `Unknown` — 을 보고하고 `crc32`, `crc64`, `uncompressed_size`를 모두 `None`으로 반환합니다. **xz에 대해서는 검사 값이 전혀 추출되지 않습니다**, 유형이 `Crc32`인 경우에도 마찬가지입니다; 이를 구하려면 블록과 인덱스를 파싱해야 합니다. 첫 번째 스트림 헤더만 읽기 때문에 다중 스트림 `.xz`는 **첫 번째 스트림의** 유형만 보고합니다.

이러한 호출 중 어느 것도 아무것도 재계산하지 않습니다. 파일이 자체에 대해 주장하는 바를 알려주며, 이는 이전에 기록한 값과 비교하는 데 유용하고 지속적으로 재작성된 파일을 감지하는 데는 쓸모가 없습니다. 바이트가 실제로 검사되도록 하려면 디코딩하십시오: 파일을 `Archive`로 열고 `validate_integrity`를 호출하여 libarchive가 압축을 해제하는 동안 컨테이너 검사를 검증하도록 하십시오.

## 콘텐츠별 두 아카이브 비교

매니페스트 다이제스트 계산 시 동일 경로 중복 엔트리의 처리 메커니즘을 설명합니다.

```rust
use unified_archive::Archive;

let a = Archive::open("backup.zip")?;
let b = Archive::open("backup.7z")?;
if a.calculate_manifest_digest()? == b.calculate_manifest_digest()? {
    println!("same file contents");
}
```

코드에서 처리해야 할 사항:

- `calculate_manifest_digest`
- `calculate_content_multiset_digest`

전체 압축 해제 크기도 원하는 경우 두 번 대신 한 번 탐색하여 `(String, u64)`를 반환하는 `calculate_content_multiset_digest_and_size`를 호출하십시오. `calculate_manifest_digest` 및 `calculate_manifest_summary`는 이에 대한 얇은 심(shim)입니다; 매니페스트 데이터가 참여하지 않으므로 "매니페스트"라는 이름은 역사적이며 오해의 소지가 있습니다.

## 저렴한 아카이브 수준 CRC 가져오기

`Archive::calculate_archive_crc`는 모든 항목의 이미 저장된 CRC32의 래핑 산술합으로 — 7-Zip이 아카이브 CRC로 표시하는 값입니다. 목록 메타데이터만 읽고 아무것도 디코딩하지 않습니다.

```rust
use unified_archive::Archive;

let archive = Archive::open("file.7z")?;
println!("archive CRC: {:08X}", archive.calculate_archive_crc()?);
```

무결성 검증 결과 활용 가이드.

반환된 `0`은 세 가지 방식으로 모호하며 호출 시 구별할 수 없습니다: 아카이브가 비어 있음; CRC32를 노출한 항목이 없어 아무것도 합산되지 않음; 또는 합계가 실제로 `0`으로 래핑됨(유효한 CRC32이자 유효한 합계임). "검사합 생성 가능 항목 없음"과 실제 0을 구별해야 할 때 `calculate_manifest_digest`를 사용하십시오. 또한 덧셈은 손실이 발생합니다 — 두 항목의 CRC32 값을 교환해도 합계는 변경되지 않고 유지됩니다.

## 결과를 "검증됨"으로 보고하기 전에

이러한 호출 중 어느 것도 저작권을 증명하지 못합니다. 일치하는 검사합은 바이트가 옆에 저장된 값과 일치한다는 것만 알려주며 바이트를 재작성할 수 있는 사람이라면 누구나 검사합을 재작성할 수 있습니다. 어떤 호출이 무엇을 증명하는지, 각 호출이 포착하는 손상 유형은 무엇인지, 이러한 모든 보증이 끝나는 위치는 [아카이브 검사합이 실제로 증명하는 것](../../../explanation/user/ko/checksums-and-integrity.md)에 제시되어 있습니다. 호출자에게 아카이브가 온전하다고 약속하기 전에 해당 내용을 읽으십시오.
