---
type: Tutorial
title: Building and testing unified-archive from source
description: A first pass through the contributor workflow: prerequisites, cargo build, the test suite, the lint gates, and one example run.
tags: [build, platform, getting-started]
audience: developer
language: en
generated:
  by: claude-code/claude-opus-5
  at: 2026-08-05T14:29:24Z
sources:
  - { id: contributing, resource: CONTRIBUTING.md }
  - { id: readme, resource: README.md }
  - { id: build-script, resource: build.rs }
  - { id: manifest, resource: Cargo.toml }
  - { id: test-layout, resource: tests/README.md }
  - { id: architecture-overview, resource: docs/architecture/README.md }
synced_hash: 7e2abd7a690180b3a7446c0e49081bb48657c436a61b7490fedecbf56825f2a1
---

# Building and testing unified-archive from source

By the end of this lesson you will have compiled `unified-archive` 0.4.0 from a fresh clone, run
its test suite, passed both lint gates, run one of its examples against an archive that ships in
the repository, and produced a build that leaves the RAR backend out. Most of the elapsed time is
the first compile.

You need a macOS or Linux machine. Those are the two platforms the project tests, and this lesson
is written for them. Windows can be built, but it needs a different set of steps —
[How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md)
covers it.

You also need Rust 1.85 or newer, because the crate is on edition 2024. Check:

```bash
rustc --version
```

```
rustc 1.85.0 (4d91de4e4 2025-02-17)
```

Any version at or above `1.85.0` is fine. If yours is older, update your toolchain before going
on.

## Step 1 — install the native prerequisites

The crate links against the system libarchive and compiles a vendored UnRAR C++ source tree, so
you need libarchive's development files, `pkg-config`, and a C++ compiler. Run the line for your
system:

```bash
# Debian / Ubuntu
sudo apt-get install libarchive-dev pkg-config g++

# Fedora / RHEL
sudo dnf install libarchive-devel pkgconf-pkg-config gcc-c++

# macOS
brew install libarchive pkg-config
```

You do not need `make`: `build.rs` compiles the vendored UnRAR sources through the `cc` crate,
which drives your C++ compiler directly.

## Step 2 — clone the repository

```bash
git clone https://github.com/QuietJoon/unified-archive
cd unified-archive
```

## Step 3 — build

```bash
cargo build
```

The first run compiles the dependency tree and roughly fifty vendored C++ translation units, so
expect it to take a while. A green run ends like this:

```
   Compiling unified-archive v0.4.0 (/home/you/unified-archive)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 47s
```

Cargo hides the build script's own progress messages unless the build fails. To watch them, run
`cargo build -vv` and look for lines such as
`unified-archive: building vendored UnRAR (cc) for target_os=linux`.

If libarchive is missing, the build script stops with a message naming the package to install,
for example `libarchive not found. Install it with: brew install libarchive` on macOS. Go back to
Step 1, then run `cargo build` again.

## Step 4 — run the whole test suite

```bash
cargo test
```

This is a large suite. Twenty-seven standalone test files sit directly under `tests/`, two further
entry points (`tests/integration_tests.rs` and `tests/contract_tests.rs`) pull in the whole
`tests/integration/` and `tests/contract/` trees, and on top of that cargo runs the unit tests
compiled into `src/` and every documentation example. Plan for several minutes.

Each of those becomes its own test binary and reports separately: a `running N tests` line, one
character per test as it finishes, then a summary line of this shape:

```
test result: ok. N passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.42s
```

A green run is one where every binary ends in `test result: ok.` with `0 failed`, and the
`cargo test` command itself exits with status `0`.

Two kinds of message are normal in a green run and are not failures. Tests that shell out to
`zip`, `unzip`, `7z`, `tar`, or `rar.exe` print a skip line when the tool is not installed. Tests
that measure timing stay switched off unless you ask for them: the performance baselines need
`UA_PRINT_PERF_BASELINE=1` in the environment, and the one performance assertion in the contract
suite needs `UA_RUN_PERF_CONTRACT_TEST=1`.

## Step 5 — run just one test file

While you are working on a change you rarely want the full suite. Every file directly under
`tests/` is its own cargo test target, named after the file with `.rs` removed:

```bash
cargo test --test extraction_test
```

```
running 7 tests
.......

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
```

That file holds seven tests, all of them RAR cases, so all seven run as long as the default
feature set is in effect. Only the timing changes between runs.

The end-to-end suites under `tests/integration/` all run through the single `integration_tests`
target, and the public-API contract suites under `tests/contract/` through `contract_tests`:

```bash
cargo test --test integration_tests
```

Add a filter after `--` to narrow it further, for example
`cargo test --test integration_tests -- extraction`.

## Step 6 — pass the lint gate

```bash
cargo clippy --all-targets
```

`--all-targets` makes clippy look at the tests, examples, and benchmarks as well as the library.
A green run prints `Finished` and no line beginning with `warning:`.

## Step 7 — pass the formatting gate

```bash
cargo fmt --check
```

A green run prints nothing at all and exits `0`. If it prints a diff, run `cargo fmt` to apply it.

## Step 8 — run an example

The crate is a library with no binary of its own, but `examples/` holds runnable programs. Point
the inspector at a fixture archive that is already in the repository:

```bash
cargo run --example inspect_archive -- tests/fixtures/test.zip
```

```
Archive: tests/fixtures/test.zip
Format:  Zip
Entries: 1

Path                                                       Size   Compressed      CRC32
----------------------------------------------------------------------------------------
test_file.txt                                                18           18   054607BC
```

That single stored 18-byte entry is the whole fixture. Seeing the format detected as `Zip` and
the CRC32 read out of the central directory means the library, the backends you just built, and
your linkage against libarchive are all working together.

## Step 9 — build without RAR support

`rar-support` is in the crate's default feature set, which is why Step 3 compiled the vendored
UnRAR sources. Build once without it:

```bash
cargo build --no-default-features
```

This compile skips the C++ sources entirely, so it is markedly faster. In the resulting build,
`Archive::open` on a RAR or RAR5 file returns an `Unsupported` error saying RAR/RAR5 support is
disabled in this build; every other format behaves exactly as before. libarchive is still
required — the `--no-default-features` flag does not affect it.

You now have a working development loop. When you change something, the sequence is Step 3,
Step 5 for the file you touched, Step 4 before you commit, then Steps 6 and 7.

## Where to go next

- Every feature flag, what it turns on, and the MSRV policy:
  [Cargo features and MSRV](../../../reference/developer/en/cargo-features.md).
- The full list of tools, libraries, and environment variables the build consults:
  [Build environment](../../../reference/operator/en/build-environment.md).
- Platform-by-platform prerequisite recipes, including Windows and the libarchive codec check:
  [How to satisfy the native build dependencies](../../../how-to/operator/en/install-native-dependencies.md).
- How a change you make gets recorded, and where the decision records and review findings live:
  [How this project records decisions](../../../explanation/developer/en/decision-records-and-reviews.md).
