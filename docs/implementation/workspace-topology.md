# Workspace Topology

Repo layout, package members, and structural conventions.

## Repo Layout

```
unified-archive/
├── src/
│   ├── lib.rs                      # Public crate surface
│   ├── archive.rs                  # Core Archive type + backend routing + finish()
│   ├── entry.rs                    # ArchiveEntry, EntryType, FileAttributes
│   ├── error.rs                    # ArchiveError enum
│   ├── format.rs                   # ArchiveFormat detection + capabilities
│   ├── options.rs                  # ExtractionOptions, CompressionOptions, ProgressCallback
│   ├── security.rs                 # Path sanitization, ExtractionLimits
│   ├── inspection.rs               # list_files, find_entry, validate, multi-part
│   ├── extraction.rs               # extract_all, extract_file, parallel, progress
│   ├── creation.rs                 # Archive::create, add_file_from_data, add_file_from_path, add_file_from_path_as, add_directory, add_directory_recursive
│   ├── modification.rs             # Archive::modify, commit_changes
│   ├── streaming.rs                # StreamingExtractor (Read trait wrapper)
│   ├── stream_crc.rs               # GZIP/BZIP2/XZ checksum parsing
│   ├── ffi/
│   │   ├── mod.rs                  # FFI module root
│   │   ├── common.rs               # TempDirGuard, path normalization, CRC helpers
│   │   ├── unrar.rs                # Raw UnRAR C FFI bindings
│   │   ├── wrapper.rs              # Safe UnRAR adapter
│   │   ├── libarchive.rs           # Raw libarchive C FFI bindings
│   │   ├── libarchive_wrapper.rs   # Safe libarchive adapter
│   │   ├── piz_wrapper.rs          # Native Rust ZIP backend (mmap)
│   │   ├── sevenz_wrapper.rs       # Native Rust 7z backend
│   │   ├── zip_wrapper.rs          # Native Rust ZIP backend (encrypted)
│   │   └── zip_writer.rs           # Native Rust ZIP creation
│   ├── sfx.rs                     # SFX module root (re-exports submodules)
│   ├── sfx/
│   │   ├── detection.rs            # SFX detection pipeline
│   │   ├── result.rs               # SfxDetectionResult
│   │   ├── signatures.rs           # Archive signature tables
│   │   └── stub_types.rs           # WindowsPE, LinuxELF, MacOSMachO, ScriptInterpreter, Unknown
│   ├── test_utils.rs               # Shared test utilities (#[cfg(test)] only)
│   └── external/
│       ├── mod.rs                  # External tools module root
│       └── rar.rs                  # Optional WinRAR CLI (feature-gated)
├── tests/
│   ├── integration/                # Integration test modules
│   │   ├── concurrency.rs
│   │   ├── creation.rs
│   │   ├── modification.rs
│   │   ├── sfx_detection.rs
│   │   └── sfx_false_positives.rs
│   ├── format_compatibility_test.rs
│   ├── password_handling_test.rs
│   ├── crc32_verification_test.rs
│   ├── streaming_test.rs
│   ├── progress_callback_test.rs
│   ├── performance_test.rs
│   ├── integrity_comprehensive_test.rs
│   ├── property_tests.rs
│   └── ...
├── examples/                       # 9 usage examples
│   ├── inspect_archive.rs
│   ├── extract_archive.rs
│   ├── create_archive.rs
│   ├── modify_archive.rs
│   ├── streaming_extract.rs
│   └── ...
├── benches/                        # Criterion benchmarks
│   ├── archive_operations.rs
│   ├── extraction.rs
│   ├── inspection.rs
│   ├── integrity_validation_bench.rs
│   └── sfx_detection.rs
├── specs/001-unified-archive/      # Feature specification
│   ├── spec.md
│   ├── plan.md
│   ├── data-model.md
│   ├── tasks.md
│   ├── contracts/                  # API contracts (canonical location)
│   │   ├── archive.md
│   │   ├── inspection.md
│   │   ├── extraction.md
│   │   ├── progress.md
│   │   ├── streaming.md
│   │   └── errors.md
│   └── ...
├── docs/                           # Design-first-architecture docs
│   ├── architecture/
│   ├── implementation/
│   └── project/
├── reviews/                        # Decision log and reviewed items
├── issues/                         # Historical issue tracking
├── Cargo.toml
├── build.rs
├── README.md
├── CLAUDE.md
├── CONTRIBUTING.md
└── SECURITY.md
```

## Workspace / Package Members

| Package | Type | Path | Notes |
|---|---|---|---|
| `unified-archive` | Library crate | `/` (root) | The only package. No workspace. |

## Binary / Process Directories

None. This is a library-only crate.

## Shared Package Directories

Not applicable (single crate).

## Contract / Codegen Output Locations

| Location | Content |
|---|---|
| `specs/001-unified-archive/contracts/` | Canonical API contract documents (6 files) |
| `src/lib.rs` | Public API re-exports (the actual Rust contract surface) |

## Build Artifacts

| Artifact | Source | Notes |
|---|---|---|
| `build.rs` | Native link orchestration | Compiles/links UnRAR SDK (static), links libarchive (dynamic via pkg-config) |
| `CARGO_TARGET_DIR` | `/Volumes/Scratch/cargo_target` | External target dir (do not modify) |
