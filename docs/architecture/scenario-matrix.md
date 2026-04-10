# Scenario Matrix

Scenario-to-system mapping for all mandatory MVP scenarios.

## Inspection Scenarios

| ID | Scenario | Actor | Trigger/Input | Expected Output | Modules Touched | Persistence | Files Touched | Integrations | Core Verification? | Critical Negative Path? |
|---|---|---|---|---|---|---|---|---|---|---|
| SCN-INS-01 | Inspect archive contents (unified API) | Rust developer | Archive file (any format) | Vec<ArchiveEntry> with path, size, compressed_size, modified, crc32 | archive, inspection, format, entry, ffi/* | None | Input archive (read) | UnRAR or libarchive or piz or sevenz | Yes | No |
| SCN-INS-02 | Inspect password-protected metadata | Rust developer | Encrypted archive, no password | Filenames + metadata accessible; content protected | archive, inspection, format, entry, ffi/wrapper | None | Input archive (read) | UnRAR (RAR), zip crate (ZIP) | Yes | Yes (ArchiveError::Password if content accessed) |
| SCN-INS-03 | Validate archive integrity via CRC32 | Rust developer | Archive file | Corruption detected per-file or Ok | archive, inspection, security, ffi/* | None | Input archive (read) | UnRAR or libarchive (auto-CRC during extract) | Yes | Yes (ArchiveError::Corruption) |
| SCN-INS-04 | Inspect multi-part archive | Rust developer | First part of split archive (.part1.rar, .z01) | Complete file list from all parts | archive, inspection, ffi/wrapper | None | All parts (read) | UnRAR (auto-handles parts) | No | No |
| SCN-INS-05 | Filter large archive listings | Rust developer | Archive with 1000+ files + filter predicate | Filtered entries with metadata | archive, inspection, entry | None | Input archive (read) | Backend (any) | No | No |

## Extraction Scenarios

| ID | Scenario | Actor | Trigger/Input | Expected Output | Modules Touched | Persistence | Files Touched | Integrations | Core Verification? | Critical Negative Path? |
|---|---|---|---|---|---|---|---|---|---|---|
| SCN-EXT-01 | Extract all files (unified API) | Rust developer | Archive + ExtractionOptions (destination) | All files extracted with correct structure/content | archive, extraction, security, options, ffi/* | None | Input archive (read), extracted files (write) | Backend (any) | Yes | No |
| SCN-EXT-02 | Extract password-protected archive | Rust developer | Encrypted archive + correct password | Decrypted files extracted | archive, extraction, security, options, ffi/wrapper, ffi/zip_wrapper | None | Input archive (read), extracted files (write) | UnRAR (RAR), zip crate (ZIP) | Yes | Yes (ArchiveError::Password on wrong password) |
| SCN-EXT-03 | Extract multi-layer compressed | Rust developer | TAR.GZ / TAR.BZ2 / TAR.XZ archive | Transparent multi-layer decompression | archive, extraction, format, ffi/libarchive_wrapper | None | Input archive (read), extracted files (write) | libarchive | Yes | No |
| SCN-EXT-04 | Handle corrupted archive | Rust developer | Invalid/corrupted archive | ArchiveError::Corruption with details | archive, extraction, ffi/*, error | None | Input archive (read) | Backend (any) | Yes | Yes (must produce clear error) |
| SCN-EXT-05 | Monitor extraction progress | Rust developer | Large archive + ProgressCallback | Callbacks at 10+ updates/sec | archive, extraction, options (RateLimiter) | None | Input archive (read), extracted files (write) | Backend (any) | No | No |

## Creation Scenarios

| ID | Scenario | Actor | Trigger/Input | Expected Output | Modules Touched | Persistence | Files Touched | Integrations | Core Verification? | Critical Negative Path? |
|---|---|---|---|---|---|---|---|---|---|---|
| SCN-CRE-01 | Create archive in multiple formats | Rust developer | Files + CompressionOptions (format) | Valid archive readable by standard tools | archive, creation, format, options, ffi/libarchive_wrapper, ffi/zip_writer | None | Source files (read), new archive (write) | libarchive (7z, TAR), zip crate (ZIP) | Yes | No |
| SCN-CRE-02 | Create with max compression | Rust developer | Files + Maximum compression level | Optimal compression per format | archive, creation, options, ffi/* | None | Source files (read), new archive (write) | libarchive or zip crate | No | No |
| SCN-CRE-03 | Create password-protected archive | Rust developer | Files + password | Encrypted archive | archive, creation, options, ffi/zip_writer | None | Source files (read), encrypted archive (write) | zip crate (ZIP encryption) | No | No |
| SCN-CRE-04 | Create from large dataset with progress | Rust developer | 10GB+ data + ProgressCallback | Bounded memory, progress callbacks | archive, creation, options | None | Source files (read), new archive (write) | Backend (any) | No | No |
| SCN-CRE-05 | Create with compression level control | Rust developer | Pre-compressed files + level adjustment | Adjustable compression overhead | archive, creation, options | None | Source files (read), new archive (write) | Backend (any) | No | No |

## Modification Scenarios

| ID | Scenario | Actor | Trigger/Input | Expected Output | Modules Touched | Persistence | Files Touched | Integrations | Core Verification? | Critical Negative Path? |
|---|---|---|---|---|---|---|---|---|---|---|
| SCN-MOD-01 | Add files to existing archive | Rust developer | Archive + new files | Archive contains original + new entries | archive, modification, creation, ffi/libarchive_wrapper | None | Source archive (read), temp archive (write), atomic rename | libarchive (read), creation backends (write) | Yes | No |
| SCN-MOD-02 | Remove entries from archive | Rust developer | Archive + entry names to remove | Entries removed, archive size reduced | archive, modification, creation | None | Source archive (read), temp archive (write), atomic rename | libarchive (read), creation backends (write) | Yes | No |
| SCN-MOD-03 | Replace file in archive | Rust developer | Archive + updated file | Only replaced entry re-compressed | archive, modification, creation | None | Source archive (read), temp archive (write), atomic rename | libarchive (read), creation backends (write) | No | No |
| SCN-MOD-04 | Modify large archive efficiently | Rust developer | Large archive + modification | Minimal temp disk usage | archive, modification, creation | None | Source archive (read), temp archive (write) | libarchive, creation backends | No | No |

## SFX Detection Scenarios

| ID | Scenario | Actor | Trigger/Input | Expected Output | Modules Touched | Persistence | Files Touched | Integrations | Core Verification? | Critical Negative Path? |
|---|---|---|---|---|---|---|---|---|---|---|
| SCN-SFX-01 | Detect Windows PE SFX (ZIP) | Rust developer | Windows .exe with embedded ZIP | is_sfx=true, format=ZIP, offset, stub_type=PE | sfx/detection, sfx/signatures, sfx/stub_types, format | None | Executable file (read, first 1MB) | goblin (PE parsing) | Yes | No |
| SCN-SFX-02 | Detect WinRAR SFX | Rust developer | WinRAR .exe | is_sfx=true, format=RAR/RAR5, offset, stub_type=PE | sfx/detection, sfx/signatures, sfx/stub_types | None | Executable file (read, first 1MB) | goblin | Yes | No |
| SCN-SFX-03 | Detect 7-Zip SFX | Rust developer | 7-Zip .exe | is_sfx=true, format=7z, offset, stub_type=PE | sfx/detection, sfx/signatures, sfx/stub_types | None | Executable file (read, first 1MB) | goblin | Yes | No |
| SCN-SFX-04 | Detect Linux ELF SFX | Rust developer | ELF binary with embedded archive | is_sfx=true, correct format, offset, stub_type=ELF | sfx/detection, sfx/signatures, sfx/stub_types | None | Executable file (read, first 1MB) | goblin (ELF parsing) | Yes | No |
| SCN-SFX-05 | Detect shell script SFX | Rust developer | Shell script + embedded tar.gz | is_sfx=true, format detected, offset, stub_type=Script | sfx/detection, sfx/signatures, sfx/stub_types | None | Script file (read, first 1MB) | (no binary parser needed) | Yes | No |
| SCN-SFX-06 | Reject standard archive (not SFX) | Rust developer | Regular archive file (ZIP, RAR, 7z) | is_sfx=false | sfx/detection, sfx/stub_types | None | Archive file (read, header) | goblin (fails PE/ELF parse) | Yes | No |
| SCN-SFX-07 | Reject non-archive executable | Rust developer | Regular binary (no embedded archive) | is_sfx=false | sfx/detection, sfx/signatures, sfx/stub_types | None | Executable file (read, first 1MB) | goblin | Yes | No |
| SCN-SFX-08 | Detect SFX with unknown stub | Rust developer | SFX with custom stub | Heuristic signature scan finds archive | sfx/detection, sfx/signatures | None | File (read, first 1MB) | (signature scan only) | No | No |
