---
type: How-To Guide
title: 진행 상황 보고 및 작업 취소 방법
description: ProgressCallback을 추출, 생성, SFX 스테이징에 연결하고, 속도를 조절하며, 취소 시 디스크에 남는 항목을 이해합니다.
tags: [api, extraction, creation, sfx, AD-0021]
audience: user
language: ko
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-12T11:20:31Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/report-progress-and-cancel.md }
synced_hash: 270ec3b28f219323a6a82d27828a9440d3c307cdf656c846d1946a05e99ab9ab
---
# 진행 상황 보고 및 작업 취소 방법

진행 상황 보고와 취소는 동일한 메커니즘입니다: `Break`가 중단을 의미하는 `ControlFlow`를 반환하는 하나의 콜백입니다. 이 페이지에서는 이를 추출, 생성 및 SFX 스테이징에 연결하고 취소된 작업이 남기는 항목을 설명합니다.

## 콜백 작성

대부분의 보고에는 클로저로 충분합니다: `FnMut(u64, Option<u64>) -> ControlFlow<()> + Send`를 만족하는 모든 것은 이미 `ProgressCallback`입니다.

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;

let cb: Box<dyn ProgressCallback> = Box::new(|processed: u64, total: Option<u64>| {
    eprintln!("{processed} / {total:?}");
    ControlFlow::Continue(())
});
```

바인딩에 타입을 명시하세요. `ControlFlow`는 continue 타입뿐만 아니라 break 타입도 함께 전달하며, `Continue(())`만으로는 타입을 고정하지 못하므로 명시적인 타입이 없는 `let cb = Box::new(…)`는 `type annotations needed` 오류와 함께 컴파일에 실패합니다. 다음 섹션에서 설명하는 것처럼 `options.progress`에 직접 할당하면 동일한 정보가 제공되므로 별도의 타입 명시가 필요하지 않습니다.

콜백에 자체 상태가 필요한 경우 트레이트를 수동으로 구현하세요:

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;

struct Bar {
    last_percent: u64,
}

impl ProgressCallback for Bar {
    fn on_progress(&mut self, processed: u64, total: Option<u64>) -> ControlFlow<()> {
        if let Some(total) = total.filter(|t| *t > 0) {
            let percent = processed * 100 / total;
            if percent != self.last_percent {
                eprintln!("{percent}%");
                self.last_percent = percent;
            }
        } else {
            eprintln!("{processed} bytes");
        }
        ControlFlow::Continue(())
    }
}
```

어느 쪽이든 두 옵션 구조체 모두 `Option<Box<dyn ProgressCallback>>`를 저장하므로 값은 박싱됩니다. 콜백은 작업당 단일 스레드에서 `&mut self`를 통해 호출되므로 `Cell`이나 `RefCell` 같은 캡처는 괜찮으며 뮤텍스가 필요하지 않습니다. 트레이트 선언 및 해당 바운드: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).

한 가지 엄격한 규칙: 콜백 내부에서 아카이브 작업을 수행하지 마세요. 백엔드는 백엔드 내부 상태를 보유한 채 동기식으로 이를 호출하며, RAR 작업 중에는 프로세스 범위의 UnRAR 락이 유지됩니다. 해당 스레드에서 다시 진입하면 프로세스가 교착 상태에 빠지는 대신 이를 감지하고 `ArchiveError::OperationBlocked` ("re-entrant UnRAR access detected")를 반환합니다. 필요한 정보를 기록하고 호출이 반환된 후 조치를 취하세요.

## 추출에 연결

```rust
use std::ops::ControlFlow;
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("release.tar.zst")?;
let mut options = ExtractionOptions {
    destination: PathBuf::from("./out"),
    ..Default::default()
};
options.progress = Some(Box::new(|processed: u64, total: Option<u64>| {
    if let Some(total) = total {
        eprintln!("{processed}/{total}");
    }
    ControlFlow::Continue(())
}));
let result = archive.extract_all(options)?;
```

필드를 할당하세요: `ExtractionOptions`에는 `password`용 빌더만 있습니다.

추출 시 `total`은 `Some(분모)`입니다: 선택한 항목들의 선언된 압축 해제 크기 합계로, 콜백이 존재하는 경우에만 계산됩니다. ZIP 및 7z 백엔드는 어차피 추출을 건너뛰는 항목(디렉토리 및 링크, ZIP의 특수 Unix 모드)을 제외합니다; libarchive 및 UnRAR 백엔드는 크기를 선언한 선택된 모든 항목을 합산합니다. 포맷이 크기를 선언하지 않은 항목은 분모나 분자 모두에 나타나지 않으므로 바를 이동시키지 않고 내부 바이트 예산을 진행시키며, `processed`는 단조 증가를 유지하고 전체 총합을 초과하지 않습니다. 각 백엔드는 순회 후 최종 호출(완료된 총합, 또는 libarchive의 경우 분모로 제한된 실제 처리 수)을 수행하며, 여기서 반환된 `Break` 역시 준수됩니다.

어떤 호출이 이 필드를 준수하는지는 균일하지 않습니다:

- 준수됨: `extract_all`, `extract_some` / `extract_filtered`, `extract_files`, `extract_by_ids`.
- 무시됨: `extract_file`, `extract_to_memory` 및 `extract_to_memory_with_options`, `extract_to_stream` 및 `extract_to_stream_with_options`. 단일 항목 및 메모리 내 경로는 진행 계획 없이 작업을 백엔드로 직접 전달합니다; 스트림의 경우 `StreamingExtractor::bytes_read`에서 보고를 구동하세요.

호출 주기(cadence)는 항목 경계당 한 번에 더해 항목 내 64 KiB 청크당 한 번입니다. libarchive 및 UnRAR 경로는 아래에 설명된 동일한 속도 제한기(rate limiter)를 사용하여 초당 약 60회 호출로 자체 제한합니다; ZIP 및 7z 경로는 속도를 제한하지 않으므로 수 십기가바이트 항목의 경우 청크당 한 번씩 호출됩니다.

## 취소 및 남는 항목 파악

`ControlFlow::Break(())`를 반환하세요. 작업이 중지되고 호출은 모든 취소 표면에 대해 하나의 타입 지정 변형인 `ArchiveError::Cancelled { operation }`을 반환하므로 메시지 텍스트를 조사하는 대신 변형을 일치시킬 수 있습니다.

```rust
use unified_archive::ArchiveError;

match archive.extract_all(options) {
    Ok(result) => { /* result.warnings */ }
    Err(ArchiveError::Cancelled { operation }) => {
        eprintln!("cancelled during {operation}");
    }
    Err(other) => return Err(other),
}
```

모든 디스크 추출 진입점에 대해 라벨은 `"extract_all"`이며, `extract_files`, `extract_by_ids` 및 `extract_some`에서 발생한 취소를 포함합니다 — 백엔드는 하나의 공유 라벨을 전달합니다. 생성은 `"create"`를 보고하고 SFX 스테이징은 `"sfx_staging"`을 보고합니다.

취소는 "향후 작업 중단"을 의미하며, "완료된 작업 취소"를 의미하지 않습니다:

- **추출.** 각 일반 파일은 대상 옆의 스테이징 파일에 작성되며 완료된 후에만 대상 위치로 이름이 변경되므로, 진행 중이던 항목은 대상 경로에 아무것도 남기지 않습니다. 이미 이름이 변경된 항목은 유지되며, 그 과정에서 생성된 대상 디렉토리 및 상위 디렉토리도 유지됩니다. 롤백은 없습니다.
- **생성.** 콜백은 바이트가 라이터에 도달한 후 실행되므로 `Break`가 이를 취소할 수 없습니다. 오류는 `add_*` 호출에서 노출되고, 진행 중인 항목은 부분 상태로 남아 라이터의 항목 수에 포함되지 않으며, 라이터는 스스로를 오염시킵니다: 이후의 `add_*` 호출은 오염 이름을 명시한 `OperationBlocked`를 반환합니다. `finish`(또는 `Drop`)는 `Break` 전에 완료된 모든 항목을 포함하여 구조적으로 유효한 아카이브로 라이터를 비웁니다. 전무(all-or-nothing) 방식이 필요한 경우 임시 경로에 생성하고 성공 시 직접 이름을 변경하세요.
- **SFX 스테이징.** 부분 임시 파일은 자동으로 제거됩니다.

## 생성에 연결

```rust
use std::ops::ControlFlow;
use unified_archive::{Archive, ArchiveFormat, CompressionOptions};

let mut options = CompressionOptions::new(ArchiveFormat::TarGzip);
options.progress = Some(Box::new(|written: u64, _total: Option<u64>| {
    eprintln!("{written} bytes written");
    ControlFlow::Continue(())
}));

let mut archive = Archive::create("out.tar.gz", options)?;
archive.add_directory_recursive("./payload")?;
archive.finish()?;
```

`CompressionOptions`에는 `progress` 세터가 없으므로 필드를 직접 할당하세요; 타입 지정된 포맷별 빌더 `ZipCompressionOptions`, `SevenZCompressionOptions` 및 `LibarchiveCompressionOptions`에는 `progress(Box<dyn ProgressCallback>)`가 있습니다. `Archive::create`는 옵션을 값으로 받아 콜백을 선택한 라이터로 이동시킵니다.

생성은 의도적으로 추출과 다르게 보고됩니다:

- `total`은 항상 `None`입니다. 생성 스트림은 사전에 크기가 지정되지 않습니다: 라이터는 진행하면서 슬라이스에서 읽거나 디렉토리를 순회하므로 보고할 분모가 없습니다 (AD 0021). 백분율에는 직접 추정한 총합이 필요합니다.
- `processed`는 라이터에 전달된 페이로드 바이트의 누적 카운트로, 전체 아카이브에 대해 포화 연산(saturating)으로 누적됩니다.
- 호출 주기는 AD 0021이 결정한 의미에서 바이트당이 아닌 항목당입니다: 빈 항목에 대한 0바이트 호출을 포함하여 `add_*` 호출당 최소 한 번의 호출을 받습니다. 그러나 큰 항목은 항목 내부에서 보고됩니다 — ZIP 라이터는 64 KiB 단위로 모든 페이로드를 나누고 청크당 다시 호출하며, libarchive 라이터는 경로에서 스트리밍된 항목에 대해 동일하게 작동하는 반면, 바이트 슬라이스에서 추가된 항목은 전체 길이로 한 번 알립니다. AD 0021의 텍스트는 원래 도입되었을 때의 엄격한 항목당 한 번 형태를 여전히 설명합니다.

재작성은 동일한 채널을 통해 보고됩니다: `ModificationOptions::compression`은 `CompressionOptions`를 전달하고, `commit_changes`는 콜백을 포함하여 아카이브를 재구성하는 라이터로 이를 이동시킵니다.

## RateLimiter로 속도 조절

여러 백엔드가 청크당 여러분을 호출하므로 콜백 본문을 가볍게 유지하거나 게이트를 적용하세요. `RateLimiter`는 크레이트 자체의 속도 제한기이며, `unified_archive::options::RateLimiter`로 접근할 수 있습니다 (크레이트 루트 재노출에는 포함되지 않음); 기본 간격은 16 ms로, 초당 약 60회 업데이트입니다:

```rust
use std::ops::ControlFlow;
use unified_archive::ProgressCallback;
use unified_archive::options::RateLimiter;

let mut limiter = RateLimiter::new();
let cb: Box<dyn ProgressCallback> = Box::new(move |processed: u64, total: Option<u64>| {
    if limiter.should_update() {
        eprintln!("{processed} / {total:?}");
    }
    ControlFlow::Continue(())
});
```

이것은 콜백 자체가 아닌 보고 속도를 조절합니다: 백엔드는 여전히 청크당 여러분을 호출하며, 반환된 `Break`는 여전히 즉시 취소됩니다. 모든 호출에서 취소를 확인하려는 경우 취소 결정을 `should_update` 분기 외부에 유지하세요. 다른 생성자, 첫 번째 호출 규칙 및 전체 메서드 목록: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).

## SFX 페이로드 스테이징 관찰

자체 추출 아카이브를 열면 일반 백엔드가 읽기 전에 임베디드 페이로드가 임시 파일로 복사되며, 이 복사는 수 기가바이트 설치 프로그램에 대해 눈에 띄는 작업입니다. 이것은 `ExtractionOptions`가 존재하기 전에 발생하므로 자체 후크가 있습니다:

```rust
use unified_archive::{Archive, SfxStagingProgress};

let progress = SfxStagingProgress::new(|copied| eprintln!("staged {copied} bytes"));
let archive = Archive::open_with_sfx_progress("installer.exe", Some(progress))?;
```

`SfxStagingProgress::new(impl FnMut(u64) + Send + 'static)`은 관찰만 합니다. 취소 가능한 변형은 `SfxStagingProgress::with_cancel(impl FnMut(u64) -> bool + Send + 'static)`이며, `false`를 반환하면 복사가 중단되고 `Archive::open_with_sfx_progress`는 `ArchiveError::Cancelled { operation: "sfx_staging" }`을 반환합니다. 어느 쪽이든 클로저는 누적 바이트 수를 수신하며 내부 속도 제한 없이 64 KiB 청크당 한 번씩 호출됩니다. 후크로 `None`을 전달하는 것은 `Archive::open_sfx`와 정확히 동일합니다. 해당 경로에 대한 자세한 정보: [자체 추출 아카이브를 감지하고 여는 방법](handle-self-extracting-archives.md).

## 재사용할 옵션 (콜백 제외)

`CompressionOptions`는 `progress` 필드가 트레이트 오브젝트이므로 `Clone`을 구현하지 않습니다. 콜백 없이 동일한 구성을 다른 작업에 전달하려면 스트립된 복사본을 구하세요:

```rust
let plain = options.strip_progress();
```

원본은 결과를 다시 할당하지 않는 한 콜백을 유지하므로, 의도 — 이 작업은 진행 상황 이벤트를 받지 않음 — 가 호출 시점에 명시됩니다.

`strip_progress`가 복사하는 내용, `Clone`이 제거된 이유, 그리고 모든 옵션 필드의 타입 및 기본값: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).
