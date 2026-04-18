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
            // Apple Silicon (`/opt/homebrew`) and Intel (`/usr/local`) layouts.
            let candidates = [
                std::path::PathBuf::from("/opt/homebrew/opt/libarchive"),
                std::path::PathBuf::from("/usr/local/opt/libarchive"),
            ];
            let homebrew_prefix = candidates.iter().find(|p| p.exists());
            if let Some(prefix) = homebrew_prefix {
                println!("cargo:rustc-link-search=native={}/lib", prefix.display());
                println!("cargo:rustc-link-lib=dylib=archive");
                println!(
                    "cargo:warning=Using Homebrew libarchive from {}",
                    prefix.display()
                );
            } else if pkg_config::probe_library("libarchive").is_err() {
                println!("cargo:warning=libarchive not found");
                println!("cargo:warning=Please install: brew install libarchive");
                std::process::exit(1);
            }
        }

        // On Linux, use pkg-config
        #[cfg(not(target_os = "macos"))]
        {
            if pkg_config::probe_library("libarchive").is_err() {
                println!("cargo:warning=libarchive not found via pkg-config");
                println!("cargo:warning=Please install libarchive development files:");
                println!("cargo:warning=  Ubuntu/Debian: sudo apt-get install libarchive-dev");
                println!("cargo:warning=  Fedora/RHEL: sudo dnf install libarchive-devel");
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

    // Build and link UnRAR library (only when rar-support feature is enabled)
    #[cfg(feature = "rar-support")]
    build_unrar();
}

#[cfg(feature = "rar-support")]
fn build_unrar() {
    use std::path::PathBuf;
    use std::process::Command;

    println!("cargo:warning=Building UnRAR library for RAR/RAR5 support");

    let unrar_dir = PathBuf::from("src/ffi/native/unrar");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Compile UnRAR library using its makefile
    let status = match Command::new("make")
        .current_dir(&unrar_dir)
        .arg("lib")
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=Failed to execute make: {e}");
            println!("cargo:warning=Install 'make' or disable the rar-support feature");
            std::process::exit(1);
        }
    };

    if !status.success() {
        println!("cargo:warning=Failed to build UnRAR library (make returned {status})");
        std::process::exit(1);
    }

    // Copy libunrar.a to OUT_DIR for linking
    if let Err(e) = std::fs::copy(
        unrar_dir.join("libunrar.a"),
        PathBuf::from(&out_dir).join("libunrar.a"),
    ) {
        println!("cargo:warning=Failed to copy libunrar.a: {e}");
        std::process::exit(1);
    }

    // Tell cargo to link the library
    println!("cargo:rustc-link-search=native={out_dir}");
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
