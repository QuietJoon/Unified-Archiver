// Build script for FFI linking configuration
// This configures pkg-config to link against libarchive and compiles the
// vendored UnRAR C++ sources with the `cc` crate.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Env tracking for pkg-config discovery (R0076-0023): `probe_library`
    // runs with `env_metadata` enabled, so the pkg-config crate itself emits
    // `cargo:rerun-if-env-changed` for every variable it actually consults
    // (PKG_CONFIG, PKG_CONFIG_PATH/LIBDIR/SYSROOT_DIR, LIBARCHIVE_* overrides,
    // and their target-suffixed variants). This script reads no discovery env
    // vars directly. The UnRAR build (see build_unrar) is driven by the `cc`
    // crate, which emits its own `cargo:rerun-if-env-changed` for the toolchain
    // vars it consults (CC/CXX/AR/CFLAGS/CXXFLAGS and target-suffixed variants),
    // so no directive is duplicated here.

    // Link to libarchive using pkg-config
    // This will automatically handle platform-specific linking
    #[cfg(not(target_os = "windows"))]
    {
        // On macOS with Homebrew, libarchive is keg-only
        #[cfg(target_os = "macos")]
        {
            // Apple Silicon (`/opt/homebrew`) and Intel (`/usr/local`) layouts.
            let candidates = [
                std::path::PathBuf::from("/opt/homebrew/opt/libarchive"),
                std::path::PathBuf::from("/usr/local/opt/libarchive"),
            ];
            // A prefix only counts when it actually carries the libarchive
            // dylib we link against (`cargo:rustc-link-lib=dylib=archive`
            // resolves to `{prefix}/lib/libarchive.dylib`). A bare directory
            // (a stale keg or partial install) must fall through to pkg-config
            // rather than emit a link search path that defers to a cryptic
            // linker error (R0081-0087).
            let homebrew_prefix = candidates
                .iter()
                .find(|p| p.join("lib/libarchive.dylib").exists());
            if let Some(prefix) = homebrew_prefix {
                println!("cargo:rustc-link-search=native={}/lib", prefix.display());
                println!("cargo:rustc-link-lib=dylib=archive");
                eprintln!(
                    "unified-archive: using Homebrew libarchive from {}",
                    prefix.display()
                );
            } else if pkg_config::probe_library("libarchive").is_err() {
                panic!("libarchive not found. Install it with: brew install libarchive");
            }
        }

        // On Linux, use pkg-config
        #[cfg(not(target_os = "macos"))]
        {
            if pkg_config::probe_library("libarchive").is_err() {
                panic!(
                    "libarchive not found via pkg-config. Install libarchive development files: \
                     `sudo apt-get install libarchive-dev` (Ubuntu/Debian) or \
                     `sudo dnf install libarchive-devel` (Fedora/RHEL)."
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, we'll bundle libarchive or expect it in a known location
        // This will be implemented later with vcpkg or direct linking
        println!(
            "cargo:warning=Windows libarchive linking will be configured in future implementation"
        );
    }

    // Build and link the UnRAR library from the vendored C++ sources. Gated on
    // the `rar-support` feature only — not on the build host — because the
    // `cc`-crate builder honours Cargo's TARGET/CC/CXX/AR and picks MSVC on
    // Windows, so the same code path produces a native Windows, macOS, or Linux
    // static library (and cross builds), replacing the former `make`-only Unix
    // build that panicked on Windows (MADR-0020 — the record older documents
    // cite as "AD 0020" — plus AD 0039; OI-0080-001 vendored half).
    #[cfg(feature = "rar-support")]
    build_unrar();
}

// UNIX (`make lib` target) curated object set. Many vendored `.cpp` files are
// `#include`d by others (unpack15/20/30/50, crypt1..5, recvol3/5, blake2s_sse,
// win32*, ulinks, uowners, hardlinks, …) and must NOT be compiled standalone,
// or the link fails on duplicate symbols. This mirrors OBJECTS + LIB_OBJ from
// the vendored makefile's `lib` target exactly.
#[cfg(feature = "rar-support")]
const UNIX_SOURCES: [&str; 48] = [
    "rar.cpp",
    "strlist.cpp",
    "strfn.cpp",
    "pathfn.cpp",
    "smallfn.cpp",
    "global.cpp",
    "file.cpp",
    "filefn.cpp",
    "filcreat.cpp",
    "archive.cpp",
    "arcread.cpp",
    "unicode.cpp",
    "system.cpp",
    "crypt.cpp",
    "crc.cpp",
    "rawread.cpp",
    "encname.cpp",
    "resource.cpp",
    "match.cpp",
    "timefn.cpp",
    "rdwrfn.cpp",
    "consio.cpp",
    "options.cpp",
    "errhnd.cpp",
    "rarvm.cpp",
    "secpassword.cpp",
    "rijndael.cpp",
    "getbits.cpp",
    "sha1.cpp",
    "sha256.cpp",
    "blake2s.cpp",
    "hash.cpp",
    "extinfo.cpp",
    "extract.cpp",
    "volume.cpp",
    "list.cpp",
    "find.cpp",
    "unpack.cpp",
    "headers.cpp",
    "threadpool.cpp",
    "rs16.cpp",
    "cmddata.cpp",
    "ui.cpp",
    "largepage.cpp",
    "filestr.cpp",
    "scantree.cpp",
    "dll.cpp",
    "qopen.cpp",
];

// Windows (`UnRARDll.vcxproj`) curated `<ClCompile>` set. Differs from the UNIX
// set: adds the Windows-only `isnt`/`motw`/`rarpch` and Reed-Solomon `rs`, drops
// the UNIX `resource` and `list` stubs. Kept faithful to the upstream project so
// the same set of translation units compiles under MSVC.
#[cfg(feature = "rar-support")]
const WINDOWS_SOURCES: [&str; 50] = [
    "archive.cpp",
    "arcread.cpp",
    "blake2s.cpp",
    "cmddata.cpp",
    "consio.cpp",
    "crc.cpp",
    "crypt.cpp",
    "dll.cpp",
    "encname.cpp",
    "errhnd.cpp",
    "extinfo.cpp",
    "extract.cpp",
    "filcreat.cpp",
    "file.cpp",
    "filefn.cpp",
    "filestr.cpp",
    "find.cpp",
    "getbits.cpp",
    "global.cpp",
    "hash.cpp",
    "headers.cpp",
    "isnt.cpp",
    "largepage.cpp",
    "match.cpp",
    "motw.cpp",
    "options.cpp",
    "pathfn.cpp",
    "qopen.cpp",
    "rar.cpp",
    "rarpch.cpp",
    "rarvm.cpp",
    "rawread.cpp",
    "rdwrfn.cpp",
    "rijndael.cpp",
    "rs.cpp",
    "rs16.cpp",
    "scantree.cpp",
    "secpassword.cpp",
    "sha1.cpp",
    "sha256.cpp",
    "smallfn.cpp",
    "strfn.cpp",
    "strlist.cpp",
    "system.cpp",
    "threadpool.cpp",
    "timefn.cpp",
    "ui.cpp",
    "unicode.cpp",
    "unpack.cpp",
    "volume.cpp",
];

/// Compile the vendored UnRAR C++ sources into a static `libunrar.a` with the
/// `cc` crate and emit the link directives.
///
/// The `cc` builder writes all objects and the archive under `OUT_DIR`, so the
/// vendored source tree is never mutated (the hermeticity guarantee of AD 0039,
/// now delivered without the previous staging/artifact-purge machinery). The
/// source list and preprocessor defines are selected from Cargo's *target* OS,
/// not the build host, and `cc` forwards TARGET/CC/CXX/AR and picks MSVC on
/// Windows — so this one code path serves native Windows/macOS/Linux and cross
/// builds (the vendored-UnRAR half of OI-0080-001).
#[cfg(feature = "rar-support")]
fn build_unrar() {
    use std::path::PathBuf;

    let src_dir = PathBuf::from("src/ffi/native/unrar");

    // Cargo target selection (build scripts must key native compilation on the
    // target, not on host `cfg`, which evaluates for the build machine).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let is_windows = target_os == "windows";

    eprintln!("unified-archive: building vendored UnRAR (cc) for target_os={target_os}");

    let mut build = cc::Build::new();
    build.cpp(true);
    // Third-party C++: don't turn on cc's extra warnings for it. The specific
    // upstream suppressors are still applied below.
    build.warnings(false);

    let sources: &[&str] = if is_windows {
        &WINDOWS_SOURCES
    } else {
        &UNIX_SOURCES
    };
    for name in sources {
        build.file(src_dir.join(name));
    }

    // Preprocessor macros — mirror the upstream builds.
    if is_windows {
        // UnRARDll.vcxproj: RARDLL;UNRAR;SILENT (the full-crypto DLL variant,
        // not the RAR_NOCRYPT one).
        build.define("RARDLL", None);
        build.define("UNRAR", None);
        build.define("SILENT", None);
    } else {
        // makefile `lib`: WHAT=RARDLL (=> -DRARDLL) plus the shared DEFINES.
        // os.hpp auto-defines SILENT whenever RARDLL is set. RAR_SMP enables the
        // pthread threadpool used by the multi-threaded unpacker.
        build.define("RARDLL", None);
        build.define("_FILE_OFFSET_BITS", "64");
        build.define("_LARGEFILE_SOURCE", None);
        build.define("RAR_SMP", None);
    }

    // Compiler flags. `flag_if_supported` keeps the clang-specific warning
    // suppressors from breaking GCC/MSVC, which don't recognise them.
    if target_env != "msvc" {
        build.flag_if_supported("-std=c++11");
        build.flag_if_supported("-Wno-logical-op-parentheses");
        build.flag_if_supported("-Wno-switch");
        build.flag_if_supported("-Wno-dangling-else");
    }
    if !is_windows {
        build.pic(true); // matches the makefile's LIBFLAGS=-fPIC for `lib`
        build.flag_if_supported("-pthread"); // RAR_SMP threadpool
    }

    // Compiles every source, archives them, and emits both
    // `cargo:rustc-link-search=native=<OUT_DIR>` and
    // `cargo:rustc-link-lib=static=unrar`. The C++ standard library link
    // directive (libc++ on macOS, libstdc++ on Linux, none needed for MSVC) is
    // emitted automatically by `cc` because `cpp(true)` is set — so the manual
    // stdlib linking the make-based build carried is no longer needed.
    build.compile("unrar");

    // RAR_SMP uses pthreads on Unix. Rust's std already links pthread on Linux,
    // but declare it explicitly for the vendored objects' benefit. macOS ships
    // pthread in libSystem (linked by default), so no directive is needed there.
    if target_os == "linux" || target_os == "android" {
        println!("cargo:rustc-link-lib=dylib=pthread");
    }

    emit_unrar_rerun(&src_dir);
}

/// Re-run only when a vendored input actually changes. `cc` tracks the compiled
/// `.cpp` files it is handed, but not the `.hpp`/`.h` headers they `#include`
/// (nor the sibling `.cpp` files that are `#include`d rather than compiled), so
/// walk the whole vendored tree and emit `rerun-if-changed` for every source and
/// header. The vendored `makefile` is no longer a build input, so it is not
/// watched.
#[cfg(feature = "rar-support")]
fn emit_unrar_rerun(src: &std::path::Path) {
    emit_unrar_rerun_sources(src);
}

/// Emit `rerun-if-changed` for every vendored source/header file, recursing into
/// subdirectories — a top-level-only walk would miss nested sources (R0076-0025).
#[cfg(feature = "rar-support")]
fn emit_unrar_rerun_sources(dir: &std::path::Path) {
    // Propagate filesystem errors instead of swallowing them: a missed
    // `rerun-if-changed` lets cargo reuse stale native output when a vendored
    // source actually changed. Panic with path-specific context so an
    // incomplete enumeration fails the build loudly (R0081-0085).
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| {
        panic!(
            "Failed to enumerate vendored UnRAR sources in {}: {e}",
            dir.display()
        )
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| {
            panic!(
                "Failed to read a directory entry in vendored UnRAR sources {}: {e}",
                dir.display()
            )
        });
        let path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|e| {
            panic!(
                "Failed to stat vendored UnRAR source {}: {e}",
                path.display()
            )
        });
        if file_type.is_dir() {
            emit_unrar_rerun_sources(&path);
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".cpp") || name.ends_with(".hpp") || name.ends_with(".h") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
