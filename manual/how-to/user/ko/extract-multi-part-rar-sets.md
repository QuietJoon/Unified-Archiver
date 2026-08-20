---
type: How-To Guide
title: 분할 RAR 세트 압축 해제 방법
description: 분할 RAR 세트의 올바른 볼륨을 열고, detect_multipart로 세트를 확인한 후 단 한 번의 호출로 압축을 해제합니다.
tags: [extraction, formats, archive]
audience: user
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/extract-multi-part-rar-sets.md }
synced_hash: f5ba137813f305f024d7c9d0f0243e819d5f18a525d6f03bb9764e5902f84c2d
language: ko
---
# 분할 RAR 세트 압축 해제 방법

RAR 및 RAR5는 이 크레이트가 처음부터 끝까지 분할 볼륨을 읽을 수 있는 유일한 포맷입니다. 하나의 볼륨을 열면 UnRAR 백엔드가 세트의 나머지 부분을 스스로 순회하므로, 볼륨 목록을 백엔드에 직접 전달할 필요가 없습니다.

전제 조건: 기본 `rar-support` Cargo 기능이 활성화되어 있어야 하며 (활성화되지 않은 경우 RAR을 열면 해당 기능을 명시하는 `ArchiveError::Unsupported` 오류가 발생함), 세트의 모든 볼륨이 원래 이름으로 동일한 디렉터리에 위치해야 하고 전체 작업 동안 읽기 가능한 상태를 유지해야 합니다.

## 1. 첫 번째 볼륨 열기

어느 파일이 "첫 번째"인지 여부는 WinRAR이 사용한 명명 규칙에 따라 달라집니다:

- 새로운 스타일의 볼륨 (`archive.part1.rar`, `archive.part2.rar`, …): `archive.part1.rar`를 엽니다.
- 이전 스타일의 볼륨 (`archive.rar` 및 `archive.r00`, `archive.r01`, …): `archive.rar`를 엽니다.

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.part1.rar")?;
println!("{:?}", archive.format());
```

라이브러리의 어떤 부분도 열린 파일이 세트의 첫 번째 볼륨인지 확인하지 않습니다. UnRAR SDK는 아카이브 플래그를 통해 해당 사실을 보고하며 크레이트는 이를 참조하지 않으므로, 중간 볼륨을 열어도 아무런 진단 메시지가 출력되지 않습니다 — 단순히 해당 볼륨의 헤더가 설명하는 항목만 표시됩니다. 첫 번째 볼륨을 직접 선택해야 합니다.

## 2. 세트 확인 및 볼륨 둘러보기

```rust
use unified_archive::Archive;

let archive = Archive::open("backup.part1.rar")?;
let (is_multipart, parts) = archive.detect_multipart()?;
if is_multipart {
    println!("{} volumes:", parts.len());
    for part in &parts {
        println!("  {}", part.display());
    }
}
```

`detect_multipart`는 `(bool, Vec<PathBuf>)`를 반환합니다. 벡터는 볼륨 순서대로 정렬되며(번호가 없는 주 볼륨이 먼저 오고, 그 뒤에 번호가 지정된 볼륨이 오름차순으로 정렬됨) 항상 열었던 아카이브를 포함하므로, 단일 볼륨 아카이브는 빈 목록 대신 `(false, [해당 경로 하나])`로 반환됩니다.

사전 검사로 사용할 때 두 가지 속성이 중요합니다. 볼륨은 **파일 이름으로만** 일치시킵니다; `detect_multipart`는 동종(sibling) 파일을 절대 열어보지 않으므로 목록에 표시된 볼륨이 검증된 볼륨임을 보장하지는 않습니다. 또한 디렉터리는 매 호출 시마다 다시 스캔되므로 핸들을 연 후에 나타나거나 사라진 볼륨이 다음 요청 시 반영됩니다.

디렉터리를 읽을 수 없을 때는 임의로 추측하지 않고 호출이 실패합니다: 스캔 중 발생한 I/O 오류는 `ArchiveError::Io`로 전파되며, `Archive::create`에서 생성된 쓰기 모드 핸들에 대해 호출하는 경우 즉시 거부됩니다.

## 3. 새 코드에서는 타입화된 레이아웃 선호하기

```rust
use unified_archive::{Archive, MultipartLayout};

let archive = Archive::open("backup.part1.rar")?;
match archive.multipart_layout()? {
    MultipartLayout::Single { path } => {
        println!("single volume: {}", path.display());
    }
    MultipartLayout::Multi { parts } => {
        println!("{} volumes, first is {}", parts.len(), parts[0].display());
    }
}
```

`MultipartLayout`에는 정확히 두 가지 변형이 있습니다. `Single { path }`는 연 아카이브를 전달합니다. `Multi { parts }`는 감지된 모든 볼륨을 볼륨 순서대로 전달하며 절대 비어 있지 않습니다. 이 열거형의 핵심은 호출자가 불리언 검사를 잊고 단일 볼륨 아카이브를 1개 볼륨 세트로 오인하는 것을 방지하는 것입니다. `detect_multipart`는 0.3 버전에서도 사용 가능하며 비권장(deprecated)되지 않았지만, 0.4 버전에서 정식으로 비권장 처리될 예정입니다.

## 4. 하나의 세트로 그룹화되는 RAR 이름 규칙

접두사만 공유하는 무관한 동종 파일이 세트로 쓸려 들어가지 않도록 그룹화 범위는 의도적으로 좁게 설정되어 있습니다. 일치 확인은 ASCII 대소문자를 구분하지 않으며 반환된 경로는 원래의 대소문자를 유지합니다. 개봉할 볼륨을 선택할 때 두 가지 형태가 중요합니다:

- **새 스타일** — `<base>.part<숫자>.rar`, 연 파일 자체가 동일한 base를 가진 `.part<숫자>.rar` 볼륨일 때만 그룹화됩니다. 접미사는 이름의 *끝*에서부터 파싱되므로 `my.part9.data.part1.rar`는 base가 `my`인 파일이 아니라 `my.part9.data.part2.rar`와 그룹화됩니다; `archive.part.rar`는 숫자 연속이 없어 볼륨이 아닙니다.
- **이전 스타일** — **정확히 두 자리** 숫자를 가진 `<stem>.rar` 및 `<stem>.r<NN>`, `<stem>.s<NN>`, 연 파일이 `.rar`로 끝날 때만 그룹화됩니다. WinRAR은 `.r99` 다음으로 `.s00`을 이어나가며, 정렬 시 전체 s-시리즈를 r-시리즈 뒤에 위치시킵니다. `archive.r1`, `archive.r123`, `archive.rab`는 볼륨이 아닙니다.

동일한 동종 스캔은 ZIP `.zip` + `.zNN` 세트 및 숫자형 `<stem>.<숫자>` 분할도 인식하지만, 이 레시피에는 둘 다 적용되지 않습니다 — 아래의 엄격한 경계 및 각 포맷의 분할 기능이 실제로 보장하는 바에 대한 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)를 참조하세요.

## 5. 압축 해제

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("backup.part1.rar")?;
let result = archive.extract_all(ExtractionOptions {
    destination: PathBuf::from("out"),
    ..Default::default()
})?;
for warning in &result.warnings {
    eprintln!("warning: {warning}");
}
```

이것으로 압축 해제의 모든 과정이 끝납니다. UnRAR은 연 볼륨의 명명 규칙에 따라 `backup.part2.rar` 및 그 이후 파일로 스스로 이어 진행합니다; 2단계의 `parts` 목록은 보고 및 사전 검사용이며 압축 해제의 입력값이 아닙니다. 선택적 호출(`extract_some`, `extract_files`, `extract_by_ids`)도 볼륨 세트에서 동일하게 동작합니다 — [올바른 추출 API 선택 방법](../../../how-to/user/ko/choose-an-extraction-api.md)을 참조하세요.

항목 ID와 목록은 전체 세트를 순회하여 얻으므로, `list_files()`에서 만든 선택 목록이 여러 볼륨에서 데이터를 가져올 수 있다는 점에 유의하세요. 이는 사용자에게 투명하게 처리됩니다.

## 6. 볼륨이 유실된 경우

압축을 해제하기 전에 볼륨 유실 여부를 직접 확인하세요: `parts`는 파일 이름에서 파생되므로 유실된 볼륨은 목록이 짧거나 연속되지 않은 형태로 나타납니다. 이 검사는 비용이 적게 들고 예상했던 볼륨 이름을 명시하는 메시지를 제공합니다.

그럼에도 압축 해제를 진행하는 경우, UnRAR이 다음 볼륨을 이름으로 열려고 시도할 때 오류가 발생합니다. 동일한 UnRAR 코드가 권한 오류 및 인코딩 문제도 포함하므로 `NotFound` 종류의 오류가 아닌 작업명이 `open`인 `ArchiveError::Io`로 표출됩니다. 여기서 명시되는 경로는 유실된 볼륨이 아닌 **열었던 아카이브**이므로, 사용자가 가져와야 할 파일을 알 수 있도록 자체 오류 메시지에 2단계의 볼륨 목록을 포함하세요. 대상에 이미 작성된 항목은 디스크에 남아있으며, 롤백은 없습니다.

압축 해제가 진행 중인 동안 볼륨을 삭제, 이동 또는 마운트 해제하지 마세요. 작업 중간에 이용할 수 없게 된 볼륨에 대한 복구 경로는 없으며, 오류가 깔끔하게 발생한다고 보장할 수도 없습니다.

## 엄격한 경계 사항

- **ZIP 분할 볼륨은 전체 과정에서 지원되지 않습니다.** 명명 휴리스틱이 공유되기 때문에 `detect_multipart`가 `.zip` + `.zNN` 세트를 이름으로 그룹화하며 `ArchiveFormat::Zip`이 이러한 이유로 분할 읽기 기능을 `Support::Partial`로 보고하지만, ZIP 세그먼트 전체에 걸쳐 항목 데이터를 읽는 코드 경로는 존재하지 않습니다. 감지된 ZIP 세트는 정보 참고용으로만 다루세요.
- **7z 숫자 볼륨은 지원되지 않으며 감지조차 되지 않습니다.** `ArchiveFormat::SevenZip`은 분할 기능을 전혀 보고하지 않으므로 `detect_multipart`는 디렉터리를 스캔하기도 전에 반환됩니다: `.7z.001` 볼륨이 열리더라도 이웃에 얼마나 많은 파일이 있든 간에 그 레이아웃은 `Single`로 보고됩니다.
- **분할 생성 기능은 존재하지 않습니다.** `CompressionOptions`에 여전히 `split_size` 필드가 있지만 `Archive::create`는 `CompressionOptions::validate_for_format`을 실행하여 모든 포맷에 대해 `Some(_)` 값을 `ArchiveError::OperationBlocked`로 거부합니다. `None`으로 두세요; 이 필드는 예약된 것이지 기능하지 않습니다. 타입화된 빌더(`ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions`)는 이 필드를 전혀 노출하지 않으며, 이것이 생성 옵션을 구축하는 더 안전한 방법입니다 — [지정된 포맷으로 아카이브 생성하는 방법](../../../how-to/user/ko/create-an-archive.md).
- RAR 생성은 분할 여부와 상관없이 어차피 `Archive::create`의 범위 밖입니다.

## 다음으로 살펴볼 내용

- 분할 열을 포함한 포맷별 기능: [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md).
- `detect_multipart`, `multipart_layout` 및 `MultipartLayout` 시그니처: [공개 API 표면](../../../reference/user/ko/public-api-surface.md).
- `extract_all` 경고의 의미: [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md).
- 단일 파사드가 백엔드마다 다르게 동작하는 이유: [다양한 백엔드를 아우르는 하나의 API](../../../explanation/user/ko/one-api-many-backends.md).
