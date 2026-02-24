// SFX detection result structure
//
// Contains the result of SFX detection including stub type, archive format, and offset

use super::stub_types::StubType;
use crate::format::ArchiveFormat;

/// Result of SFX (Self-Extracting Archive) detection
#[derive(Debug, Clone)]
pub struct SfxDetectionResult {
    /// Whether the file is a self-extracting archive
    pub is_sfx: bool,

    /// Format of the embedded archive (if SFX detected)
    pub archive_format: Option<ArchiveFormat>,

    /// Byte offset where archive data begins (if SFX detected)
    pub data_offset: Option<u64>,

    /// Type of executable stub (if SFX detected)
    pub stub_type: Option<StubType>,

    /// Confidence score for detection (0.0-1.0)
    /// 1.0 = confirmed SFX with validated archive
    /// 0.5-0.99 = probable SFX (signature found but validation incomplete)
    /// 0.0 = not an SFX file
    pub confidence: f32,
}

impl SfxDetectionResult {
    /// Create a result indicating the file is not an SFX archive
    pub fn not_sfx() -> Self {
        Self {
            is_sfx: false,
            archive_format: None,
            data_offset: None,
            stub_type: None,
            confidence: 0.0,
        }
    }

    /// Create a result for a successfully detected SFX archive
    pub fn detected(stub_type: StubType, archive_format: ArchiveFormat, data_offset: u64) -> Self {
        Self {
            is_sfx: true,
            archive_format: Some(archive_format),
            data_offset: Some(data_offset),
            stub_type: Some(stub_type),
            confidence: 1.0,
        }
    }

    /// Create a result for a probable SFX (signature found but not fully validated)
    pub fn probable(
        stub_type: StubType,
        archive_format: ArchiveFormat,
        data_offset: u64,
        confidence: f32,
    ) -> Self {
        Self {
            is_sfx: true,
            archive_format: Some(archive_format),
            data_offset: Some(data_offset),
            stub_type: Some(stub_type),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// Check if detection result is confirmed (confidence == 1.0)
    pub fn is_confirmed(&self) -> bool {
        self.confidence >= 0.99
    }

    /// Get a human-readable summary of the detection result
    pub fn summary(&self) -> String {
        if !self.is_sfx {
            return "Not a self-extracting archive".to_string();
        }

        let stub = self.stub_type.map(|s| s.description()).unwrap_or("Unknown");
        let format = self
            .archive_format
            .map(|f| format!("{:?}", f))
            .unwrap_or_else(|| "Unknown".to_string());
        let offset = self
            .data_offset
            .map(|o| format!("{}", o))
            .unwrap_or_else(|| "Unknown".to_string());

        format!(
            "SFX detected: {} stub, {} archive at offset {}",
            stub, format, offset
        )
    }
}

impl Default for SfxDetectionResult {
    fn default() -> Self {
        Self::not_sfx()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_sfx() {
        let result = SfxDetectionResult::not_sfx();
        assert!(!result.is_sfx);
        assert_eq!(result.confidence, 0.0);
        assert!(result.archive_format.is_none());
    }

    #[test]
    fn test_detected_sfx() {
        let result = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 1024);
        assert!(result.is_sfx);
        assert_eq!(result.confidence, 1.0);
        assert!(result.is_confirmed());
        assert_eq!(result.data_offset, Some(1024));
    }

    #[test]
    fn test_probable_sfx() {
        let result =
            SfxDetectionResult::probable(StubType::LinuxELF, ArchiveFormat::SevenZip, 2048, 0.8);
        assert!(result.is_sfx);
        assert_eq!(result.confidence, 0.8);
        assert!(!result.is_confirmed());
    }

    #[test]
    fn test_summary_not_sfx() {
        let result = SfxDetectionResult::not_sfx();
        assert_eq!(result.summary(), "Not a self-extracting archive");
    }

    #[test]
    fn test_summary_detected() {
        let result = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 1024);
        let summary = result.summary();
        assert!(summary.contains("SFX detected"));
        assert!(summary.contains("Windows PE executable"));
        assert!(summary.contains("Zip"));
        assert!(summary.contains("1024"));
    }

    #[test]
    fn test_summary_probable() {
        let result =
            SfxDetectionResult::probable(StubType::ShellScript, ArchiveFormat::Rar, 512, 0.75);
        let summary = result.summary();
        assert!(summary.contains("SFX detected"));
        assert!(summary.contains("Unix shell script"));
        assert!(summary.contains("Rar"));
    }

    #[test]
    fn test_default() {
        let result = SfxDetectionResult::default();
        assert!(!result.is_sfx);
        assert_eq!(result.confidence, 0.0);
        assert!(result.archive_format.is_none());
        assert!(result.data_offset.is_none());
        assert!(result.stub_type.is_none());
    }

    #[test]
    fn test_confidence_clamping() {
        // Test that confidence is clamped to 0.0-1.0 range
        let result_high =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, 1.5);
        assert_eq!(result_high.confidence, 1.0);

        let result_low =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, -0.5);
        assert_eq!(result_low.confidence, 0.0);
    }

    #[test]
    fn test_is_confirmed_boundary() {
        // Test boundary case for is_confirmed (>= 0.99)
        let result_99 =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, 0.99);
        assert!(result_99.is_confirmed());

        let result_98 =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, 0.98);
        assert!(!result_98.is_confirmed());
    }

    // ============================================================
    // T110e: Additional unit tests for result.rs constructors/helpers
    // ============================================================

    #[test]
    fn test_detected_all_stub_types() {
        // Test detected() with all stub types
        let stub_types = [
            StubType::WindowsPE,
            StubType::LinuxELF,
            StubType::MacOSMachO,
            StubType::ShellScript,
        ];

        for stub in stub_types {
            let result = SfxDetectionResult::detected(stub, ArchiveFormat::Zip, 1024);
            assert!(result.is_sfx);
            assert!(result.is_confirmed());
            assert_eq!(result.stub_type, Some(stub));
        }
    }

    #[test]
    fn test_detected_all_archive_formats() {
        // Test detected() with various archive formats
        let formats = [
            ArchiveFormat::Zip,
            ArchiveFormat::SevenZip,
            ArchiveFormat::Rar,
            ArchiveFormat::Rar5,
        ];

        for format in formats {
            let result = SfxDetectionResult::detected(StubType::WindowsPE, format, 2048);
            assert_eq!(result.archive_format, Some(format));
        }
    }

    #[test]
    fn test_detected_with_zero_offset() {
        // Edge case: archive at offset 0
        let result = SfxDetectionResult::detected(StubType::ShellScript, ArchiveFormat::Zip, 0);
        assert!(result.is_sfx);
        assert_eq!(result.data_offset, Some(0));
    }

    #[test]
    fn test_detected_with_large_offset() {
        // Large offset (beyond 1MB scan limit)
        let large_offset = 10_000_000u64;
        let result = SfxDetectionResult::detected(
            StubType::WindowsPE,
            ArchiveFormat::SevenZip,
            large_offset,
        );
        assert_eq!(result.data_offset, Some(large_offset));
    }

    #[test]
    fn test_probable_confidence_zero() {
        // Zero confidence
        let result = SfxDetectionResult::probable(StubType::LinuxELF, ArchiveFormat::Zip, 500, 0.0);
        assert!(result.is_sfx); // Still marked as SFX even with zero confidence
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_probable_confidence_one() {
        // Exactly 1.0 confidence
        let result = SfxDetectionResult::probable(StubType::LinuxELF, ArchiveFormat::Zip, 500, 1.0);
        assert!(result.is_confirmed());
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_probable_confidence_mid_range() {
        // Various mid-range confidences
        for conf in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let result =
                SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, conf);
            assert_eq!(result.confidence, conf);
        }
    }

    #[test]
    fn test_summary_all_combinations() {
        // Test summary for different stub+format combinations
        let result1 =
            SfxDetectionResult::detected(StubType::LinuxELF, ArchiveFormat::SevenZip, 4096);
        let summary1 = result1.summary();
        assert!(summary1.contains("Linux/BSD ELF"));
        assert!(summary1.contains("SevenZip"));
        assert!(summary1.contains("4096"));

        let result2 = SfxDetectionResult::detected(StubType::MacOSMachO, ArchiveFormat::Rar5, 8192);
        let summary2 = result2.summary();
        assert!(summary2.contains("macOS Mach-O"));
        assert!(summary2.contains("Rar5"));
    }

    #[test]
    fn test_not_sfx_all_fields_none() {
        // Verify not_sfx() sets all optional fields to None
        let result = SfxDetectionResult::not_sfx();
        assert!(!result.is_sfx);
        assert!(result.archive_format.is_none());
        assert!(result.data_offset.is_none());
        assert!(result.stub_type.is_none());
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_clone_preserves_all_fields() {
        // Verify Clone works correctly
        let original = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 1234);
        let cloned = original.clone();

        assert_eq!(original.is_sfx, cloned.is_sfx);
        assert_eq!(original.archive_format, cloned.archive_format);
        assert_eq!(original.data_offset, cloned.data_offset);
        assert_eq!(original.stub_type, cloned.stub_type);
        assert_eq!(original.confidence, cloned.confidence);
    }

    #[test]
    fn test_debug_format() {
        // Verify Debug trait produces readable output
        let result = SfxDetectionResult::detected(StubType::ShellScript, ArchiveFormat::Rar, 512);
        let debug_str = format!("{:?}", result);

        assert!(debug_str.contains("SfxDetectionResult"));
        assert!(debug_str.contains("is_sfx"));
        assert!(debug_str.contains("true"));
    }

    #[test]
    fn test_is_confirmed_with_detected() {
        // detected() should always be confirmed
        let result = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 100);
        assert!(result.is_confirmed());
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_is_confirmed_with_not_sfx() {
        // not_sfx() should never be confirmed
        let result = SfxDetectionResult::not_sfx();
        assert!(!result.is_confirmed());
    }

    #[test]
    fn test_default_matches_not_sfx() {
        // Default::default() should be equivalent to not_sfx()
        let default_result = SfxDetectionResult::default();
        let not_sfx_result = SfxDetectionResult::not_sfx();

        assert_eq!(default_result.is_sfx, not_sfx_result.is_sfx);
        assert_eq!(default_result.archive_format, not_sfx_result.archive_format);
        assert_eq!(default_result.data_offset, not_sfx_result.data_offset);
        assert_eq!(default_result.stub_type, not_sfx_result.stub_type);
        assert_eq!(default_result.confidence, not_sfx_result.confidence);
    }

    #[test]
    fn test_summary_does_not_panic_on_none_fields() {
        // summary() with various None fields shouldn't panic
        let mut result = SfxDetectionResult::not_sfx();
        let _ = result.summary(); // Should not panic

        // Manually set some fields to create edge cases
        result.is_sfx = true;
        result.stub_type = None;
        result.archive_format = None;
        result.data_offset = None;
        let summary = result.summary();
        assert!(summary.contains("Unknown")); // Should use fallback text
    }
}
