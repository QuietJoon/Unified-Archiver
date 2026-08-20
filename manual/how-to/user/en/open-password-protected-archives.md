---
type: How-To Guide
title: How to open password-protected archives
description: Detect that an archive needs a password, open it with one, pass it through extraction options, and read the failure when it is wrong.
tags: [security, extraction, formats, api, MADR-0027]
audience: user
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-06T23:41:13Z
sources:
  - { id: archive-facade, resource: src/archive.rs }
  - { id: password-type, resource: src/password.rs }
  - { id: options, resource: src/options.rs }
  - { id: zip-backend, resource: src/ffi/zip_wrapper.rs }
  - { id: sevenz-backend, resource: src/ffi/sevenz_wrapper.rs }
  - { id: madr-0027, resource: docs/records/MADR-0027-reject-encrypted-archive-creation.md }
synced_hash: 6c03775d79e80627ca9e7d1bebfb67acf78116b20584b065d16b8e6c8ce9334f
---

# How to open password-protected archives

This crate reads encrypted archives and never writes them. This page covers
finding out whether a password is needed, handing one to the reader, and the
shape of the failure when the password is wrong.

## Before you start

- Encrypted reads exist for RAR, RAR5, ZIP (both AES and the legacy ZipCrypto
  scheme), and 7z. `ArchiveFormat::supports_encryption_read` is the
  authoritative check: it is true for exactly those variants and false for the
  TAR family, ISO, and the standalone compressed streams, which carry no
  archive-level encryption at all.
- RAR and RAR5 need the `rar-support` feature, which is on by default. Without
  it, `Archive::open_encrypted` on a RAR returns `ArchiveError::Unsupported`.
- Passwords are UTF-8 strings; see "What the Password type gives you".
- `Archive` is `Send` but not `Sync`, so keep one handle per thread.

## Ask whether a password is needed

```rust
use unified_archive::{Archive, ArchiveError};

let archive = Archive::open("download.zip")?;
match archive.is_encrypted() {
    Ok(true) => { /* at least one entry's payload is encrypted */ }
    Ok(false) => { /* no encrypted entry was visible in the metadata */ }
    Err(ArchiveError::Password { .. }) => { /* the listing itself needs the password */ }
    Err(other) => return Err(other),
}
```

`is_encrypted` walks the archive's cached metadata listing and reports whether
any entry carries the encrypted flag. Three properties matter:

- `Ok(false)` means "no encrypted entry was observed in metadata", not
  "extraction will succeed without a password".
- A mixed archive, where some entries are encrypted and some are not, returns
  `Ok(true)`.
- Header-encrypted archives cannot be listed at all without the password, so
  the call returns `Err` rather than `Ok(false)`. A 7z written with `-mhe`
  keeps its whole table of contents encrypted, and the 7z reader's failure to
  open that table classifies as `ArchiveError::Password`. A RAR written with
  `-hp` behaves the same way; there the refusal can arrive from
  `Archive::open` itself, because the UnRAR SDK reports a missing password
  while opening the archive.

Encrypted ZIP is the exception that lists cleanly. ZIP encrypts entry payloads
but leaves the central directory readable, and the ZIP backend reads listings
through the raw, non-decrypting accessor. Names, sizes, timestamps, and stored
CRC values come out of `list_files` with no password at all; the password
matters only when you ask for entry data.

So treat `is_encrypted` as a hint for your interface, not as a gate. The
guaranteed test is to attempt the real work with a candidate password and watch
for `ArchiveError::Password`.

## Open the archive with the password

```rust
let archive = Archive::open_encrypted("secret.7z", "hunter2")?;
let entries = archive.list_files()?;
```

`Archive::open_encrypted(path: impl AsRef<Path>, password: impl AsRef<str>)`
mirrors `Archive::open`'s detection — magic bytes win when present, and a path
with an executable extension routes through the SFX path, where the embedded
payload is staged into a temporary file and then reopened against the
password-capable backend. If you already hold a `Password`, pass
`pw.as_str()`.

Two things it does not do:

- It does not verify the password. Validation is deliberately deferred to the
  first read of entry data (AD 0014): the archive formats themselves defer it,
  `open_encrypted` stays as cheap as `open`, and mixed-encryption archives
  remain usable. Header encryption is the exception, because there the backend
  must decrypt metadata before it can answer anything.
- It does not accept formats that cannot be encrypted. Called on a TAR, ISO,
  or standalone compressed stream it returns `ArchiveError::Unsupported`,
  naming the format and pointing at `Archive::open`. It never falls back to a
  plaintext open, because a silent fallback hides the caller's mistake.

## Or supply the password through extraction options

Every extraction entry point that takes `ExtractionOptions` honours
`options.password`:

```rust
use std::path::PathBuf;
use unified_archive::{Archive, ExtractionOptions};

let archive = Archive::open("secret.zip")?;
let mut options = ExtractionOptions::default().password("hunter2");
options.destination = PathBuf::from("./out");
let result = archive.extract_all(options)?;
```

`ExtractionOptions::password(impl Into<String>)` is a consuming builder over
the public `password: Option<Password>` field. Assigning the field yourself is
equivalent; the builder just wraps the string for you.

When the field is set, the extraction call reopens the archive internally
through `Archive::open_encrypted`, using the path the handle actually reads
from — for an SFX handle that is the staged payload, not the outer executable —
and runs against that fresh handle. Two consequences:

- The reopen happens on every such call. For repeated extractions, open once
  with `open_encrypted` and pass options without a password.
- A password set for a format that cannot be encrypted fails exactly as
  `open_encrypted` does, with `Unsupported`. An earlier version extracted
  plaintext and reported success in that situation; that behaviour was removed.

`extract_to_memory_with_options` and `extract_to_stream_with_options` honour
`password` as well (along with `limits` and `verify_crc32`) and ignore the
fields that describe disk writes or multi-entry traversal.

## What the Password type gives you

`Password`, re-exported at the crate root, is the type stored in
`ExtractionOptions::password` and `CompressionOptions::password`.

- It is constructed only from Rust strings: `Password::new(impl Into<String>)`,
  or the `From<String>`, `From<&str>`, and `From<&String>` impls. The stored
  bytes are therefore valid UTF-8 by construction, which is what every backend
  this crate wraps requires, and `Password::as_str() -> &str` is infallible.
- There is no byte-oriented constructor, so a non-UTF-8 password is
  unrepresentable rather than rejected at run time. The earlier fallible
  accessor that had to guard against it — and its "password bytes are not valid
  UTF-8" error — is gone (AD 0042 and its 2026-07-22 amendment).
- It redacts itself. `Display` prints `***`, `Debug` prints `Password(***)`,
  and `CompressionOptions`'s own `Debug` renders the field as `Some("***")`.
  Logging an options struct does not leak the secret.
- The bytes live in a buffer that is zeroed when the value drops. `Clone` and
  `PartialEq` are available; the underlying secret-string type never appears in
  a public signature or field.

One backend-specific limit: a RAR password crosses a C string boundary, so a
password containing a NUL byte is rejected with `ArchiveError::Password`
("Password contains null byte").

## Read a wrong password's failure

Password failures are `ArchiveError::Password { message }`. Where the error
appears differs by backend, and one case does not use that variant at all:

- **RAR and RAR5.** The UnRAR SDK distinguishes the two conditions, so you get
  `Password` carrying "Wrong password" or "Password required" — from the open,
  the listing, or the extraction, depending on where the SDK notices.
- **ZIP with a password supplied.** A failed AES or ZipCrypto check surfaces as
  `Password` ("Invalid password for ZIP entry N") when that entry is opened for
  reading.
- **ZIP with no password supplied.** Reading an encrypted entry fails as
  `ArchiveError::Format` ("Read entry N: ..."), because the underlying reader
  reports "password required" as an unsupported-archive condition rather than
  as an invalid password. Do not match only on `Password` when deciding whether
  to prompt for one.
- **ZipCrypto specifically.** The password check for the legacy scheme lives in
  the underlying `zip` crate and compares a decrypted header byte, so a wrong
  password can pass it and fail later — usually as `ArchiveError::Corruption`
  from the CRC comparison. Treat corruption on an encrypted ZipCrypto entry as a
  possible wrong password.
- **7z.** A header-encrypted archive fails at the first real operation with
  `Password`. In the common content-only case (`7z a -p`) the archive opens
  with any password, and a wrong one simply decodes to garbage; the backend
  reclassifies the resulting read or CRC failure on an encrypted entry as
  `Password` ("likely wrong password"), so you do not have to interpret
  decoder noise.

The full variant list and message shapes:
[Errors and warnings](../../../reference/user/en/errors-and-warnings.md). Why
one question gets different answers per format:
[One API over many backends](../../../explanation/user/en/one-api-many-backends.md).

## Do not expect to create one

`Archive::create` rejects a password for every format. The gate is
`CompressionOptions::validate_for_format`, which `create` runs first and which
you can call yourself as a preflight; it returns
`ArchiveError::OperationBlocked` citing MADR-0027. Both writer backends repeat
the check before touching the filesystem, so a rejected request leaves no
plaintext file behind, and the typed builders `ZipCompressionOptions`,
`SevenZCompressionOptions`, and `LibarchiveCompressionOptions` expose no
password field at all — through them the request is not even constructible.

The `password` field stays on `CompressionOptions` because MADR-0027 was amended
(2026-07-20) from a permanent rejection to one deferred behind an explicit
opt-in. That opt-in has not shipped, so in 0.4.0 the answer is still a flat
refusal for ZIP, 7z, and every libarchive format.

Two adjacent boundaries. `Archive::modify` refuses an encrypted source outright
with `OperationBlocked` ("password-aware modification not yet supported"), so
you cannot round-trip an encrypted archive through the modification API either.
And the Windows-only `external-rar-create` feature does produce encrypted RAR
archives, but it does so by invoking `rar.exe` with `-hp`, which exposes the
password in the operating system's process listing; it is an escape hatch, not
a confidential channel.

If you need an encrypted archive today, write it with a dedicated tool and use
this crate to read it back.
