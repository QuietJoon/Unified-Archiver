---
okf_version: "0.2"
---
# Manual

Human-facing manual for the `unified-archive` crate, organized by the four Diataxis
needs. Tutorials teach, how-to guides solve a stated goal, reference describes the
machinery, explanation gives the reasoning. Each page names its audience: **user** (you
are calling the crate from your own program), **developer** (you are working on the
crate), **operator** (you build, package, or CI-run it).

## Tutorials
* [Getting started with unified-archive](tutorials/user/en/getting-started.md) - Install the native prerequisites, then write one program that opens a ZIP archive, lists its entries, and extracts it. (user)
* [Your first archive, created then changed](tutorials/user/en/your-first-archive.md) - Write a ZIP with Archive::create, reopen it to confirm the entries, then add and remove entries through Archive::modify. (user)
* [Building and testing unified-archive from source](tutorials/developer/en/build-and-test-from-source.md) - A first pass through the contributor workflow: prerequisites, cargo build, the test suite, the lint gates, and one example run. (developer)

## How-to guides
* [How to add, replace, or remove entries in an existing archive](how-to/user/en/modify-a-zip-or-7z.md) - Queue additions, replacements, and removals against an existing ZIP or 7z archive and commit them as one atomic rewrite. (user)
* [How to create an archive in a given format](how-to/user/en/create-an-archive.md) - Pick a creatable format, build the right options value, add entries, finalize the writer, and recognise the refusals this recipe runs into. (user)
* [How to detect and open a self-extracting archive](how-to/user/en/handle-self-extracting-archives.md) - Detect an SFX executable, read the detection verdict, open or stage its embedded payload, and pull the stub bytes out for inspection. (user)
* [How to extract a multi-part RAR set](how-to/user/en/extract-multi-part-rar-sets.md) - Opens the right volume of a split RAR set, confirms the set with detect_multipart, and extracts it in one call. (user)
* [How to extract an untrusted archive safely](how-to/user/en/extract-untrusted-archives-safely.md) - Configure extraction limits, an overwrite policy, and checksum verification before extracting an archive from an untrusted source, then inspect the warnings the run returns. (user)
* [How to open password-protected archives](how-to/user/en/open-password-protected-archives.md) - Detect that an archive needs a password, open it with one, pass it through extraction options, and read the failure when it is wrong. (user)
* [How to pick the right extraction call](how-to/user/en/choose-an-extraction-api.md) - Routes an extraction goal to the matching Archive method and shows the two selection choices that are easy to get wrong. (user)
* [How to report progress and cancel an operation](how-to/user/en/report-progress-and-cancel.md) - Wire a ProgressCallback into extraction, creation, and SFX staging, throttle it, and understand what a cancellation leaves on disk. (user)
* [How to stream a large entry](how-to/user/en/stream-a-large-entry.md) - Read one archive entry incrementally through a bounded StreamingExtractor and pick the StreamBound that matches your trust in the archive. (user)
* [How to verify an archive's integrity](how-to/user/en/verify-archive-integrity.md) - Recipes for validating an archive with validate_integrity, verify_crc32, the standalone stream-checksum helpers, and the content digests. (user)
* [How to satisfy the native build dependencies](how-to/operator/en/install-native-dependencies.md) - Per-platform recipes for libarchive, pkg-config, and the C++ toolchain, plus how to check the libarchive write filters and how to drop RAR support. (operator)

## Reference
* [Errors and warnings](reference/user/en/errors-and-warnings.md) - Every ArchiveError variant, its fields, its Display text and what raises it, plus the Operation labels, ArchiveWarning variants, and ResultWithWarnings. (user)
* [Format support matrix](reference/user/en/format-support-matrix.md) - Complete per-format capability table, extension and backend mapping, and detection rules as ArchiveFormat defines them. (user)
* [Options and defaults](reference/user/en/options-and-defaults.md) - Field-by-field description of every option and limit type in unified-archive, with types, defaults, and per-backend semantics. (user)
* [Public API surface](reference/user/en/public-api-surface.md) - A complete index of everything unified-archive exports — crate-root re-exports, public modules, every public Archive method by mode, and the v2 typed handles. (user)
* [Cargo features and MSRV](reference/developer/en/cargo-features.md) - Every Cargo feature of unified-archive with its platform gate, licence consequence, and disabled behaviour, plus the crate's edition, MSRV, and dependency set. (developer)
* [Build environment](reference/operator/en/build-environment.md) - What a unified-archive build requires per platform, exactly what build.rs does, which libraries end up on the link line, and the current platform support status. (operator)

## Explanation
* [One API over many backends](explanation/user/en/one-api-many-backends.md) - Why unified-archive puts a single Archive type over an internal backend enum, how formats map to engines, and which asymmetries the facade refuses to hide. (user)
* [The extraction safety model](explanation/user/en/extraction-safety-model.md) - What unified-archive defends against when it extracts an archive, why the gates run before the first byte is written, and what the model deliberately leaves to you. (user)
* [What archive checksums actually prove](explanation/user/en/checksums-and-integrity.md) - Discussion of the three kinds of checksum this crate exposes, what each one detects, and where the guarantees stop. (user)
* [How SFX detection decides](explanation/developer/en/sfx-detection-pipeline.md) - Why self-extracting-archive detection is staged, why its verdict is an enum with an evidence trail, and what it refuses to claim. (developer)
* [How this project records decisions](explanation/developer/en/decision-records-and-reviews.md) - The append-only decision store under docs/records, the review gates that feed it, the open-issues ledger, and how code points back at the ruling that authorised it. (developer)
* [Streaming and memory behaviour](explanation/developer/en/streaming-and-memory.md) - Why one entry-reader type covers backends with very different memory profiles, and why exceeding the stream's cap is an error instead of a silent end of data. (developer)
* [Why modification rewrites the archive](explanation/developer/en/modification-is-a-rewrite.md) - The reasoning behind copy-on-write modification, the advisory lock and inode-identity guard that protect it, and why only ZIP and 7z are in scope. (developer)
