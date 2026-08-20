---
type: Tutorial
title: unified-archive 시작하기
description: 네이티브 필수 조건을 설치한 후 ZIP 아카이브를 열고, 엔트리를 확인하며 추출하는 단일 프로그램을 작성합니다.
tags: [getting-started, archive, extraction, build]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/tutorials/user/en/getting-started.md }
synced_hash: 5530b1663e4a41969b98820ea41a994faf2f89475c4641913ba8f1d6b87a66a2
---
# unified-archive 시작하기

이 튜토리얼을 마치면 실제 ZIP 아카이브를 열어 포맷과 엔트리 목록을 출력하고, 디스크로 추출하는 간단한 Rust 프로그램을 완성하게 됩니다.

이 학습은 Ubuntu 또는 Debian Linux 환경을 기준으로 작성되었습니다. 다른 OS의 경우 네이티브 필수 조건 설치 문서를 참조하십시오.

## 준비 사항

- **Rust 1.85 이상**: Cargo.toml에 에디션 2024 및 버전이 명시되어 있습니다.
- **libarchive 및 pkg-config**: 빌드 시 필수 조건입니다.
- **C++ 컴파일러**: `rar-support` 구동을 위해 필요합니다.
- **zip 도구**: 샘플 아카이브 생성을 위해 필요합니다.

다음 명령으로 설치할 수 있습니다:

```bash
sudo apt-get install libarchive-dev pkg-config g++ zip
```

## 1단계 — 프로젝트 생성 및 의존성 추가

```bash
cargo new archive-tour
cd archive-tour
```

`Cargo.toml`에 추가:

```toml
[dependencies]
unified-archive = "0.4.0"
```

> crates.io 미등록 상태 안내.

## 2단계 — 테스트용 아카이브 생성

시스템 zip 명령을 통해 테스트용 아카이브를 생성합니다:

```bash
mkdir -p demo/notes
printf 'hello from unified-archive\n' > demo/readme.txt
printf 'second file\n' > demo/notes/todo.txt
zip -r sample.zip demo
```

zip 출력 확인:

```text
  adding: demo/ (stored 0%)
  adding: demo/notes/ (stored 0%)
  adding: demo/notes/todo.txt (stored 0%)
  adding: demo/readme.txt (stored 0%)
```

## 3단계 — 아카이브 열기 및 목록 조회

`src/main.rs` 작성:

```rust
use std::env;
use unified_archive::{Archive, ArchiveError};

fn main() -> Result<(), ArchiveError> {
    let path = env::args().nth(1).expect("usage: archive-tour <archive>");

    let archive = Archive::open(&path)?;
    println!("Format:  {:?}", archive.format());
    println!("Entries: {}", archive.entry_count()?);

    for entry in archive.list_files()? {
        let size = match entry.size {
            Some(bytes) => bytes.to_string(),
            None => "-".to_string(),
        };
        println!("{:<24} {:>8}", entry.path, size);
    }

    Ok(())
}
```

실행:

```bash
cargo run -- sample.zip
```

출력 결과:

```text
Format:  Zip
Entries: 4
demo/                           -
demo/notes/                     -
demo/notes/todo.txt            12
demo/readme.txt                27
```

결과 설명.

## 4단계 — 아카이브 추출

`src/main.rs` 수정:

```rust
use std::env;
use std::path::PathBuf;
use unified_archive::{Archive, ArchiveError, ExtractionOptions};

fn main() -> Result<(), ArchiveError> {
    let path = env::args().nth(1).expect("usage: archive-tour <archive>");

    let archive = Archive::open(&path)?;
    println!("Format:  {:?}", archive.format());
    println!("Entries: {}", archive.entry_count()?);

    for entry in archive.list_files()? {
        let size = match entry.size {
            Some(bytes) => bytes.to_string(),
            None => "-".to_string(),
        };
        println!("{:<24} {:>8}", entry.path, size);
    }

    let options = ExtractionOptions {
        destination: PathBuf::from("out"),
        ..Default::default()
    };
    let result = archive.extract_all(options)?;

    println!("Warnings: {}", result.warnings.len());
    for warning in &result.warnings {
        println!("  {warning}");
    }

    Ok(())
}
```

실행:

```bash
cargo run -- sample.zip
```

```text
Format:  Zip
Entries: 4
demo/                           -
demo/notes/                     -
demo/notes/todo.txt            12
demo/readme.txt                27
Warnings: 0
```

추출 결과 설명.

추출된 파일 확인:

```bash
find out -type f | sort
```

```text
out/demo/notes/todo.txt
out/demo/readme.txt
```

축하합니다! 완성되었습니다.

## 다음으로 알아볼 내용

- 관련 튜토리얼 및 문서를 참조하십시오.
