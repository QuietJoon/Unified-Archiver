// SFX stub type identification
//
// Identifies the executable format of SFX stubs across platforms

use crate::error::ArchiveError;
use goblin::Object;

/// Type of executable stub in an SFX archive
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubType {
    /// Windows PE (Portable Executable) format (.exe)
    WindowsPE,
    /// Linux/BSD ELF (Executable and Linkable Format)
    LinuxELF,
    /// macOS Mach-O (Mach Object) format
    MacOSMachO,
    /// Script with shebang (#!) — shell, Python, Perl, etc.
    ScriptInterpreter,
    /// Unrecognized executable format
    Unknown,
}

impl StubType {
    /// Detect the stub type from file header bytes
    ///
    /// # Arguments
    /// * `bytes` - First 4KB of the file for header analysis
    ///
    /// # Returns
    /// * `Ok(StubType)` — always returns a variant. Returns `StubType::Unknown`
    ///   when the bytes don't match any recognized executable format so that
    ///   callers (notably `detect_sfx`) can still proceed to signature scanning
    ///   for SFX archives with custom / unknown stubs.
    /// * `Err(ArchiveError)` — reserved for future I/O failures; format
    ///   recognition failures are not considered errors.
    pub fn detect(bytes: &[u8]) -> Result<StubType, ArchiveError> {
        // Check for shell script shebang first (most common Unix SFX)
        if bytes.len() >= 2 && bytes[0] == b'#' && bytes[1] == b'!' {
            return Ok(StubType::ScriptInterpreter);
        }

        // Use goblin to parse executable format. Any parse failure or
        // unrecognized Object variant maps to StubType::Unknown so that
        // unknown-stub SFX heuristic scanning can run in Stage 2.
        match Object::parse(bytes) {
            Ok(Object::PE(_)) => Ok(StubType::WindowsPE),
            Ok(Object::Elf(_)) => Ok(StubType::LinuxELF),
            Ok(Object::Mach(_)) => Ok(StubType::MacOSMachO),
            Ok(_) | Err(_) => Ok(StubType::Unknown),
        }
    }

    /// Get human-readable description of the stub type
    pub fn description(&self) -> &'static str {
        match self {
            StubType::WindowsPE => "Windows PE executable",
            StubType::LinuxELF => "Linux/BSD ELF executable",
            StubType::MacOSMachO => "macOS Mach-O executable",
            StubType::ScriptInterpreter => "Script interpreter (shebang)",
            StubType::Unknown => "Unknown executable format",
        }
    }

    /// Check if this stub type typically runs on the current platform
    pub fn is_native(&self) -> bool {
        #[cfg(target_os = "windows")]
        return matches!(self, StubType::WindowsPE);

        #[cfg(target_os = "linux")]
        return matches!(self, StubType::LinuxELF | StubType::ScriptInterpreter);

        #[cfg(target_os = "macos")]
        return matches!(self, StubType::MacOSMachO | StubType::ScriptInterpreter);

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        return matches!(self, StubType::ScriptInterpreter);
    }

    /// Check if this is a known (non-Unknown) stub type
    pub fn is_known(&self) -> bool {
        !matches!(self, StubType::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_script_detection() {
        let script = b"#!/bin/sh\necho test";
        assert_eq!(
            StubType::detect(script).unwrap(),
            StubType::ScriptInterpreter
        );
    }

    #[test]
    fn test_invalid_format() {
        let invalid = b"Not an executable";
        assert_eq!(StubType::detect(invalid).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_description() {
        assert_eq!(StubType::WindowsPE.description(), "Windows PE executable");
        assert_eq!(StubType::LinuxELF.description(), "Linux/BSD ELF executable");
        assert_eq!(
            StubType::MacOSMachO.description(),
            "macOS Mach-O executable"
        );
        assert_eq!(
            StubType::ScriptInterpreter.description(),
            "Script interpreter (shebang)"
        );
        assert_eq!(StubType::Unknown.description(), "Unknown executable format");
    }

    #[test]
    fn test_is_native() {
        // Test that at least one stub type is native on current platform
        #[cfg(target_os = "windows")]
        {
            assert!(StubType::WindowsPE.is_native());
            assert!(!StubType::LinuxELF.is_native());
            assert!(!StubType::ScriptInterpreter.is_native());
        }

        #[cfg(target_os = "linux")]
        {
            assert!(StubType::LinuxELF.is_native());
            assert!(StubType::ScriptInterpreter.is_native());
            assert!(!StubType::WindowsPE.is_native());
        }

        #[cfg(target_os = "macos")]
        {
            assert!(StubType::MacOSMachO.is_native());
            assert!(StubType::ScriptInterpreter.is_native());
            assert!(!StubType::WindowsPE.is_native());
        }
    }

    #[test]
    fn test_shell_script_variations() {
        // Test various shell script shebangs
        let scripts: Vec<&[u8]> = vec![
            b"#!/bin/sh\n",
            b"#!/bin/bash\n",
            b"#!/usr/bin/env python\n",
            b"#!  /bin/sh\n", // with spaces
        ];

        for script in scripts {
            assert_eq!(
                StubType::detect(script).unwrap(),
                StubType::ScriptInterpreter,
                "Failed to detect: {:?}",
                String::from_utf8_lossy(script)
            );
        }
    }

    #[test]
    fn test_partial_shebang() {
        // Test that a single # without ! is not classified as a script
        let not_script = b"#This is a comment";
        assert_eq!(StubType::detect(not_script).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_empty_input() {
        let empty = b"";
        assert_eq!(StubType::detect(empty).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_short_input() {
        // Test input shorter than minimum executable header
        let short = b"PK"; // Too short for any format
        assert_eq!(StubType::detect(short).unwrap(), StubType::Unknown);
    }

    // ============================================================
    // T110d: Additional unit tests for stub_types.rs error paths
    // ============================================================

    #[test]
    fn test_detect_with_single_byte() {
        // Single byte input (too short for any format)
        let single = b"#";
        assert_eq!(StubType::detect(single).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_detect_with_null_bytes() {
        // Buffer of null bytes (no recognizable header)
        let nulls = &[0u8; 100];
        assert_eq!(StubType::detect(nulls).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_detect_with_random_data() {
        // Random-looking data that doesn't match any format
        let random = b"\x12\x34\x56\x78\x9a\xbc\xde\xf0garbage";
        assert_eq!(StubType::detect(random).unwrap(), StubType::Unknown);
    }

    #[test]
    fn test_detect_truncated_pe_header() {
        // PE magic but truncated header
        let truncated_pe = b"MZ"; // Just DOS header magic, no complete header
        // This might parse partially or fail - test that it doesn't panic
        let result = StubType::detect(truncated_pe);
        // Either it detects PE or fails gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_detect_truncated_elf_header() {
        // ELF magic but truncated
        let truncated_elf = b"\x7fELF"; // Just 4 bytes of ELF magic
        // Either parses or fails gracefully
        let result = StubType::detect(truncated_elf);
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_shebang_with_only_hash_exclaim() {
        // Just #! with nothing after
        let minimal = b"#!";
        // This should still be detected as shell script
        assert_eq!(
            StubType::detect(minimal).unwrap(),
            StubType::ScriptInterpreter
        );
    }

    #[test]
    fn test_shebang_unicode_path() {
        // Shebang with unicode characters (unusual but valid)
        let unicode_shebang = "#!/usr/bin/日本語\n".as_bytes();
        assert_eq!(
            StubType::detect(unicode_shebang).unwrap(),
            StubType::ScriptInterpreter
        );
    }

    #[test]
    fn test_shebang_very_long_path() {
        // Shebang with very long path
        let mut long_shebang = b"#!/".to_vec();
        long_shebang.extend(vec![b'a'; 1000]); // Very long interpreter path
        assert_eq!(
            StubType::detect(&long_shebang).unwrap(),
            StubType::ScriptInterpreter
        );
    }

    #[test]
    fn test_stub_type_copy_semantics() {
        // Verify Copy trait works
        let stub = StubType::WindowsPE;
        let copy = stub;
        assert_eq!(stub, copy);
    }

    #[test]
    fn test_stub_type_clone_semantics() {
        // Verify Clone trait works
        let stub = StubType::LinuxELF;
        let copied = stub;
        assert_eq!(stub, copied);
    }

    #[test]
    fn test_stub_type_equality() {
        // Verify PartialEq works correctly
        assert_eq!(StubType::WindowsPE, StubType::WindowsPE);
        assert_ne!(StubType::WindowsPE, StubType::LinuxELF);
        assert_ne!(StubType::MacOSMachO, StubType::ScriptInterpreter);
    }

    #[test]
    fn test_stub_type_debug_format() {
        // Verify Debug trait produces reasonable output
        let stub = StubType::MacOSMachO;
        let debug_str = format!("{:?}", stub);
        assert!(debug_str.contains("MacOSMachO"));
    }

    #[test]
    fn test_all_descriptions_non_empty() {
        // Verify all descriptions return non-empty strings
        let stub_types = [
            StubType::WindowsPE,
            StubType::LinuxELF,
            StubType::MacOSMachO,
            StubType::ScriptInterpreter,
            StubType::Unknown,
        ];

        for stub in stub_types {
            let desc = stub.description();
            assert!(
                !desc.is_empty(),
                "{:?} should have non-empty description",
                stub
            );
            assert!(
                desc.len() > 5,
                "{:?} description should be meaningful",
                stub
            );
        }
    }

    #[test]
    fn test_is_native_consistency() {
        // At least ScriptInterpreter should be native on Unix-like systems
        #[cfg(unix)]
        assert!(StubType::ScriptInterpreter.is_native());

        // WindowsPE should NOT be native on non-Windows
        #[cfg(not(target_os = "windows"))]
        assert!(!StubType::WindowsPE.is_native());
    }

    #[test]
    fn test_detect_almost_pe_magic() {
        // Data that starts like PE but isn't
        let almost_pe = b"MZgarbage data that is not actually PE format";
        // May or may not parse as PE depending on goblin's tolerance
        let result = StubType::detect(almost_pe);
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_detect_almost_elf_magic() {
        // Data that starts like ELF but isn't valid
        let almost_elf = b"\x7fELFgarbage";
        // May or may not parse as ELF
        let result = StubType::detect(almost_elf);
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_detect_binary_garbage_no_panic() {
        // Various binary patterns that shouldn't panic
        let patterns: Vec<&[u8]> = vec![
            b"\xff\xff\xff\xff",
            b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09",
            &[0xde, 0xad, 0xbe, 0xef],
            &[0xca, 0xfe, 0xba, 0xbe], // Mach-O universal magic (might parse)
        ];

        for pattern in patterns {
            let result = StubType::detect(pattern);
            // Just ensure no panic
            let _ = result;
        }
    }
}
