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

    let src_dir = PathBuf::from("src/ffi/native/unrar");
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let build_dir = out_dir.join("unrar-build");

    // Stage the UnRAR source tree into OUT_DIR. The vendored makefile writes
    // object files and libraries alongside its sources, so building in-place
    // would pollute the tracked source tree. Staging keeps the repo clean and
    // makes the build hermetic per-target.
    if let Err(e) = stage_unrar_sources(&src_dir, &build_dir) {
        println!("cargo:warning=Failed to stage UnRAR sources: {e}");
        std::process::exit(1);
    }

    let status = match Command::new("make")
        .current_dir(&build_dir)
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

    println!("cargo:rustc-link-search=native={}", build_dir.display());
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

    // Re-run only when a vendored input actually changes. Watching the whole
    // directory would retrigger whenever stray build artifacts land in it.
    emit_unrar_rerun(&src_dir);
}

#[cfg(feature = "rar-support")]
fn stage_unrar_sources(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::fs;

    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if is_unrar_artifact(&name.to_string_lossy()) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            // fs::copy overwrites so re-staging keeps the build dir in sync.
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(feature = "rar-support")]
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    use std::fs;

    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(feature = "rar-support")]
fn is_unrar_artifact(name: &str) -> bool {
    name.ends_with(".o")
        || name.ends_with(".a")
        || name.ends_with(".so")
        || name.ends_with(".dylib")
        || name == "unrar"
        || name == "default.sfx"
}

#[cfg(feature = "rar-support")]
fn emit_unrar_rerun(src: &std::path::Path) {
    println!("cargo:rerun-if-changed={}/makefile", src.display());
    let Ok(entries) = std::fs::read_dir(src) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".cpp") || name.ends_with(".hpp") || name.ends_with(".h") {
            println!("cargo:rerun-if-changed={}/{}", src.display(), name);
        }
    }
}
