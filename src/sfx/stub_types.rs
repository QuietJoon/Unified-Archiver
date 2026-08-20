// SFX stub type identification
//
// Identifies the executable format of SFX stubs across platforms

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
    /// * `bytes` - First 4KB of the file for header analysis. The fixed
    ///   ELF/PE/Mach-O header must be present in that window; a shorter
    ///   prefix classifies as `Unknown` rather than as an executable
    ///   (R0001-0066 / R0001-0067).
    ///
    /// # Returns
    /// Always a variant — `StubType::Unknown` when the bytes don't
    /// match any recognized executable format. Format recognition
    /// failures are not errors; the check is pure and infallible.
    pub fn detect(bytes: &[u8]) -> StubType {
        // Check for shell script shebang first (most common Unix SFX).
        //
        // R0075-0069: require a proper newline-terminated interpreter
        // path (`#!<at least one non-whitespace path char>...\n`) so
        // a binary that happens to start with the bare two bytes
        // `#!` is not classified as a script SFX. Interpreters are
        // capped at 4 KiB by Linux's `BINPRM_BUF_SIZE`; we check up
        // to the lesser of the buffer length and 4 KiB.
        if bytes.len() >= 4 && bytes[0] == b'#' && bytes[1] == b'!' {
            const MAX_SHEBANG: usize = 4096;
            let scan_len = bytes.len().min(MAX_SHEBANG);
            let after_shebang = &bytes[2..scan_len];
            if let Some(newline_pos) = after_shebang.iter().position(|b| *b == b'\n') {
                let line = &after_shebang[..newline_pos];
                // Skip leading spaces/tabs after `#!`, then require
                // at least one non-whitespace, non-control character
                // (a path separator or a name byte). This rejects
                // `#! \n` and `#!\t\n` and similar empty-path forms.
                let trimmed = line
                    .iter()
                    .skip_while(|b| **b == b' ' || **b == b'\t')
                    .copied()
                    .collect::<Vec<u8>>();
                let has_path = trimmed
                    .iter()
                    .any(|b| !b.is_ascii_whitespace() && !b.is_ascii_control());
                if has_path {
                    return StubType::ScriptInterpreter;
                }
            }
        }

        // R0079-0010: classify native executables from header fields
        // only. goblin's `Object::parse` walks section tables / import
        // data whose file offsets sit far beyond the 4 KiB header window
        // for any real PE/ELF stub, so parsing the truncated prefix
        // failed and every native SFX stub classified as Unknown. The
        // checks below need only prefix bytes. Anything unrecognized
        // maps to StubType::Unknown so unknown-stub handling can run in
        // Stage 2.
        if is_elf(bytes) {
            return StubType::LinuxELF;
        }
        if is_pe(bytes) {
            return StubType::WindowsPE;
        }
        if is_macho(bytes) {
            return StubType::MacOSMachO;
        }
        StubType::Unknown
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

        // R0001-0068: the BSDs run ELF natively too, so they share the
        // Linux arm. Previously they fell into the catch-all below and
        // reported a local ELF stub as foreign, contradicting
        // `LinuxELF`'s own "Linux/BSD ELF executable" description.
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        return matches!(self, StubType::LinuxELF | StubType::ScriptInterpreter);

        #[cfg(target_os = "macos")]
        return matches!(self, StubType::MacOSMachO | StubType::ScriptInterpreter);

        #[cfg(not(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "macos"
        )))]
        return matches!(self, StubType::ScriptInterpreter);
    }

    /// Check if this is a known (non-Unknown) stub type
    pub fn is_known(&self) -> bool {
        !matches!(self, StubType::Unknown)
    }
}

/// Read a 16-bit header field at `offset` in the byte order the header
/// declares, or `None` when the probe window is too short. Every
/// multi-byte header read goes through this so a truncated probe can
/// never index out of bounds (R0001-0066 / R0001-0067).
fn read_u16(bytes: &[u8], offset: usize, big_endian: bool) -> Option<u16> {
    let raw: [u8; 2] = bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(if big_endian {
        u16::from_be_bytes(raw)
    } else {
        u16::from_le_bytes(raw)
    })
}

/// 32-bit counterpart of [`read_u16`].
fn read_u32(bytes: &[u8], offset: usize, big_endian: bool) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(if big_endian {
        u32::from_be_bytes(raw)
    } else {
        u32::from_le_bytes(raw)
    })
}

/// ELF identification: `e_ident` (magic plus sane class/data/version
/// fields — `EI_CLASS`/`EI_DATA` in {1, 2}, `EI_VERSION` == 1) followed
/// by the rest of the fixed header.
///
/// R0001-0066: `e_ident` alone let incidental prefix bytes pose as an
/// executable stub. The whole fixed header (52 bytes for ELFCLASS32, 64
/// for ELFCLASS64) sits inside the 4 KiB probe window, so `e_type`
/// (executable image only), `e_version` and `e_ehsize` are validated
/// too. Multi-byte fields honour `EI_DATA` byte order and every read is
/// bounds-checked, matching `is_pe`'s shape.
fn is_elf(bytes: &[u8]) -> bool {
    const ET_EXEC: u16 = 2;
    const ET_DYN: u16 = 3;
    const EV_CURRENT: u32 = 1;
    // `e_ehsize` — also the minimum header length — per EI_CLASS.
    const EHSIZE_32: u16 = 52;
    const EHSIZE_64: u16 = 64;

    if bytes.len() < 7 || bytes[..4] != *b"\x7fELF" {
        return false;
    }
    let (class, data) = (bytes[4], bytes[5]);
    if !matches!(class, 1 | 2) || !matches!(data, 1 | 2) || bytes[6] != 1 {
        return false;
    }
    // EI_DATA: 1 = ELFDATA2LSB, 2 = ELFDATA2MSB.
    let big_endian = data == 2;
    // EI_CLASS: 1 = ELFCLASS32, 2 = ELFCLASS64. The class fixes both the
    // header size and where `e_ehsize` lives.
    let (ehsize, ehsize_offset) = if class == 1 {
        (EHSIZE_32, 0x28)
    } else {
        (EHSIZE_64, 0x34)
    };
    if bytes.len() < ehsize as usize {
        return false;
    }
    // e_type (0x10): an SFX stub is an executable image, never a
    // relocatable object, a core dump, or ET_NONE.
    let Some(e_type) = read_u16(bytes, 0x10, big_endian) else {
        return false;
    };
    if !matches!(e_type, ET_EXEC | ET_DYN) {
        return false;
    }
    // e_version (0x14) restates EI_VERSION as a word; both must be 1.
    if read_u32(bytes, 0x14, big_endian) != Some(EV_CURRENT) {
        return false;
    }
    // e_ehsize must match the size the class implies.
    read_u16(bytes, ehsize_offset, big_endian) == Some(ehsize)
}

/// PE identification: DOS `MZ` magic plus `e_lfanew` (offset 0x3C)
/// pointing at the `PE\0\0` COFF signature. `e_lfanew` must land inside
/// the supplied header window — real stubs keep it in the first few
/// hundred bytes, comfortably inside the 4 KiB probe.
fn is_pe(bytes: &[u8]) -> bool {
    if bytes.len() < 0x40 || bytes[..2] != *b"MZ" {
        return false;
    }
    let e_lfanew =
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    e_lfanew
        .checked_add(4)
        .and_then(|end| bytes.get(e_lfanew..end))
        .is_some_and(|magic| magic == b"PE\0\0")
}

/// Mach-O identification: thin 32/64-bit magics in either byte order
/// whose fixed header validates, plus fat (universal) headers gated on
/// a plausible architecture count so Java class files (which share the
/// 0xCAFEBABE magic but follow it with a version word whose major byte
/// is >= 45) are not misclassified.
fn is_macho(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    // (magic bytes, 64-bit header, big-endian file). MH_CIGAM* are the
    // byte-swapped spellings of MH_MAGIC*, so the magic that matched
    // also fixes the byte order of every field behind it.
    const THIN_MAGICS: [([u8; 4], bool, bool); 4] = [
        ([0xFE, 0xED, 0xFA, 0xCE], false, true), // MH_MAGIC (big-endian 32-bit)
        ([0xCE, 0xFA, 0xED, 0xFE], false, false), // MH_CIGAM (little-endian 32-bit)
        ([0xFE, 0xED, 0xFA, 0xCF], true, true),  // MH_MAGIC_64 (big-endian 64-bit)
        ([0xCF, 0xFA, 0xED, 0xFE], true, false), // MH_CIGAM_64 (little-endian 64-bit)
    ];
    if let Some(&(_, is_64, big_endian)) = THIN_MAGICS.iter().find(|(m, _, _)| bytes[..4] == *m) {
        // R0001-0067: four magic bytes used to be enough to mark
        // arbitrary data executable; validate the fixed header behind
        // the magic before classifying.
        return is_macho_header(bytes, is_64, big_endian);
    }
    // Fat header: FAT_MAGIC followed by big-endian nfat_arch. Apple
    // tooling never produces more than a handful of slices.
    bytes.len() >= 8
        && bytes[..4] == [0xCA, 0xFE, 0xBA, 0xBE]
        && matches!(
            u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            1..=16
        )
}

/// Validate the fixed `mach_header` / `mach_header_64` behind a thin
/// magic (R0001-0067). The 28-byte (32-bit) / 32-byte (64-bit) header
/// is entirely inside the probe window, so `filetype`, `ncmds` and
/// `sizeofcmds` can all be checked: an SFX stub is an executable image,
/// it always carries at least one load command, and its load-command
/// block (~1-3 KiB for real binaries) must fit in the window handed to
/// [`StubType::detect`]. `big_endian` comes from which magic matched.
fn is_macho_header(bytes: &[u8], is_64: bool, big_endian: bool) -> bool {
    const MH_EXECUTE: u32 = 2;
    const MH_DYLIB: u32 = 6;
    // Smallest possible load command: `cmd` + `cmdsize`. Bounding
    // `ncmds` by `sizeofcmds / 8` caps the count without a made-up
    // constant.
    const LOAD_COMMAND_MIN: u32 = 8;

    let header_size: usize = if is_64 { 32 } else { 28 };
    if bytes.len() < header_size {
        return false;
    }
    // filetype (0x0C): only executable images make sense as a stub.
    let Some(filetype) = read_u32(bytes, 0x0C, big_endian) else {
        return false;
    };
    if !matches!(filetype, MH_EXECUTE | MH_DYLIB) {
        return false;
    }
    // ncmds (0x10) and sizeofcmds (0x14).
    let (Some(ncmds), Some(sizeofcmds)) = (
        read_u32(bytes, 0x10, big_endian),
        read_u32(bytes, 0x14, big_endian),
    ) else {
        return false;
    };
    if ncmds == 0 {
        return false;
    }
    ncmds
        .checked_mul(LOAD_COMMAND_MIN)
        .is_some_and(|minimum| minimum <= sizeofcmds)
        && usize::try_from(sizeofcmds)
            .ok()
            .and_then(|size| size.checked_add(header_size))
            .is_some_and(|end| end <= bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_script_detection() {
        let script = b"#!/bin/sh\necho test";
        assert_eq!(StubType::detect(script), StubType::ScriptInterpreter);
    }

    #[test]
    fn test_invalid_format() {
        let invalid = b"Not an executable";
        assert_eq!(StubType::detect(invalid), StubType::Unknown);
    }

    #[test]
    fn test_is_known() {
        for known in [
            StubType::WindowsPE,
            StubType::LinuxELF,
            StubType::MacOSMachO,
            StubType::ScriptInterpreter,
        ] {
            assert!(known.is_known(), "{known:?} should be known");
        }
        assert!(!StubType::Unknown.is_known());
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
                StubType::detect(script),
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
        assert_eq!(StubType::detect(not_script), StubType::Unknown);
    }

    #[test]
    fn test_empty_input() {
        let empty = b"";
        assert_eq!(StubType::detect(empty), StubType::Unknown);
    }

    #[test]
    fn test_short_input() {
        // Test input shorter than minimum executable header
        let short = b"PK"; // Too short for any format
        assert_eq!(StubType::detect(short), StubType::Unknown);
    }

    // ============================================================
    // T110d: Additional unit tests for stub_types.rs error paths
    // ============================================================

    #[test]
    fn test_detect_with_single_byte() {
        // Single byte input (too short for any format)
        let single = b"#";
        assert_eq!(StubType::detect(single), StubType::Unknown);
    }

    #[test]
    fn test_detect_with_null_bytes() {
        // Buffer of null bytes (no recognizable header)
        let nulls = &[0u8; 100];
        assert_eq!(StubType::detect(nulls), StubType::Unknown);
    }

    #[test]
    fn test_detect_with_random_data() {
        // Random-looking data that doesn't match any format
        let random = b"\x12\x34\x56\x78\x9a\xbc\xde\xf0garbage";
        assert_eq!(StubType::detect(random), StubType::Unknown);
    }

    #[test]
    fn test_detect_truncated_pe_header() {
        // PE magic but truncated header — too short for the e_lfanew
        // chase, so it must classify as Unknown rather than panic.
        let truncated_pe = b"MZ"; // Just DOS header magic, no complete header
        assert_eq!(StubType::detect(truncated_pe), StubType::Unknown);
    }

    #[test]
    fn test_detect_truncated_elf_header() {
        // ELF magic but truncated before e_ident's class/data/version
        // fields — must classify as Unknown rather than panic.
        let truncated_elf = b"\x7fELF"; // Just 4 bytes of ELF magic
        assert_eq!(StubType::detect(truncated_elf), StubType::Unknown);
    }

    #[test]
    fn test_shebang_with_only_hash_exclaim() {
        // R0075-0069: bare `#!` without a newline-terminated
        // interpreter path is no longer classified as a script SFX
        // (previously it was, which let arbitrary binaries starting
        // with two bytes pose as scripts).
        let minimal = b"#!";
        assert_eq!(StubType::detect(minimal), StubType::Unknown);
    }

    #[test]
    fn test_shebang_unicode_path() {
        // Shebang with unicode characters (unusual but valid)
        let unicode_shebang = "#!/usr/bin/日本語\n".as_bytes();
        assert_eq!(
            StubType::detect(unicode_shebang),
            StubType::ScriptInterpreter
        );
    }

    #[test]
    fn test_shebang_very_long_path() {
        // R0075-0069: a long interpreter path is still accepted as
        // long as the line is newline-terminated within the 4 KiB
        // BINPRM_BUF_SIZE window. Without a newline, the detector
        // rejects (caught by `test_shebang_with_only_hash_exclaim`).
        let mut long_shebang = b"#!/".to_vec();
        long_shebang.extend(vec![b'a'; 1000]); // Very long interpreter path
        long_shebang.push(b'\n');
        assert_eq!(StubType::detect(&long_shebang), StubType::ScriptInterpreter);
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
        // Data that starts like PE but has no e_lfanew -> 'PE\0\0' chain
        let almost_pe = b"MZgarbage data that is not actually PE format";
        assert_eq!(StubType::detect(almost_pe), StubType::Unknown);
    }

    #[test]
    fn test_detect_almost_elf_magic() {
        // ELF magic with an invalid EI_CLASS byte ('g')
        let almost_elf = b"\x7fELFgarbage";
        assert_eq!(StubType::detect(almost_elf), StubType::Unknown);
    }

    // ============================================================
    // R0079-0010: header-field-only classification. goblin's eager
    // whole-file parsers failed on the truncated 4 KiB prefix for any
    // real PE/ELF stub; these synthetic minimal headers prove the
    // prefix-only checks classify correctly.
    // ============================================================

    /// Minimal PE prefix: `MZ` + `e_lfanew` at 0x3C pointing at the
    /// `PE\0\0` COFF signature at 0x40. No section table or import
    /// data — exactly what a 4 KiB window of a real stub provides.
    fn minimal_pe_header() -> Vec<u8> {
        let mut header = vec![0u8; 0x48];
        header[0] = b'M';
        header[1] = b'Z';
        header[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        header[0x40..0x44].copy_from_slice(b"PE\0\0");
        header
    }

    /// Write a 16-bit header field in the requested byte order.
    fn write_u16(buf: &mut [u8], offset: usize, value: u16, big_endian: bool) {
        let raw = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        buf[offset..offset + 2].copy_from_slice(&raw);
    }

    /// Write a 32-bit header field in the requested byte order.
    fn write_u32(buf: &mut [u8], offset: usize, value: u32, big_endian: bool) {
        let raw = if big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        };
        buf[offset..offset + 4].copy_from_slice(&raw);
    }

    /// R0001-0066: well-formed fixed ELF header — `e_ident`, plus
    /// `e_type` / `e_version` / `e_ehsize` written in the byte order
    /// `EI_DATA` declares. Program headers and sections (which goblin
    /// used to chase out of bounds) are absent entirely.
    fn elf_header(class: u8, data: u8, e_type: u16) -> Vec<u8> {
        let big_endian = data == 2; // ELFDATA2MSB
        let ehsize: u16 = if class == 1 { 52 } else { 64 };
        let mut header = vec![0u8; ehsize as usize];
        header[..4].copy_from_slice(b"\x7fELF");
        header[4] = class;
        header[5] = data;
        header[6] = 1; // EI_VERSION = EV_CURRENT
        write_u16(&mut header, 0x10, e_type, big_endian);
        write_u32(&mut header, 0x14, 1, big_endian); // e_version
        let ehsize_offset = if class == 1 { 0x28 } else { 0x34 };
        write_u16(&mut header, ehsize_offset, ehsize, big_endian);
        header
    }

    /// Minimal ELF stub header: 64-bit class, little-endian data,
    /// ET_EXEC image.
    fn minimal_elf_header() -> Vec<u8> {
        elf_header(2, 1, 2)
    }

    /// R0001-0067: well-formed thin Mach-O header padded to the 4 KiB
    /// probe window `detect` is documented to receive. Byte order and
    /// header size follow the magic.
    fn macho_header(magic: [u8; 4], filetype: u32, ncmds: u32, sizeofcmds: u32) -> Vec<u8> {
        let big_endian = magic[0] == 0xFE; // MH_MAGIC / MH_MAGIC_64
        let mut data = vec![0u8; 4096];
        data[..4].copy_from_slice(&magic);
        write_u32(&mut data, 0x0C, filetype, big_endian);
        write_u32(&mut data, 0x10, ncmds, big_endian);
        write_u32(&mut data, 0x14, sizeofcmds, big_endian);
        data
    }

    /// The four thin magics, in file-byte order.
    const THIN_MACHO_MAGICS: [[u8; 4]; 4] = [
        [0xFE, 0xED, 0xFA, 0xCE],
        [0xCE, 0xFA, 0xED, 0xFE],
        [0xFE, 0xED, 0xFA, 0xCF],
        [0xCF, 0xFA, 0xED, 0xFE],
    ];

    #[test]
    fn test_detect_minimal_pe_header() {
        assert_eq!(StubType::detect(&minimal_pe_header()), StubType::WindowsPE);
    }

    #[test]
    fn test_detect_pe_header_with_dos_stub_padding() {
        // A real SFX stub has megabytes of sections after the headers;
        // classification must succeed from the prefix alone even when
        // the buffer is just the 4 KiB window.
        let mut data = minimal_pe_header();
        data.resize(4096, 0);
        assert_eq!(StubType::detect(&data), StubType::WindowsPE);
    }

    #[test]
    fn test_detect_mz_without_pe_signature_is_unknown() {
        // `MZ` with an e_lfanew that does not reach a `PE\0\0` magic is
        // a plain DOS executable (or noise), not a PE stub.
        let mut data = vec![0u8; 0x48];
        data[0] = b'M';
        data[1] = b'Z';
        data[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        // bytes at 0x40 stay zero — no COFF signature
        assert_eq!(StubType::detect(&data), StubType::Unknown);
    }

    #[test]
    fn test_detect_pe_e_lfanew_beyond_buffer_is_unknown() {
        let mut data = vec![0u8; 0x48];
        data[0] = b'M';
        data[1] = b'Z';
        data[0x3C..0x40].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        assert_eq!(StubType::detect(&data), StubType::Unknown);
    }

    #[test]
    fn test_detect_minimal_elf_header() {
        assert_eq!(StubType::detect(&minimal_elf_header()), StubType::LinuxELF);
    }

    #[test]
    fn test_detect_elf_header_with_padding() {
        let mut data = minimal_elf_header();
        data.resize(4096, 0);
        assert_eq!(StubType::detect(&data), StubType::LinuxELF);
    }

    #[test]
    fn test_detect_macho_thin_magics() {
        // R0001-0067: a plausible header (MH_EXECUTE, 20 load commands
        // spanning 1712 bytes — the shape `otool -h` reports for a real
        // system binary) classifies for every magic/byte-order pair.
        for magic in THIN_MACHO_MAGICS {
            let data = macho_header(magic, 2, 20, 1712);
            assert_eq!(
                StubType::detect(&data),
                StubType::MacOSMachO,
                "magic {:02x?} should classify as Mach-O",
                magic
            );
        }
    }

    #[test]
    fn test_detect_macho_fat_vs_java_class() {
        // Universal binary: FAT_MAGIC + nfat_arch = 2 (big-endian).
        let mut fat = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x02];
        fat.resize(64, 0);
        assert_eq!(StubType::detect(&fat), StubType::MacOSMachO);

        // Java class file: same magic, then minor=0/major=52 — the
        // implausible "architecture count" must not classify as Mach-O.
        let mut class = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34];
        class.resize(64, 0);
        assert_eq!(StubType::detect(&class), StubType::Unknown);
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

    // ============================================================
    // R0001-0066 / R0001-0067: the fixed ELF and Mach-O headers are
    // validated, not just the magic. Well-formed headers still
    // classify; truncated or implausible ones fall back to Unknown
    // (which detection.rs turns into not_sfx).
    // ============================================================

    #[test]
    fn test_detect_elf_all_class_and_byte_order_combinations() {
        // ELFCLASS32/64 x ELFDATA2LSB/MSB, ET_EXEC and ET_DYN.
        for class in [1u8, 2] {
            for data in [1u8, 2] {
                for e_type in [2u16, 3] {
                    let header = elf_header(class, data, e_type);
                    assert_eq!(
                        StubType::detect(&header),
                        StubType::LinuxELF,
                        "class {class} data {data} e_type {e_type} should classify as ELF"
                    );
                }
            }
        }
    }

    #[test]
    fn test_detect_elf_rejects_non_executable_e_type() {
        // R0001-0066: ET_NONE(0), ET_REL(1) and ET_CORE(4) are not
        // executable images, so they cannot be an SFX stub.
        for e_type in [0u16, 1, 4, 0xFFFF] {
            let header = elf_header(2, 1, e_type);
            assert_eq!(
                StubType::detect(&header),
                StubType::Unknown,
                "e_type {e_type} must not classify as ELF"
            );
        }
    }

    #[test]
    fn test_detect_elf_rejects_bad_e_version() {
        // e_version (0x14) must be EV_CURRENT.
        let mut header = minimal_elf_header();
        write_u32(&mut header, 0x14, 0, false);
        assert_eq!(StubType::detect(&header), StubType::Unknown);
    }

    #[test]
    fn test_detect_elf_rejects_wrong_e_ehsize() {
        // A 64-bit header claiming the 32-bit header size is malformed.
        let mut header = minimal_elf_header();
        write_u16(&mut header, 0x34, 52, false);
        assert_eq!(StubType::detect(&header), StubType::Unknown);

        // ...and vice versa for ELFCLASS32.
        let mut header32 = elf_header(1, 1, 2);
        write_u16(&mut header32, 0x28, 64, false);
        assert_eq!(StubType::detect(&header32), StubType::Unknown);
    }

    #[test]
    fn test_detect_elf_rejects_truncated_fixed_header() {
        // R0001-0066: e_ident alone (16 bytes) used to be enough; the
        // whole fixed header must be inside the probe window now.
        let header = minimal_elf_header();
        for truncated in [7usize, 16, 32, 63] {
            assert_eq!(
                StubType::detect(&header[..truncated]),
                StubType::Unknown,
                "{truncated}-byte ELF prefix must not classify as ELF"
            );
        }
        // The 32-bit header is complete at 52 bytes, so a 52-byte
        // prefix of it still classifies.
        let header32 = elf_header(1, 1, 2);
        assert_eq!(StubType::detect(&header32), StubType::LinuxELF);
        assert_eq!(StubType::detect(&header32[..51]), StubType::Unknown);
    }

    #[test]
    fn test_detect_elf_big_endian_fields_respect_ei_data() {
        // A big-endian header whose fields were written little-endian
        // is garbage to a big-endian reader and must be rejected.
        let mut header = elf_header(2, 2, 2);
        write_u16(&mut header, 0x10, 2, false); // e_type, wrong order
        assert_eq!(StubType::detect(&header), StubType::Unknown);
    }

    #[test]
    fn test_detect_macho_accepts_dylib_filetype() {
        // MH_DYLIB (6) is still an executable image.
        let data = macho_header(THIN_MACHO_MAGICS[3], 6, 19, 1664);
        assert_eq!(StubType::detect(&data), StubType::MacOSMachO);
    }

    #[test]
    fn test_detect_macho_rejects_magic_only_prefix() {
        // R0001-0067: four magic bytes (and any prefix shorter than the
        // fixed header) no longer mark data executable.
        for magic in THIN_MACHO_MAGICS {
            for len in [4usize, 8, 16, 27] {
                let mut data = magic.to_vec();
                data.resize(len, 0);
                assert_eq!(
                    StubType::detect(&data),
                    StubType::Unknown,
                    "magic {magic:02x?} truncated to {len} bytes must not classify as Mach-O"
                );
            }
        }
    }

    #[test]
    fn test_detect_macho_rejects_implausible_header_fields() {
        let magic = THIN_MACHO_MAGICS[3]; // MH_CIGAM_64

        // filetype 0 / MH_OBJECT(1) — not an executable image.
        for filetype in [0u32, 1, 0xFFFF_FFFF] {
            let data = macho_header(magic, filetype, 20, 1712);
            assert_eq!(
                StubType::detect(&data),
                StubType::Unknown,
                "filetype {filetype} must not classify as Mach-O"
            );
        }

        // ncmds must be nonzero and small enough that every load
        // command still gets its 8-byte minimum out of sizeofcmds.
        let no_commands = macho_header(magic, 2, 0, 1712);
        assert_eq!(StubType::detect(&no_commands), StubType::Unknown);
        let absurd_ncmds = macho_header(magic, 2, 0xFFFF_FFFF, 1712);
        assert_eq!(StubType::detect(&absurd_ncmds), StubType::Unknown);
        let too_many_for_size = macho_header(magic, 2, 300, 1712);
        assert_eq!(StubType::detect(&too_many_for_size), StubType::Unknown);

        // sizeofcmds must fit in the probe window behind the header.
        let overlong_cmds = macho_header(magic, 2, 20, 0xFFFF_FFF0);
        assert_eq!(StubType::detect(&overlong_cmds), StubType::Unknown);
        let just_past_window = macho_header(magic, 2, 20, 4096 - 32 + 1);
        assert_eq!(StubType::detect(&just_past_window), StubType::Unknown);
    }

    #[test]
    fn test_detect_macho_byte_order_follows_magic() {
        // Big-endian magic with little-endian fields is malformed.
        let mut data = macho_header(THIN_MACHO_MAGICS[2], 2, 20, 1712); // MH_MAGIC_64
        write_u32(&mut data, 0x0C, 2, false); // filetype in the wrong order
        assert_eq!(StubType::detect(&data), StubType::Unknown);
    }

    #[test]
    fn test_is_native_elf_on_bsd() {
        // R0001-0068: `LinuxELF` is described as "Linux/BSD ELF
        // executable", so BSD hosts must accept it as native.
        #[cfg(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            assert!(StubType::LinuxELF.is_native());
            assert!(StubType::ScriptInterpreter.is_native());
            assert!(!StubType::WindowsPE.is_native());
            assert!(!StubType::MacOSMachO.is_native());
        }
        // The description must keep naming BSD wherever this runs —
        // it is the claim `is_native` was made to agree with.
        assert!(StubType::LinuxELF.description().contains("BSD"));
    }
}
