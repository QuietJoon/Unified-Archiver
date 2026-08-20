---
type: How-To Guide
title: 비밀번호로 보호된 아카이브를 열고 열람하는 방법
description: 아카이브의 비밀번호 필요 여부를 감지하고, 비밀번호를 전달하여 열거나 추출하며, 오류 발생 시 실패 처리 방식을 안내합니다.
tags: [security, extraction, formats, api, MADR-0027]
audience: user
language: ko
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-19T00:17:06Z
sources:
  - { id: en-source, resource: manual/how-to/user/en/open-password-protected-archives.md }
synced_hash: f1964e8e5b6451e756b3ee4a424eb974b612f679332ee6ab18a35ace2b92d585
---
# 비밀번호로 보호된 아카이브를 여는 방법

이 크레이트는 암호화된 아카이브를 읽기만 하며 작성하지는 않습니다. 이 페이지에서는 비밀번호가 필요한지 확인하는 방법, 리더에 비밀번호를 전달하는 방법, 그리고 비밀번호가 틀렸을 때 발생하는 실패 형태를 다룹니다.

## 시작하기 전에

- 암호화된 읽기는 RAR, RAR5, ZIP (AES 및 레거시 ZipCrypto 방식 모두 지원), 7z에서 지원됩니다. `ArchiveFormat::supports_encryption_read`가 기준이 되는 검사입니다: 해당 변형들에서만 true이고 아카이브 수준의 암호화가 전혀 없는 TAR 패밀리, ISO, 단독 압축 스트림에서는 false입니다.
- RAR 및 RAR5는 기본적으로 활성화되어 있는 `rar-support` 기능이 필요합니다. 이 기능이 없으면 RAR 아카이브에서 `Archive::open_encrypted`를 호출할 때 `ArchiveError::Unsupported`가 반환됩니다.
- 비밀번호는 UTF-8 문자열입니다; "Password 타입이 제공하는 것"을 참조하세요.
- `Archive`는 `Send`이지만 `Sync`는 아니므로 스레드당 하나의 핸들을 사용하세요.

## 비밀번호가 필요한지 확인

```rust
use unified_archive::{Archive, ArchiveError};

let archive = Archive::open("download.zip")?;
match archive.is_encrypted() {
    Ok(true) => { /* 최소한 하나 이상의 엔트리 페이로드가 암호화되어 있음 */ }
    Ok(false) => { /* 메타데이터에서 암호화된 엔트리가 관찰되지 않음 */ }
    Err(ArchiveError::Password { .. }) => { /* 목록 자체를 여는 데 비밀번호가 필요함 */ }
    Err(other) => return Err(other),
}
```

`is_encrypted`는 아카이브의 캐시된 메타데이터 목록을 순회하여 암호화 플래그가 설정된 엔트리가 있는지 보고합니다. 중요한 세 가지 속성:

- `Ok(false)`는 "메타데이터에서 암호화된 엔트리가 관찰되지 않음"을 의미하며, "비밀번호 없이 추출이 성공함"을 의미하지는 않습니다.
- 일부 엔트리는 암호화되어 있고 일부는 암호화되지 않은 혼합 아카이브는 `Ok(true)`를 반환합니다.
- 헤더가 암호화된 아카이브는 비밀번호 없이는 목록을 아예 읽을 수 없으므로 `Ok(false)` 대신 `Err`를 반환합니다. `-mhe` 옵션으로 작성된 7z는 전체 목차를 암호화된 상태로 유지하며, 7z 리더가 해당 목차를 열지 못한 실패는 `ArchiveError::Password`로 분류됩니다. `-hp` 옵션으로 작성된 RAR도 동일하게 동작하며; 이 경우 UnRAR SDK가 아카이브를 열 때 누락된 비밀번호를 보고하므로 `Archive::open` 자체에서 거부가 발생할 수 있습니다.

암호화된 ZIP은 목록이 깨끗하게 조회되는 예외입니다. ZIP은 엔트리 페이로드는 암호화하지만 중앙 디렉토리는 읽을 수 있는 상태로 남겨두며, ZIP 백엔드는 원시 비복호화 접근자를 통해 목록을 읽습니다. 이름, 크기, 타임스탬프 및 저장된 CRC 값은 비밀번호 없이도 `list_files`에서 출력됩니다; 비밀번호는 엔트리 데이터를 요청할 때만 필요합니다.

따라서 `is_encrypted`는 게이트가 아니라 인터페이스를 위한 힌트로 처리하세요. 보장된 확인 방법은 후보 비밀번호로 실제 작업을 시도해보고 `ArchiveError::Password`를 관찰하는 것입니다.

## 비밀번호로 아카이브 열기

```rust
let archive = Archive::open_encrypted("secret.7z", "hunter2")?;
let entries = archive.list_files()?;
```

`Archive::open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>)`는 `Archive::open`의 감지 로직을 그대로 따릅니다 — 매직 바이트가 있는 경우 매직 바이트가 우선하며, 실행 파일 확장자를 가진 경로는 SFX 경로를 타서 내장된 페이로드가 임시 파일에 스테이징된 후 비밀번호 지원 백엔드에 대해 다시 열립니다. 이미 `Password`를 가지고 있다면 `pw.as_str()`을 전달하세요.

이 호출이 수행하지 않는 두 가지:

- 비밀번호를 검증하지 않습니다. 검증은 엔트리 데이터의 첫 읽기 시점으로 의도적으로 연기됩니다 (AD 0014): 아카이브 포맷 자체가 검증을 연기하고, `open_encrypted`가 `open`만큼 저렴하게 유지되며, 혼합 암호화 아카이브도 계속 사용할 수 있기 때문입니다. 헤더 암호화는 예외로, 이 경우 백엔드가 무언가 답하기 전에 메타데이터를 먼저 복호화해야 합니다.
- 암호화할 수 없는 포맷은 수용하지 않습니다. TAR, ISO 또는 단독 압축 스트림에서 호출되면 포맷 이름을 명시하고 `Archive::open`을 가리키는 `ArchiveError::Unsupported`를 반환합니다. 침묵 속의 폴백은 호출자의 실수를 숨기므로 일반 오픈으로 폴백하지 않습니다.

## 또는 추출 옵션을 통해 비밀번호 제공

`ExtractionOptions`를 받는 모든 추출 엔트리 포인트는 `options.password`를 반영합니다:

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("secret.zip")?;
let mut options = ExtractionOptions::default().password("hunter2");
options.destination = PathBuf::from("./out");
let result = archive.extract_all(options)?;
```

`ExtractionOptions::password(impl Into<String>)`는 공개 `password: Option<Password>` 필드에 대한 소비형 빌더입니다. 필드를 직접 할당하는 것도 동등하며, 빌더는 문자열을 포장해 주는 역할만 합니다.

필드가 설정되면 추출 호출은 핸들이 실제로 읽는 경로(SFX 핸들의 경우 외부 실행 파일이 아닌 스테이징된 페이로드)를 사용하여 내부적으로 `Archive::open_encrypted`를 통해 아카이브를 다시 열고 해당 새 핸들에 대해 실행합니다. 이에 따른 두 가지 결과:

- 이러한 재열기(reopen)는 매 호출마다 발생합니다. 반복적인 추출의 경우 `open_encrypted`로 한 번 열고 비밀번호 없는 옵션을 전달하세요.
- 암호화할 수 없는 포맷에 비밀번호를 설정하면 `open_encrypted`와 동일하게 `Unsupported`로 실패합니다. 이전 버전에서는 그러한 상황에서 일반 텍스트를 추출하고 성공을 보고했으나 해당 동작은 제거되었습니다.

`extract_to_memory_with_options` 및 `extract_to_stream_with_options`도 (`limits` 및 `verify_crc32`와 함께) `password`를 준수하며 디스크 쓰기나 다중 엔트리 순회를 설명하는 필드는 무시합니다.

## Password 타입이 제공하는 것

크레이트 루트에서 다시 내보내지는 `Password`는 `ExtractionOptions::password` 및 `CompressionOptions::password`에 저장되는 타입입니다.

- Rust 문자열로부터만 생성됩니다: `Password::new(impl Into<String>)`, 또는 `From<String>`, `From<&str>`, `From<&String>` 구현. 따라서 저장된 바이트는 이 크레이트가 감싸는 모든 백엔드가 요구하는 대로 올바른 UTF-8임이 보장되며, `Password::as_str() -> &str`은 에러를 발생시키지 않습니다(infallible).
- 바이트 기반 생성자가 없으므로 UTF-8이 아닌 비밀번호는 런타임에 거부되는 대신 표현 자체가 불가능합니다. 이를 가드해야 했던 이전의 가변 접근자 및 그 "password bytes are not valid UTF-8" 에러는 제거되었습니다 (AD 0042 및 2026-07-22 개정안).
- 비밀번호를 재덕팅(redact)합니다. `Display`는 `***`를 출력하고, `Debug`는 `Password(***)`를 출력하며, `CompressionOptions` 자체의 `Debug`는 해당 필드를 `Some("***")`로 렌더링합니다. 옵션 구조체를 로깅해도 비밀이 유출되지 않습니다.
- 바이트는 값이 drop될 때 0으로 지워지는 버퍼에 거주합니다. `Clone` 및 `PartialEq`를 사용할 수 있으며, 바탕이 되는 비밀 문자열 타입은 퍼블릭 시그니처나 필드에 절대 나타나지 않습니다.

백엔드 전용 제한 한 가지: RAR 비밀번호는 C 문자열 경계를 넘어가므로 NUL 바이트가 포함된 비밀번호는 `ArchiveError::Password` ("Password contains null byte")로 거부됩니다.

## 잘못된 비밀번호의 실패 읽기

비밀번호 실패는 `ArchiveError::Password { message }` 형태입니다. 에러가 나타나는 위치는 백엔드마다 다르며 한 가지 케이스는 이 변형을 전혀 사용하지 않습니다:

- **RAR 및 RAR5.** UnRAR SDK는 두 조건을 구별하므로, SDK가 감지한 위치(오픈, 목록 조회 또는 추출 시점)에 따라 "Wrong password" 또는 "Password required"가 담긴 `Password`를 받게 됩니다.
- **비밀번호가 제공된 ZIP.** 실패한 AES 또는 ZipCrypto 검사는 해당 엔트리를 읽기 위해 열 때 `Password` ("Invalid password for ZIP entry N")로 표출됩니다.
- **비밀번호가 제공되지 않은 ZIP.** 암호화된 엔트리를 읽으면 `ArchiveError::Format` ("Read entry N: ...")으로 실패하는데, 이는 하위 리더가 "password required"를 잘못된 비밀번호가 아닌 지원되지 않는 아카이브 조건으로 보고하기 때문입니다. 비밀번호 입력을 요청할지 결정할 때 `Password`만 매칭하지 마세요.
- **특히 ZipCrypto.** 레거시 방식에 대한 비밀번호 검사는 하위 `zip` 크레이트에 위치하며 복호화된 헤더 바이트를 비교하므로, 틀린 비밀번호가 통과된 후 나중에(보통 CRC 비교 시점의 `ArchiveError::Corruption`으로) 실패할 수 있습니다. 암호화된 ZipCrypto 엔트리의 부패(corruption)는 틀린 비밀번호의 가능성이 있는 것으로 처리하세요.
- **7z.** 헤더가 암호화된 아카이브는 첫 번째 실제 작업에서 `Password`로 실패합니다. 일반적인 콘텐츠 전용 케이스(`7z a -p`)에서 아카이브는 임의의 비밀번호로 열리며, 틀린 비밀번호는 쓰레기 데이터로 디코딩될 뿐입니다; 백엔드는 암호화된 엔트리에서 발생하는 결과 읽기 또는 CRC 실패를 `Password` ("likely wrong password")로 재분류하므로 디코더 노이즈를 직접 해석할 필요가 없습니다.

전체 변형 목록과 메시지 형태: [에러 및 경고](../../../reference/user/ko/errors-and-warnings.md). 한 가지 질문에 포맷마다 다른 답변이 나오는 이유: [여러 백엔드를 아우르는 단일 API](../../../explanation/user/ko/one-api-many-backends.md).

## 비밀번호 아카이브 생성을 기대하지 말 것

`Archive::create`는 모든 포맷에 대해 비밀번호를 거부합니다. 게이트는 `CompressionOptions::validate_for_format`이며, `create`가 이를 가장 먼저 실행하고 사전에 직접 호출할 수도 있습니다; 이 함수는 MADR-0027을 인용하며 `ArchiveError::OperationBlocked`를 반환합니다. 두 라이터 백엔드 모두 파일 시스템에 손을 대기 전에 검사를 반복하므로, 거부된 요청은 일반 텍스트 파일을 남기지 않으며, 타입화된 빌더 `ZipCompressionOptions`, `SevenZCompressionOptions`, `LibarchiveCompressionOptions`는 비밀번호 필드를 전혀 노출하지 않으므로 이들을 통해서는 요청을 구성조차 할 수 없습니다.

구현 유예 상태 안내.

두 가지 인접한 경계 사항. `Archive::modify`는 암호화된 소스를 "password-aware modification not yet supported"라는 `OperationBlocked`와 함께 단칼에 거부하므로, 수정 API를 통해 암호화된 아카이브를 수정 왕복 작업할 수 없습니다. 그리고 Windows 전용 `external-rar-create` 기능은 암호화된 RAR 아카이브를 생성하지만, 이는 운영 체제의 프로세스 목록에 비밀번호를 노출하는 `-hp` 옵션으로 `rar.exe`를 호출하여 수행됩니다; 이는 비상 탈출구일 뿐 비밀 채널이 아닙니다.

오늘날 암호화된 아카이브가 필요한 경우 전용 도구로 작성하고 이 크레이트를 사용해 다시 읽으세요.
