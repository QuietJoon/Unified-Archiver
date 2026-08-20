---
type: How-To Guide
title: 지정된 포맷으로 아카이브 생성하는 방법
description: 생성 가능한 포맷을 선택하고, 올바른 옵션 값을 작성하며, 항목을 추가하고, 작성기를 완료한 뒤 발생할 수 있는 거부 상황을 확인합니다.
tags: [creation, formats, api, AD-0017, AD-0044]
audience: user
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/create-an-archive.md }
synced_hash: ebb3c5d05f2d2c3df72f8f2d0442175ca0c8e9505a1bc18824982f34cae5af20
language: ko
---
# 지정된 포맷으로 아카이브 생성하는 방법

생성은 항상 동일한 4단계 동작입니다: 포맷 생성이 가능한지 확인하고, 해당 포맷의 압축 옵션 값을 구성하고, 쓰기 모드의 `Archive` 핸들을 통해 항목을 추가한 다음 `finish()`를 호출합니다. 이 페이지에서는 이 순서 중 포맷별 고유 부분과 진행 과정에서 만나는 거부 상황을 다룹니다.

## 시작하기 전에

```rust
use unified_archive::{
    Archive, ArchiveFormat, CompressionLevel, CompressionOptions,
    LibarchiveCompressionOptions, SevenZCompressionOptions, ZipCompressionOptions, Result,
};
```

모든 포맷에 대해 다음 4가지 전제 조건이 유지됩니다:

- 출력 경로는 존재하지 않아야 합니다. 덮어쓰기 플래그는 존재하지 않습니다 (AD 0017).
- 비밀번호 설정 불가. 현재 모든 포맷에 대해 암호화된 생성은 거부됩니다. 2026-07-20 MADR-0027 개정판에서 영구 금지가 아닌 명시적 선택 옵션 뒤로 연기되었지만, 해당 옵션은 출시되지 않았습니다.
- `split_size`는 `None`을 유지해야 합니다. 어떤 백엔드도 분할 볼륨 쓰기를 구현하지 않습니다.
- 스레드당 하나의 핸들. `Archive`는 `Send`이지만 `Sync`는 아닙니다.

`cargo run --example create_archive`는 ZIP, TAR.GZ, 7z 및 비밀번호 거부 동작을 처음부터 끝까지 수행합니다. 소스는 `examples/create_archive.rs`에 있습니다.

## 포맷 생성이 가능한지 확인

포맷 목록에서 추측하지 말고 프레디케이트(predicate)를 사용하여 확인하세요:

```rust
let format = ArchiveFormat::TarZst;
if !format.can_create() {
    // 여기서 할 작업이 없음 — Archive::create가 이 포맷을 거부함.
    return Ok(());
}
```

`ArchiveFormat::can_create`는 `Archive::create`가 참조하는 동일한 프레디케이트이므로, 그 응답과 파사드의 동작이 달라지지 않습니다. 생성 가능한 전체 포맷 집합과 남은 포맷들이 거부되는 이유는 [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)에 나와 있습니다. 실제 적용에서 두 가지 대체 포맷이 나타납니다: `Rar` 및 `Rar5`는 파사드 생성 경로가 전혀 없으며, `Gzip`이나 `Xz`와 같은 단일 파일 압축기의 경우 대응되는 `Tar*` 복합 포맷을 대신 생성합니다.

`can_create`는 고정된 집합이지만, 그중 두 가지 구성 요소는 링크된 libarchive 라이브러리에 의존합니다. `TarZst` 및 `TarLz4`는 해당하는 쓰기 필터와 함께 빌드된 libarchive가 필요합니다. 이것이 없으면 libarchive는 필터를 외부 프로그램 펄백으로 등록하고 경고를 보고합니다. 작성기는 포맷 또는 필터 등록 시 OK가 아닌 반환값을 커맨드라인 압축기로 암묵적 쉘아웃을 수행하는 대신 생성 시점에 엄격한 `ArchiveError::Format`으로 처리합니다. `TarLzma`는 별도의 코덱이 필요하지 않습니다: `TarXz`가 이미 요구하는 동일한 liblzma를 거칩니다. 네이티브 의존성을 설치하는 방법은 [네이티브 빌드 의존성 설치 방법](../../../how-to/operator/ko/install-native-dependencies.md)을 참조하세요.

어느 백엔드가 쓰기를 처리하는지는 오류 메시지를 진단할 때만 의미가 있습니다: ZIP 생성은 Rust `zip` 크레이트를 통해 실행되며, 7z를 포함하여 생성 가능한 다른 모든 포맷은 libarchive를 통해 실행됩니다.

## 하나의 완전한 예시

```rust
use std::path::Path;
use unified_archive::{Archive, ArchiveFormat, CompressionLevel, CompressionOptions, Result};

fn build_bundle(out: &Path, staging: &Path) -> Result<()> {
    let options = CompressionOptions {
        format: ArchiveFormat::TarGzip,
        level: CompressionLevel::Maximum,
        password: None,
        split_size: None,
        progress: None,
    };

    // 선택적 사전 검사: Archive::create와 동일한 게이트, 파일 시스템 접근 없음.
    options.validate_for_format()?;

    // `out`이 이미 존재하면 실패함.
    let mut archive = Archive::create(out, options)?;

    archive.add_file_from_data("MANIFEST", b"bundle 1\n")?;
    archive.add_file_from_path_as(staging.join("notes.md"), "docs/notes.md")?;
    archive.add_directory("empty-slot")?;
    archive.add_directory_recursive(staging.join("bin"))?;

    println!("{} entries written so far", archive.entry_count()?);

    // 핸들을 소비함; flush, fsync 및 오류가 여기서 표출됨.
    archive.finish()
}
```

이 페이지의 나머지 부분은 해당 호출들에 대한 주제별 세부 사항입니다.

## 옵션 구성하기

세 가지 방식으로 `Archive::create`가 소비하는 것과 동일한 `CompressionOptions` 값을 생성할 수 있습니다.

**구조체 리터럴.** 5개 필드가 모두 `pub`이며 0.3 버전에서 `#[non_exhaustive]` 타입이 아니므로, 리터럴이 컴파일되며 번들 예제에서도 사용됩니다:

```rust
let options = CompressionOptions {
    format: ArchiveFormat::SevenZip,
    level: CompressionLevel::Ultra,
    password: None,
    split_size: None,
    progress: None,
};
```

**생성자.** `CompressionOptions::new(format)`은 `Normal` 압축 레벨을 설정하고 password, `split_size`, progress에 `None`을 제공합니다. `CompressionOptions::default()`는 `new(ArchiveFormat::Zip)`입니다.

**포맷별 빌더.** 각 빌더는 고유한 `create_*` 호출과 쌍을 이루며 해당 포맷이 지원하는 필드만 노출하므로 `Archive::create`가 런타임에 거부할 조합은 처음부터 표현할 수 없습니다:

```rust
let mut archive =
    Archive::create_zip("out.zip", ZipCompressionOptions::new().level(CompressionLevel::Fast))?;
archive.add_file_from_data("MANIFEST", b"bundle 1\n")?;
archive.finish()?;
```

다른 두 호출도 두 번째 인자에 고유한 옵션 타입을 취하며 동일한 형태를 가집니다:

```rust
Archive::create_seven_zip("out.7z", SevenZCompressionOptions::new())
Archive::create_libarchive(
    "out.tar.xz",
    LibarchiveCompressionOptions::new(ArchiveFormat::TarXz).level(CompressionLevel::Maximum),
)
```

각각은 여전히 `add_*` 호출과 `finish()`가 필요한 쓰기 모드 핸들을 반환합니다.

- `ZipCompressionOptions` — `level` 및 `progress`. 포맷은 ZIP으로 고정됩니다; `password` 없음, `split_size` 없음.
- `SevenZCompressionOptions` — `level` 및 `progress`. 모든 포맷에 대해 암호화된 생성이 거부되므로 비밀번호 설정자가 존재하지 않습니다; MADR-0027의 2026-07-20 개정판은 아직 출시되지 않은 옵션 선택 뒤로 이를 연기합니다.
- `LibarchiveCompressionOptions::new(format)` — 대상 포맷을 먼저 받고, 추가로 `level` 및 `progress`를 제공합니다. 포맷이 필수이므로 `Default`는 없습니다. `Archive::create_libarchive`는 `ArchiveFormat::Zip`을 명시적으로 거부하며(`create_zip` 또는 `create` 사용), 생성할 수 없는 기타 포맷은 `create` 내부의 공유 `can_create` 게이트에 의해 포착됩니다.

각 빌더는 `From`을 통해 `CompressionOptions`로 변환되며, 각 `create_*` 호출은 `Archive::create`로 라우팅되므로 동작은 구조체 리터럴 방식과 동일합니다. 타입 시스템이 표현할 수 없는 부분(백엔드가 요청된 포맷을 실제로 쓸 수 있는지 여부)은 여전히 생성 시점에 검증됩니다.

`CompressionOptions`는 박싱된 진행률 콜백을 복제할 수 없기 때문에 `Clone`을 구현하지 않습니다. 옵션을 다른 작업으로 전달해야 할 때 `strip_progress()`는 `progress`가 지워진 새로운 값을 반환합니다.

필드별 세부 정보 및 모든 기본값: [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md).

## 압축 레벨 선택하기

`CompressionLevel`의 6가지 변형 중 하나를 전달하세요. 각 백엔드는 이를 자체 스케일에 매핑하며, 변형별 매핑은 [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md)에 나와 있습니다. 매핑 시 주의할 두 가지 동작이 있습니다.

**`TarZst`에서 `Store`는 "압축 없음"이 아닙니다.** zstd 필터는 값을 libzstd에 직접 전달하며, 여기서 `0`은 "압축 없음"이 아닌 "라이브러리 기본값"(레벨 3)을 의미합니다. 따라서 `TarZst`에 대해 `Store`를 요청하면 `1`(가장 약한 실제 압축 레벨)을 전달하므로 요청이 `Fastest`보다 조용히 더 강한 압축이 되는 대신 방향성 솔직함을 유지합니다. 다른 모든 `TarZst` 레시피 레벨은 변경 없이 전달됩니다. 진정으로 압축되지 않은 출력을 원하면 `ArchiveFormat::Tar`를 사용하세요.

**일반 `Tar`는 압축 레벨을 완전히 무시합니다.** 설정할 필터나 포맷 옵션이 없으므로 해당 필드는 허용되지만 사용되지 않습니다.

백엔드가 거부하는 레벨은 암묵적으로 다운그레이드되지 않습니다: "옵션 무시됨" 경고를 포함하여 libarchive의 옵션 설정자로부터 OK가 아닌 반환이 오면 핸들을 해제하고 `compression-level option rejected: …` 메시지와 함께 `ArchiveError::Format`을 반환합니다 (개정된 AD 0013).

## 콘텐츠 추가하기

5가지 호출로 쓰기 모드 핸들에 항목을 추가합니다.

```rust
// 명시적인 아카이브 내부 이름의 메모리 내 바이트.
archive.add_file_from_data("config.json", br#"{"version":"1.0"}"#)?;

// 자체 파일 이름으로 저장되는 파일 시스템 파일.
archive.add_file_from_path("./README.md")?;

// 선택한 이름으로 저장되는 파일 시스템 파일.
archive.add_file_from_path_as("./build/app", "bin/app")?;

// 단일 디렉터리 항목, 내용 없음.
archive.add_directory("logs")?;

// 전체 트리.
archive.add_directory_recursive("./assets")?;
```

- `add_file_from_data`는 버퍼를 단일 항목으로 작성합니다. 전달할 소스 메타데이터가 존재하지 않으므로 아무것도 저장되지 않습니다.
- `add_file_from_path`는 소스 파일 이름에서 아카이브 이름을 파생하고 `add_file_from_path_as`에 위임합니다. 파일 이름이 유효한 UTF-8이 아닌 경우 손실이 발생하는 이름을 대체하는 대신 `InvalidPath`로 실패합니다 (AD 0064) — 대신 `add_file_from_path_as`에 명시적 이름을 전달하세요.
- `add_file_from_path_as`는 소스 파일의 수정 시간과 Unix의 경우 권한 비트를 보존합니다. 파일 크기는 시작 시 한 번 읽어옵니다. 해당 stat 연산과 복사 사이에 파일 크기가 커지거나 줄어들면 헤더와 일치하지 않는 바이트 수를 작성하는 대신 크게 실패합니다.
- `add_directory`는 하나의 디렉터리 항목을 생성합니다. 동일한 정규화된 경로에 대해 두 번 호출하면 두 번째 호출은 중복 기록이 아니라 성공(no-op)으로 처리됩니다.
- `add_directory_recursive`는 링크를 따르지 않고 디렉터리별로 자식을 정렬하여 트리를 순회하므로 동일한 트리는 동일한 아카이브 순서를 만듭니다. 항목은 부모 기준입니다: 소스 디렉터리의 이름이 아카이브로 남아 유지되며(`add_directory_recursive("foo/bar")`는 `bar/...` 항목을 생성), 이는 `tar`, `zip -r`, `7z a -r`과 동일합니다. 비어 있지 않은 디렉터리는 자식에 의해 암시되므로 리프 디렉터리만 고유한 항목을 가져옵니다; 빈 소스 디렉터리는 여전히 루트를 지정하는 단일 항목을 생성합니다. 두 백엔드 모두 동일한 레이아웃을 생성합니다.

**아카이브 내부 이름은 이 경계에서 검증됩니다** (AD 0044). `add_file_from_data`, `add_file_from_path_as`(및 `add_file_from_path`), `add_directory`에 제공하는 이름은 상대적이고 정돈되어 있어야 합니다. `InvalidPath`로 거부되는 항목: 빈 문자열, NUL을 포함하는 모든 항목, `..` 세그먼트, 절대 또는 드라이브 접두사 경로 및 선두 `./` — 즉 `"./file.txt"`는 거부되며 `"file.txt"`가 원하는 형태입니다. 이 검사는 선두 `.`를 제외한 모든 `.`를 정규화하여 제거하는 `std::path::Components`를 순회하므로 `"dir/./file.txt"`는 허용되어 작성된 대로 저장됩니다. 백슬래시는 검사 전 슬래시로 폴딩되므로 `..\..\evil`도 무해해 보이는 하나의 구성 요소로 전달되지 않고 Unix에서도 거부됩니다. 추출 측은 이러한 이름을 암묵적으로 다시 쓸 수 있지만 쓰기 측은 이름이 변경 없이 왕복할 수 있도록 거부합니다. `add_directory_recursive`는 파일 시스템에서 이름을 파생하므로 이 검사가 적용되지 않습니다.

**충돌은 무언가 작성되기 전에 포착됩니다.** 파사드는 약속한 이름을 추적합니다: 동일한 경로를 두 번 추가하거나, 파일과 디렉터리 모두로 하나의 경로를 주장하는 경우(이미 파일로 주장된 경로 아래에 위치할 파일 포함) `OperationBlocked`로 실패합니다. `add_directory_recursive`의 경우 백엔드가 단일 항목을 내보내기 전에 전체 트리를 사전 순회하고 검사하므로 트리 깊은 곳의 충돌도 작성기를 보정 후 재시도할 수 있는 상태로 유지합니다.

**작성기 자체의 출력 파일은 입력이 될 수 없습니다.** 이를 직접 추가하거나 재귀적으로 추가된 트리 내부에서 나타나게 하면 자가 섭취(self-ingestion)로 거부됩니다. 식별 정보는 Unix에서는 `(device, inode)`로, Windows에서는 볼륨 시리얼 번호 및 파일 인덱스로 비교하므로 다르게 표기된 경로나 출력에 대한 하드 링크도 포착됩니다.

## 완료 및 drop 시 대신 일어나는 일

```rust
archive.finish()?;   // 동일한 호출인 archive.close()?도 가능
```

`finish()`는 핸들을 소비하고 백엔드를 비우며(flush) 내구성 경계입니다: ZIP 작성기는 중앙 디렉터리를 작성한 다음 파일을 `sync_data`하고, libarchive 작성기는 네이티브 핸들을 닫고 소유한 기술자를 `sync_data`합니다. 그런 다음 둘 다 최선(best-effort)으로 부모 디렉터리를 동기화합니다. 디스크 용량 부족, 코덱 완료 오류 등 해당 과정의 모든 실패는 이 호출의 `Err`로 표출됩니다.

핸들을 그냥 드롭(drop)하는 것은 펄백일 뿐 동일한 작업이 아닙니다. `Drop`은 동일한 최선 완료 작업을 실행하지만 아무것도 반환할 수 없으므로 실패 시 stderr에 출력되고 유실됩니다:

```text
unified-archive: silent finalize during Drop for `out.tar.gz` failed: …
Call Archive::finish() / Archive::close() to surface this error explicitly.
```

핸들이 오염(poisoned)된 경우 `Drop`은 완료 작업을 전혀 시도하지 않고(백엔드 상태가 정의되지 않음) 출력을 내구성이 있는 것으로 취급해서는 안 된다는 경고를 출력합니다. 아카이브가 중요한 경우 항상 `finish()`를 호출하세요.

오염은 백엔드 쓰기 중 `add_*` 호출이 실패할 때 발생합니다. 그 이후의 모든 추가 `add_*` 및 `finish()`는 아카이브를 다시 생성하라는 `OperationBlocked`를 반환합니다. 복구 경로는 없으며 새 출력 파일을 시작하세요.

핸들이 열려 있는 동안 `archive.entry_count()?`는 작성기가 지금까지 내보낸 항목 수(최종 합계가 아닌 쓰기 진행률)를 보고합니다.

생성에 진행률 콜백을 연결하려면 `CompressionOptions::progress`(또는 빌더의 `.progress(…)`)를 설정하세요; 작성기는 생성 시점에 소유권을 가져옵니다. 해당 콜백에서의 취소는 이미 작성된 내용을 롤백하는 대신 향후 작업을 중단합니다. [진행 상황 보고 및 작업 취소 방법](../../../how-to/user/ko/report-progress-and-cancel.md)을 참조하세요.

## 구성 사전 검사하기

`CompressionOptions::validate_for_format()`은 파일 시스템을 건드리지 않고 다음 순서대로 3가지 구성 게이트를 실행합니다: `can_create`, `password`, `split_size`. `Archive::create`는 작성기를 생성하기 전에 내부적으로 이를 호출하므로 직접 호출하는 것은 선택 사항입니다 — 파일이 생성되기 전에 UI에서 사용자의 선택을 확인하는 등 사전에 구성을 검증하려는 흐름을 위해 존재합니다.

해당 게이트가 먼저 실행되므로 파일 시스템 문제보다 구성 오류가 먼저 보고됩니다: 출력 경로가 이미 존재하는 경우에도 비밀번호를 포함하는 옵션은 `OperationBlocked`로 실패합니다.

## 직면하게 될 거부 사항들

4가지 거부 사항이 실패한 첫 시도의 거의 대부분을 차지합니다:

| 상황 | 결과 |
|---|---|
| 출력 파일이 이미 존재함 | 사전 검사가 아닌 작성기의 배타적 오픈에 의해 원자적으로 감지되는 내부 종류가 `AlreadyExists`인 `ArchiveError::Io`. 직접 파일을 삭제하거나 이름을 변경하세요. 덮어쓰기 플래그는 존재하지 않습니다 (AD 0017) |
| `password`가 `Some`임 | `OperationBlocked` (MADR-0027). 암호화된 아카이브 읽기는 영향받지 않음 |
| 포맷이 생성 가능 집합에 없음 | `OperationBlocked`, `format … is not supported for creation via Archive::create` |
| `tar.zst` 또는 `tar.lz4`용 libarchive 쓰기 필터가 누락됨 | 항목이 작성되기 전 `Archive::create`에서 발생하는 `ArchiveError::Format` |

그 이외에는: 작성기가 표현할 수 없는 소스 경로는 이를 지정하는 `add_*` 호출 시점 또는 재귀 사전 순회 중 거부됩니다 — 심볼릭 링크, 소켓, FIFO 및 디바이스 노드는 `OperationBlocked`로, 일반 파일이 아닌 기타 항목은 `InvalidPath`로 거부됩니다. `ControlFlow::Break`를 반환하는 진행률 콜백은 `ArchiveError::Cancelled { operation: "create" }`로 실행을 종료하며, 쓰기 모드가 아닌 핸들은 경로 검증이나 파일 I/O 전에 `ReadOnlyBackend`로 거부됩니다.

전체 변형 목록, 메시지 및 거부 사유: [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md).

## 다음 단계

- 이미 존재하는 아카이브 수정하기: [기존 아카이브에서 항목을 추가, 교체 또는 제거하는 방법](../../../how-to/user/ko/modify-a-zip-or-7z.md).
- 단일 파사드가 4개의 서로 다른 엔진에 대해 이렇게 동작하는 이유: [다양한 백엔드를 아우르는 하나의 API](../../../explanation/user/ko/one-api-many-backends.md).
- 결과 다시 읽어보기: [올바른 추출 API 선택 방법](../../../how-to/user/ko/choose-an-extraction-api.md).
