// Build script for FFI linking configuration
// This configures pkg-config to link against libarchive

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Link to libarchive using pkg-config
    // This will automatically handle platform-specific linking
    #[cfg(not(target_os = "windows"))]
    {
        // On macOS with Homebrew, libarchive is keg-only
        #[cfg(target_os = "macos")]
        {
            let homebrew_prefix = std::path::PathBuf::from("/opt/homebrew/opt/libarchive");
            if homebrew_prefix.exists() {
                println!(
                    "cargo:rustc-link-search=native={}/lib",
                    homebrew_prefix.display()
                );
                println!("cargo:rustc-link-lib=dylib=archive");
                println!(
                    "cargo:warning=Using Homebrew libarchive from {}",
                    homebrew_prefix.display()
                );
            } else {
                // Try standard pkg-config
                if pkg_config::probe_library("libarchive").is_err() {
                    eprintln!("ERROR: libarchive not found");
                    eprintln!("Please install: brew install libarchive");
                    std::process::exit(1);
                }
            }
        }

        // On Linux, use pkg-config
        #[cfg(not(target_os = "macos"))]
        {
            if pkg_config::probe_library("libarchive").is_err() {
                eprintln!("ERROR: libarchive not found via pkg-config");
                eprintln!("Please install libarchive development files:");
                eprintln!("  Ubuntu/Debian: sudo apt-get install libarchive-dev");
                eprintln!("  Fedora/RHEL: sudo dnf install libarchive-devel");
                std::process::exit(1);
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

    // Build and link UnRAR library (MANDATORY for RAR/RAR5 CRC32 support)
    build_unrar();
}

fn build_unrar() {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:warning=Building UnRAR library for RAR/RAR5 support");

    let unrar_dir = PathBuf::from("src/ffi/native/unrar");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Compile UnRAR library using its makefile
    let status = Command::new("make")
        .current_dir(&unrar_dir)
        .arg("lib")
        .status()
        .expect("Failed to execute make - is make installed?");

    if !status.success() {
        panic!("Failed to build UnRAR library");
    }

    // Copy libunrar.a to OUT_DIR for linking
    std::fs::copy(
        unrar_dir.join("libunrar.a"),
        PathBuf::from(&out_dir).join("libunrar.a"),
    )
    .expect("Failed to copy libunrar.a");

    // Tell cargo to link the library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=unrar");

    // Link C++ standard library (CRITICAL for UnRAR C++ code)
    #[cfg(target_os = "macos")]
    {
        // macOS uses libc++ (C++11+) with modern clang
        println!("cargo:rustc-link-lib=dylib=c++");
        println!("cargo:rustc-link-search=native=/usr/lib");
    }

    #[cfg(target_os = "linux")]
    {
        // Linux typically uses libstdc++
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    #[cfg(target_os = "windows")]
    {
        // Windows uses MSVCRT
        println!("cargo:rustc-link-lib=dylib=msvcrt");
    }

    println!("cargo:rerun-if-changed=src/ffi/native/unrar");
}
