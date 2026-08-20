---
type: Tutorial
title: 첫 번째 아카이브 생성 및 수정하기
description: Archive::create로 ZIP을 작성하고, 다시 열어 항목을 확인한 다음, Archive::modify를 통해 항목을 추가 및 제거합니다.
tags: [creation, modification, archive, getting-started]
audience: user
generated:
  by: gemini/gemini-3.6-flash
  at: 2026-08-09T09:59:54Z
sources:
  - { id: en-source, resource: manual/tutorials/user/en/your-first-archive.md }
synced_hash: d6c8b569d17db9ae83ab6b0e34f553012dbef750584b42ed7fe36551777d866a
language: ko
---
# 첫 번째 아카이브 생성 및 수정하기

이 튜토리얼에서는 ZIP 아카이브를 처음부터 생성하고, 다시 열어서 작성된 내용을 확인한 다음, 내용을 직접 수정하고 다시 열어 결과를 확인합니다. 4단계, 1개 프로그램, 단계별 1회 실행으로 진행됩니다.

[unified-archive 시작하기](../../../tutorials/user/ko/getting-started.md)의 완료 상태에서 시작합니다: `demo/` 디렉터리 트리가 유지되어 있고 `unified-archive`에 의존하는 `archive-tour`라는 이름의 Cargo 프로젝트입니다. 아래의 모든 명령은 프로젝트 루트에서 실행합니다.

## 1단계 — ZIP 아카이브 생성

`Archive::create`는 쓰기 모드의 핸들을 반환하며, 항목은 한 번에 하나씩 추가되고 `finish`를 호출하면 중앙 디렉터리가 작성됩니다. `src/main.rs`를 다음 코드로 교체하세요:

```rust
use unified_archive::{Archive, ArchiveError, ArchiveFormat, CompressionLevel, CompressionOptions};

fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    Ok(())
}

fn create_bundle() -> Result<(), ArchiveError> {
    let options = CompressionOptions {
        format: ArchiveFormat::Zip,
        level: CompressionLevel::Normal,
        password: None,
        split_size: None,
        progress: None,
    };

    let mut archive = Archive::create("bundle.zip", options)?;
    archive.add_file_from_data("notes/readme.txt", b"created from memory\n")?;
    archive.add_file_from_path_as("demo/notes/todo.txt", "notes/todo.txt")?;
    println!("staged {} entries", archive.entry_count()?);
    archive.finish()?;
    println!("wrote bundle.zip");

    Ok(())
}
```

실행하세요:

```bash
cargo run
```

```text
staged 2 entries
wrote bundle.zip
```

`entry_count`가 2를 보고하는 이유는 쓰기 모드에서 작성기가 이미 출력한 항목 수를 집계하며, 완료된 아카이브의 항목 수를 집계하는 것이 아니기 때문입니다.

## 2단계 — 다시 열어서 항목 확인

파일에 실제로 무엇이 기록되었는지 확인하는 유일한 방법은 다시 읽어보는 것입니다. 두 번째 함수를 추가하고 호출하세요:

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    Ok(())
}

fn show_bundle(label: &str) -> Result<(), ArchiveError> {
    let archive = Archive::open("bundle.zip")?;
    println!("{label}: format {:?}", archive.format());
    for entry in archive.list_files()? {
        println!("  {} ({} bytes)", entry.path, entry.size.unwrap_or(0));
    }
    Ok(())
}
```

`Archive::create`는 이미 존재하는 파일 위에 덮어쓰는 것을 거부하므로, 다시 실행하기 전에 이전 실행의 출력 파일을 삭제해야 합니다:

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
```

두 항목 모두 파일 시스템 경로가 아닌 지정한 아카이브 내부 경로를 가지고 있습니다.

## 3단계 — 아카이브 직접 수정하기

`Archive::modify`는 기존 아카이브를 수정 모드로 열고, 변경 사항을 메모리에 큐로 쌓은 다음, `commit_changes` 호출 시 한 번에 적용합니다. 세 번째 함수를 추가하고 호출하세요:

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    change_bundle()?;
    Ok(())
}

fn change_bundle() -> Result<(), ArchiveError> {
    let mut archive = Archive::modify("bundle.zip")?;
    archive.add_entry("notes/changelog.txt", b"added by modify\n")?;
    let removed = archive.remove_entry("notes/todo.txt")?;
    println!("queued 1 add and {removed} removal");
    archive.commit_changes()?;
    println!("committed");
    Ok(())
}
```

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
queued 1 add and 1 removal
committed
```

`remove_entry`는 지정한 경로와 일치하는 소스 목록의 항목 수를 반환합니다. 이 예시에서는 `1`이며, 아카이브에 해당 경로가 존재하지 않는 경우 `0`입니다.

## 4단계 — 한 번 더 다시 열기

새로운 코드는 필요하지 않으며, `show_bundle`을 두 번째로 호출하면 됩니다.

```rust
fn main() -> Result<(), ArchiveError> {
    create_bundle()?;
    show_bundle("after create")?;
    change_bundle()?;
    show_bundle("after commit")?;
    Ok(())
}
```

```bash
rm -f bundle.zip
cargo run
```

```text
staged 2 entries
wrote bundle.zip
after create: format Zip
  notes/readme.txt (20 bytes)
  notes/todo.txt (12 bytes)
queued 1 add and 1 removal
committed
after commit: format Zip
  notes/readme.txt (20 bytes)
  notes/changelog.txt (16 bytes)
```

`notes/todo.txt`는 삭제되었고 `notes/changelog.txt`가 추가되었으며, 기존에 남아있던 항목은 새 항목보다 앞선 순서를 유지합니다. 커밋 시 보존되는 항목을 원래 순서대로 먼저 복사한 후 새로 추가된 항목을 뒤에 덧붙이기 때문입니다.

## 기억해야 할 두 가지 규칙

- **매 실행 전 `bundle.zip`을 삭제하세요.** `Archive::create`는 이미 존재하는 파일에 덮어쓰지 않으므로 위의 모든 실행은 `rm -f bundle.zip`으로 시작했습니다. 발생할 수 있는 정확한 오류는 [오류 및 경고](../../../reference/user/ko/errors-and-warnings.md) 문서에서 확인할 수 있습니다.
- **ZIP과 7z만 수정 가능합니다.** `Archive::modify`는 다른 포맷에 대해 아무것도 다시 쓰지 않고 거부합니다. [포맷 지원 매트릭스](../../../reference/user/ko/format-support-matrix.md)에서 각 포맷이 허용하는 작업을 확인할 수 있습니다.

## 다음 단계

- [지정된 포맷으로 아카이브 생성하는 방법](../../../how-to/user/ko/create-an-archive.md) — TAR, 7z 및 압축된 타르볼에 대한 동일한 작업입니다.
- [기존 아카이브에서 항목을 추가, 교체 또는 제거하는 방법](../../../how-to/user/ko/modify-a-zip-or-7z.md) — 교체, 백업 및 수정 기능의 나머지 부분입니다.
- [옵션 및 기본값](../../../reference/user/ko/options-and-defaults.md) — `CompressionOptions`, `ExtractionOptions`, `ModificationOptions`의 모든 필드와 기본값입니다.
- [수정 작업이 아카이브를 다시 쓰는 이유](../../../explanation/developer/ko/modification-is-a-rewrite.md) — 커밋이 보존하는 것, 삭제하는 것, 그리고 이렇게 동작하는 이유를 설명합니다.
