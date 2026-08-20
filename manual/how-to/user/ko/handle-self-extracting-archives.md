---
type: How-To Guide
title: 자가 추출 아카이브 감지 및 열기 방법
description: SFX 실행 파일을 감지하고, 감지 판정을 읽고, 포함된 페이로드를 열거나 스테이징하며, 검사를 위해 스텁 바이트를 추출합니다.
tags: [sfx, archive, extraction]
audience: user
language: ko
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/handle-self-extracting-archives.md }
synced_hash: 78693aad09454d0e8b7b0ddb307dd25cc5700e760dc1f1848aacbe20be2c3cde
---
# 자가 추출 아카이브 감지 및 열기 방법

이 페이지에서는 파일이 자가 추출 아카이브(SFX)인지 묻고, 답에 따라 조치를 취하고, 파일의 두 부분 중 하나에 접근하는 방법을 다룹니다. 여기에 포함된 어떠한 내용도 스텁을 실행하지 않습니다.

## 시작하기 전에

필요한 유형은 크레이트 루트에서 재내보내집니다:

```rust
use unified_archive::{Archive, ArchiveError, SfxConfidence, SfxStagingProgress, StubType};
```

`Archive::open`은 자체적으로 이미 SFX 경로로 라우팅됩니다: 매직 바이트 감지가 실패하고 경로에 실행 가능한 확장자(`.exe`, `.com`, `.scr`, `.app`, `.run`, `.sh`, `.bash`)가 포함되어 있으면 SFX로 재시도하고 이 또한 실패하면 두 실패를 함께 보고합니다. `Archive::open_encrypted`는 비밀번호를 적용하기 전에 동일한 작업을 수행합니다. 따라서 판정 자체를 원하거나, 폴백이 다루지 않는 확장자를 가진 파일이거나, 이미 페이로드 오프셋을 알고 있는 경우 아래 호출을 사용하십시오.

`cargo run --example detect_sfx -- <file>`은 단일 파일에 대한 전체 판정을 출력합니다; 소스는 `examples/detect_sfx.rs`에 있습니다.

## 감지

```rust
let detection = Archive::detect_sfx("installer.exe")?;
if !detection.is_sfx() {
    println!("{}", detection.summary()); // "Not a self-extracting archive"
    return Ok(());
}
```

`Archive::detect_sfx`는 파일의 제한된 접두사를 읽고 포함된 아카이브를 결코 열지 않습니다. I/O 문제(누락되거나 읽을 수 없는 경로)에 대해서만 `Err`를 반환합니다; "SFX가 아님"은 `is_sfx() == false`인 성공적인 `Ok` 결과입니다.

## 판정 읽기

패키지화된 접근자(accessor)를 선호하십시오 — 이 접근자는 세 가지 페이로드 사실을 함께 전달하거나 하나라도 누락되면 `None`을 전달합니다:

```rust
let Some((format, offset, stub)) = detection.payload_coordinates() else {
    return Ok(()); // not an SFX, or an incomplete result
};
println!("{format:?} payload at byte {offset}, behind a {}", stub.description());
```

로그 라인의 경우 `summary()`는 전체 판정을 하나의 문자열(예: `SFX detected (probable): Windows PE executable stub, Zip archive at offset 8192`)로 렌더링합니다. 남아 있는 접근자 — 개별 형식, 오프셋, 스텁 유형 및 신뢰성 게터, 그리고 그대로 로깅할 가치가 있는 `evidence()` 추적 — 는 [퍼블릭 API 표면](../../../reference/user/ko/public-api-surface.md)에 해당 유형과 함께 열거되어 있습니다.

`is_confirmed()`에서 동작을 게이팅하지 마십시오: 라이브 감지기는 `NotSfx` 및 `Probable`만 생성하므로 모든 실제 감지에 대해 `false`입니다. 페이로드를 확정(confirm)하는 방법은 페이로드를 여는 것입니다 — 아래를 참조하십시오.

판정을 사람에게 보여주는 경우 `StubType::is_native()`는 현재 사용 중인 플랫폼에서 해당 스텁 종류가 실행되는지 여부를 알려주며, 이를 통해 사용자에게 Windows-PE 인스톨러가 해당 Linux 머신에서 실행되지 않는다는 점을 경고할 수 있습니다. 이는 *페이로드*를 추출할 수 있는지 여부에 대해서는 아무것도 알려주지 않습니다; 그것은 플랫폼 독립적입니다.

## 페이로드 열기

```rust
let archive = Archive::open_sfx("installer.exe")?;
for entry in archive.list_files()? {
    println!("{} ({:?} bytes)", entry.path, entry.size);
}
```

`Archive::open_sfx`는 감지한 다음 페이로드 테일을 임시 파일로 복사하고 일반 파이프라인을 통해 이를 엽니다. 임시 파일은 반환된 `Archive`가 드롭될 때 삭제됩니다. 반환된 핸들에서:

- `path()`는 스테이징된 사본이 아니라 전달한 외부 경로를 여전히 보고합니다.
- `format()`은 *페이로드* 형식을 보고하는 반면 `extension_format()`은 외부 경로의 확장자를 검사하므로 실제 SFX에 대해 일반적으로 `None`을 반환합니다. 이 조합은 확장자/콘텐츠 불일치가 아닙니다.

SFX가 아닌 입력은 메시지가 `File is not a self-extracting archive: <path>`로 읽히는 `ArchiveError::Format`을 제공합니다. 암호화된 페이로드의 경우 외부 경로에서 `Archive::open_encrypted`를 호출하고 비밀번호가 적용되기 전에 SFX 폴백이 페이로드를 스테이징하도록 하십시오.

이전 감지에서 캐싱했거나 벤더 문서에서 확인했거나 이 크레이트가 스캔하지 않는 컨테이너 형식에서 가져와서 오프셋을 이미 알고 있는 경우 감지를 완전히 건너뛰십시오:

```rust
let archive = Archive::open_at_offset("installer.exe", 8192)?;
```

`open_at_offset`은 사용자가 제공한 숫자를 신뢰합니다: 스텁 분류 및 시그니처 선별이 실행되지 않으므로 해당 오프셋의 바이트가 아카이브를 진정으로 시작해야 하며 그렇지 않으면 백엔드 오픈이 실패합니다. `offset == 0`은 `Archive::open`으로 바로 연결되며 복사를 수행하지 않습니다. 파일 끝 또는 그 너머의 오프셋은 `ArchiveError::Format`입니다.

## 진행 상황 보고 및 스테이징 취소

수 기가바이트 인스톨러를 스테이징하는 것은 눈에 띄는 복사 작업입니다. `Archive::open_with_sfx_progress`는 `open_sfx`에 해당 복사에 대한 훅(hook)을 더한 것입니다:

```rust
let progress = SfxStagingProgress::new(|copied| {
    eprintln!("staged {copied} bytes");
});
let archive = Archive::open_with_sfx_progress("installer.exe", Some(progress))?;
```

콜백은 *누적* 바이트 수를 수신하며 쓰기 청크당 한 번 트리거됩니다. `SfxStagingProgress::new`는 관찰 전용 클로저를 받습니다. 복사를 중단할 수 있으려면 `SfxStagingProgress::with_cancel`로 훅을 빌드하고 `false`를 반환하십시오:

```rust
let progress = SfxStagingProgress::with_cancel(move |copied| {
    copied < 512 * 1024 * 1024 // give up past 512 MiB
});
match Archive::open_with_sfx_progress("installer.exe", Some(progress)) {
    Ok(archive) => { /* … */ }
    Err(ArchiveError::Cancelled { operation }) => {
        assert_eq!(operation, "sfx_staging");
    }
    Err(other) => return Err(other),
}
```

부분적인 임시 파일은 알아서 제거됩니다. 훅으로 `None`을 전달하는 것은 `open_sfx`와 정확히 동일합니다. 취소 모양 및 더 넓은 진행 이야기: [진행 상황 보고 및 작업 취소 방법](report-progress-and-cancel.md).

## 스텁 바이트 추출

```rust
let stub = Archive::extract_stub("installer.exe", &detection)?;
std::fs::write("stub.bin", &stub)?;
```

파일 선두의 `data_offset` 바이트를 그대로 받게 됩니다 — 페이로드가 전혀 없는 실행 가능 접두사입니다. PE, ELF 또는 Mach-O 스텁의 경우 해당 바이트는 그 자체로 완전한 실행 가능 이미지이며, 이것이 바로 크레이트가 이를 실행하지 않으며 사용자도 실행해서는 안 되는 이유입니다: 샌드박스에서 파일을 분석하십시오.

호출에 대한 두 가지 제약 조건:

- 동일한 경로에 대해 실제로 취득한 `SfxDetectionResult`를 전달하십시오. `extract_stub`는 내부적으로 감지를 재실행하고 제공한 결과의 오프셋과 새 오프셋이 다르면 `ArchiveError::Format`으로 거부합니다. 수동으로 생성되거나 오래된 결과로는 읽기를 선택한 오프셋으로 유도할 수 없습니다.
- 호출 도중에 파일이 변경되어서는 안 됩니다. `extract_stub`는 읽기 전후에 파일의 식별성을 검사하고 검증되지 않은 바이트를 반환하기보다 실패합니다.

## 이 호출들이 조정하지 않는 제한 사항

- **스캔 창 — 1 MiB.** 감지는 파일의 첫 1 MiB에서만 페이로드 시그니처를 탐색합니다. 해당 오프셋을 지나 시작하는 페이로드를 가진 SFX는 `is_sfx() == false`를 보고합니다; 오류도 경고도 없습니다. 다른 소스에서 오프셋을 가져온 경우 `open_at_offset`은 여전히 작동합니다 — 창은 열기가 아닌 *감지*를 한정합니다.
- **페이로드 스테이징 한계 — 16 GiB.** `open_sfx` 및 `open_at_offset`은 페이로드 크기와 최대값을 명시하는 `ArchiveError::Format`과 함께 16 GiB보다 큰 페이로드를 거부하며 임시 파일을 생성하기 *전에* 거부하므로 임시 디렉터리에 아무것도 남지 않습니다. 해당 숫자는 이 진입점들에 대해 크레이트 내에 고정되어 있습니다: `ExtractionLimits`는 `max_sfx_payload_size` 노브를 포함하지만 스테이징 경로는 제한 설정 대신 내장 기본값을 읽으므로 거기서 값을 올리거나 내리는 것은 `open_sfx`에 아무런 영향을 주지 않습니다. 복사 중간에 페이로드 크기가 변경되거나 감지와 스테이징 사이에 파일이 교체된 경우에도 스테이징은 실패합니다.
- **`Probable`은 받을 수 있는 가장 강력한 판정입니다.** 감지 중에는 포함된 아카이브가 디코딩되지 않으므로 성공적인 `open_sfx`(또는 핸들에서의 성공적인 `list_files`)를 유일한 확정으로 처리하고 해당 오류를 반박으로 처리하십시오. 양성 판정이 배제하는 것과 배제하지 못하는 것: [SFX 감지가 결정하는 방식](../../../explanation/developer/ko/sfx-detection-pipeline.md).

이 파이프라인의 그 어떤 항목도 어떤 인스톨러 제품이 파일을 빌드했는지 알려주지 않으며, 크레이트의 그 어디에서도 스텁을 실행하지 않습니다.

## 관련 항목

- 감지가 단계별로 수행되는 이유, 각 단계가 배제하는 항목, 판정이 점수가 아닌 열거형인 이유: [SFX 감지가 결정하는 방식](../../../explanation/developer/ko/sfx-detection-pipeline.md).
- SFX 페이로드로 나타날 수 있는 형식과 각 백엔드가 지원하는 항목: [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md).
- API 전반에서의 진행 상황 콜백 및 취소: [진행 상황 보고 및 작업 취소 방법](report-progress-and-cancel.md).
