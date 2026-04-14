# AD: Dual ZIP Backend Strategy

Status: Accepted

## Context and Problem Statement
`piz` (mmap-based) provides fast parallel ZIP reading with CRC32 metadata but cannot decrypt. The `zip` crate supports AES and ZipCrypto decryption. A single backend cannot satisfy both the performance requirements for unencrypted archives and the decryption requirements for encrypted ones.

## Decision Drivers
- Best performance for the common unencrypted ZIP case
- Encrypted ZIP support without requiring API changes for consumers
- Seamless backend selection based on archive properties

## Considered Alternatives
- **Replace piz entirely with zip crate** -- rejected because piz's mmap-based reading is substantially faster for large unencrypted ZIP archives.
- **Add decryption to piz upstream** -- blocked by upstream API limitations that make this impractical.

## Decision Outcome
We decided to use `piz` as the default ZIP backend via `Archive::open()` and switch to the `zip` crate via `ZipReader` when `open_encrypted()` is called for ZIP, because it delivers the best performance for the common case while providing seamless encrypted ZIP support.

## Consequences
- Good: Best performance for the common unencrypted case; encrypted ZIP support is seamless to consumers.
- Bad: Two ZIP read backends add maintenance cost and introduce slight behavioral differences that must be tested and documented.
