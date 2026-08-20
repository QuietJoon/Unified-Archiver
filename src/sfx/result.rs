// SFX detection result structure
//
// Contains the result of SFX detection including stub type, archive format, and offset

use super::stub_types::StubType;
use crate::format::ArchiveFormat;

/// Tri-state SFX detection confidence (I3).
///
/// Replaces the former `f32` confidence score. The staged detector
/// (arch AD 0006) produces exactly two of these states in production —
/// [`NotSfx`](Self::NotSfx) and [`Probable`](Self::Probable) — which the
/// float used to encode as `0.0` and a clamped `~0.9`. No value between
/// those was ever produced, so the float carried no information while
/// forcing callers to compare against magic thresholds (this is what
/// retired the clamp arithmetic of MADR-0014). The enum names the reachable
/// states directly and keeps [`Confirmed`](Self::Confirmed) as a
/// first-class target for the still-deferred confirmed-detection patterns
/// (MADR-0015 / OI-0080-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxConfidence {
    /// Not a self-extracting archive: no recognised executable stub was
    /// present, or no archive signature passed Stage-3 structural
    /// screening.
    NotSfx,
    /// A recognised executable stub plus an archive signature that passed
    /// the Stage-3 structural probe at the detected offset. The embedded
    /// archive was **not** opened or validated — treat this as a strong
    /// hint, not proof. This is the strongest verdict the live detector
    /// currently produces.
    Probable,
    /// The embedded archive was opened and validated at the detected
    /// offset. The production [`detect_sfx`](super::detection::detect_sfx)
    /// never returns this today — the confirmed-detection patterns of
    /// MADR-0015 remain deferred — so this variant is reachable only from the
    /// crate-internal, test-only `detected` constructor (not linked: it is
    /// `#[cfg(test)]`, so the target does not exist in a rustdoc build).
    /// It exists so a future confirmed path (and unit tests) have a target.
    Confirmed,
}

/// Result of SFX (Self-Extracting Archive) detection.
///
/// Constructed by [`SfxDetectionResult::not_sfx`] /
/// [`SfxDetectionResult::probable`]. Production
/// [`detect_sfx`](super::detection::detect_sfx) only returns those two
/// states; an internal test-only `detected` constructor produces a
/// [`Confirmed`](SfxConfidence::Confirmed) result for unit tests
/// (R0069-0076).
///
/// **Construction (R0070-0077).** The struct is `#[non_exhaustive]`,
/// so external crates can no longer fabricate a contradictory state
/// (`is_sfx=false` with `archive_format=Some(_)`, a
/// [`Confirmed`](SfxConfidence::Confirmed) confidence outside the
/// test-only path, etc.). New construction goes through
/// [`SfxDetectionResult::not_sfx`] / [`SfxDetectionResult::probable`].
///
/// **Field access (R0075-0071).** The state-bearing fields are
/// `pub(crate)` and reachable from outside the crate only via the
/// dedicated accessor methods ([`is_sfx`](Self::is_sfx),
/// [`archive_format`](Self::archive_format),
/// [`data_offset`](Self::data_offset),
/// [`stub_type`](Self::stub_type), [`confidence`](Self::confidence),
/// [`evidence`](Self::evidence)). This stops downstream code from
/// depending on the field shape and frees the crate to reorganise the
/// struct without a breaking change.
///
/// **`is_confirmed()` semantics (R0070-0078, I3).** Today the production
/// pipeline never returns a [`Confirmed`](SfxConfidence::Confirmed)
/// result — the `is_confirmed()` helper exists for symmetry with the
/// [`SfxConfidence`] tri-state and for unit tests that drive the
/// test-only confirmed constructor. Callers that need a guaranteed "this
/// is a real archive" answer must follow [`crate::Archive::detect_sfx`]
/// with [`crate::Archive::open_sfx`] and treat any archive-error from the
/// open as a refused confirmation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SfxDetectionResult {
    /// Whether the file is a self-extracting archive
    pub(crate) is_sfx: bool,

    /// Format of the embedded archive (if SFX detected)
    pub(crate) archive_format: Option<ArchiveFormat>,

    /// Byte offset where archive data begins (if SFX detected)
    pub(crate) data_offset: Option<u64>,

    /// Type of executable stub (if SFX detected)
    pub(crate) stub_type: Option<StubType>,

    /// Tri-state detection confidence (I3, replaces the former f32
    /// score). See [`SfxConfidence`] for the meaning of each variant.
    pub(crate) confidence: SfxConfidence,

    /// Human-readable evidence naming *why* the result reached its
    /// confidence — the matched stub kind, the archive signature and
    /// offset, and any structural-probe notes. Empty for
    /// [`not_sfx`](Self::not_sfx) results.
    pub(crate) evidence: Vec<String>,

    /// Identity of the file as seen *through the detection open itself*
    /// (R0001-0002, completing OI-0081-001). The staging call sites used
    /// to re-`stat` the pathname after detection had already closed its
    /// handle, so a replacement dropped in that window became the trusted
    /// baseline; carrying the detection handle's own identity here closes
    /// that gap. Populated only by the production detector when it
    /// returns payload coordinates — `None` for
    /// [`not_sfx`](Self::not_sfx) results and for results built by
    /// external callers through the public constructors, which have no
    /// detection open to bind to.
    pub(crate) source_identity: Option<crate::archive::ReadFileIdentity>,
}

impl SfxDetectionResult {
    /// Create a result indicating the file is not an SFX archive
    pub fn not_sfx() -> Self {
        Self {
            is_sfx: false,
            archive_format: None,
            data_offset: None,
            stub_type: None,
            confidence: SfxConfidence::NotSfx,
            evidence: Vec::new(),
            source_identity: None,
        }
    }

    /// Test-only constructor for a confirmed detection (R0069-0076).
    /// Crate-internal `#[cfg(test)]` because the production pipeline
    /// never produces a [`Confirmed`](SfxConfidence::Confirmed) result —
    /// see the struct rustdoc.
    #[cfg(test)]
    pub(crate) fn detected(
        stub_type: StubType,
        archive_format: ArchiveFormat,
        data_offset: u64,
    ) -> Self {
        Self {
            is_sfx: true,
            archive_format: Some(archive_format),
            data_offset: Some(data_offset),
            stub_type: Some(stub_type),
            confidence: SfxConfidence::Confirmed,
            evidence: vec![format!(
                "confirmed: {} stub with validated {:?} payload at offset {}",
                stub_type.description(),
                archive_format,
                data_offset
            )],
            source_identity: None,
        }
    }

    /// Create a result for a probable SFX (signature found and structurally
    /// screened, but the embedded archive was not opened/validated).
    ///
    /// The confidence is fixed at [`SfxConfidence::Probable`] (I3 — there is
    /// no longer a float to clamp; the retired MADR-0014 clamp only existed to
    /// keep the float from colliding with the confirmed value). `evidence`
    /// records why the candidate passed — the matched stub kind, the archive
    /// signature, and its offset. Callers that genuinely have no signal
    /// should use [`SfxDetectionResult::not_sfx`] instead.
    pub fn probable(
        stub_type: StubType,
        archive_format: ArchiveFormat,
        data_offset: u64,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            is_sfx: true,
            archive_format: Some(archive_format),
            data_offset: Some(data_offset),
            stub_type: Some(stub_type),
            confidence: SfxConfidence::Probable,
            evidence,
            source_identity: None,
        }
    }

    /// Bind this result to the identity of the file the detector actually
    /// read, captured from the detection open's own descriptor
    /// (R0001-0002). Crate-internal: only
    /// [`detect_sfx`](super::detection::detect_sfx) holds that handle, and
    /// the staging call sites consume the value through
    /// [`source_identity`](Self::source_identity) instead of re-`stat`ing
    /// the pathname.
    pub(crate) fn with_source_identity(
        mut self,
        identity: crate::archive::ReadFileIdentity,
    ) -> Self {
        self.source_identity = Some(identity);
        self
    }

    /// Identity captured at the detection open, when this result came from
    /// a real detection of a payload-bearing file (R0001-0002). `None` for
    /// non-SFX results and for results fabricated through the public
    /// constructors; callers that stage bytes must fail closed on `None`
    /// rather than fall back to a fresh `stat`.
    pub(crate) fn source_identity(&self) -> Option<crate::archive::ReadFileIdentity> {
        self.source_identity
    }

    /// Whether the file was detected as a self-extracting archive
    /// (R0075-0071).
    pub fn is_sfx(&self) -> bool {
        self.is_sfx
    }

    /// Detected archive format of the embedded payload, when known
    /// (R0075-0071). `None` for non-SFX results.
    pub fn archive_format(&self) -> Option<ArchiveFormat> {
        self.archive_format
    }

    /// Byte offset where the embedded archive begins, when known
    /// (R0075-0071). `None` for non-SFX results.
    pub fn data_offset(&self) -> Option<u64> {
        self.data_offset
    }

    /// Detected executable-stub kind, when known (R0075-0071). `None`
    /// for non-SFX results.
    pub fn stub_type(&self) -> Option<StubType> {
        self.stub_type
    }

    /// Tri-state detection confidence (R0075-0071, I3). See
    /// [`SfxConfidence`] for the meaning of each variant.
    pub fn confidence(&self) -> SfxConfidence {
        self.confidence
    }

    /// Convenience boolean: `true` when the result is a probable
    /// (signature-screened but unvalidated) SFX. Equivalent to
    /// `confidence() == SfxConfidence::Probable`.
    pub fn is_probable(&self) -> bool {
        matches!(self.confidence, SfxConfidence::Probable)
    }

    /// Convenience boolean: `true` when the embedded archive was opened
    /// and validated (`confidence() == SfxConfidence::Confirmed`).
    ///
    /// I3: this is now a direct match on the [`SfxConfidence`] tri-state
    /// rather than the former exact-float check on `confidence == 1.0`.
    /// The live detector never returns a confirmed result — see the
    /// struct rustdoc and [`SfxConfidence::Confirmed`].
    pub fn is_confirmed(&self) -> bool {
        matches!(self.confidence, SfxConfidence::Confirmed)
    }

    /// Human-readable evidence naming why the result reached its
    /// [`confidence`](Self::confidence) — matched stub kind, archive
    /// signature, and offset. Empty for non-SFX results.
    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }

    /// Return the SFX payload coordinates as `Some(format, offset, stub)`
    /// when [`is_sfx`](Self::is_sfx) is true and the optional fields are
    /// populated. Returns `None` for non-SFX results or for results
    /// missing one of the required fields.
    ///
    /// Use this in preference to manually unwrapping `archive_format`,
    /// `data_offset`, and `stub_type` after checking `is_sfx`. The
    /// helper packages the three together so callers can pattern-match
    /// a single `Option` rather than coupling three field-level
    /// `unwrap()`s.
    pub fn payload_coordinates(&self) -> Option<(ArchiveFormat, u64, StubType)> {
        if !self.is_sfx {
            return None;
        }
        Some((self.archive_format?, self.data_offset?, self.stub_type?))
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

        let confidence_label = if self.is_confirmed() {
            "confirmed"
        } else {
            "probable"
        };

        format!(
            "SFX detected ({}): {} stub, {} archive at offset {}",
            confidence_label, stub, format, offset
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
        assert_eq!(result.confidence, SfxConfidence::NotSfx);
        assert!(result.archive_format.is_none());
        assert!(result.evidence.is_empty());
    }

    #[test]
    fn test_detected_sfx() {
        let result = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 1024);
        assert!(result.is_sfx);
        assert_eq!(result.confidence, SfxConfidence::Confirmed);
        assert!(result.is_confirmed());
        assert_eq!(result.data_offset, Some(1024));
    }

    #[test]
    fn test_payload_coordinates_populated_and_none() {
        // Populated SFX result: the accessor packages all three fields.
        let result = SfxDetectionResult::probable(
            StubType::LinuxELF,
            ArchiveFormat::Xz,
            4096,
            vec!["signature Xz at offset 4096".to_string()],
        );
        assert_eq!(
            result.payload_coordinates(),
            Some((ArchiveFormat::Xz, 4096, StubType::LinuxELF))
        );

        // Non-SFX result: short-circuits to None before field access.
        assert_eq!(SfxDetectionResult::not_sfx().payload_coordinates(), None);
    }

    #[test]
    fn test_probable_sfx() {
        let result = SfxDetectionResult::probable(
            StubType::LinuxELF,
            ArchiveFormat::SevenZip,
            2048,
            vec!["signature 7z at offset 2048".to_string()],
        );
        assert!(result.is_sfx);
        assert_eq!(result.confidence, SfxConfidence::Probable);
        assert!(result.is_probable());
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
        assert!(summary.contains("confirmed"));
        assert!(summary.contains("Windows PE executable"));
        assert!(summary.contains("Zip"));
        assert!(summary.contains("1024"));
    }

    #[test]
    fn test_summary_probable() {
        let result = SfxDetectionResult::probable(
            StubType::ScriptInterpreter,
            ArchiveFormat::Rar,
            512,
            Vec::new(),
        );
        let summary = result.summary();
        assert!(summary.contains("SFX detected"));
        assert!(summary.contains("probable"));
        assert!(!summary.contains("confirmed"));
        assert!(summary.contains("Script interpreter"));
        assert!(summary.contains("Rar"));
    }

    #[test]
    fn test_default() {
        let result = SfxDetectionResult::default();
        assert!(!result.is_sfx);
        assert_eq!(result.confidence, SfxConfidence::NotSfx);
        assert!(result.archive_format.is_none());
        assert!(result.data_offset.is_none());
        assert!(result.stub_type.is_none());
    }

    #[test]
    fn test_confidence_enum_states() {
        // I3: the three confidence states map onto the three constructors.
        assert_eq!(
            SfxDetectionResult::not_sfx().confidence(),
            SfxConfidence::NotSfx
        );
        assert_eq!(
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, Vec::new())
                .confidence(),
            SfxConfidence::Probable
        );
        assert_eq!(
            SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 100).confidence(),
            SfxConfidence::Confirmed
        );
    }

    #[test]
    fn test_probable_is_never_confirmed() {
        // I3: probable() can only ever yield SfxConfidence::Probable;
        // Confirmed is reachable exclusively through detected().
        let probable =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 100, Vec::new());
        assert!(probable.is_probable());
        assert!(!probable.is_confirmed());

        let detected = SfxDetectionResult::detected(StubType::WindowsPE, ArchiveFormat::Zip, 100);
        assert!(detected.is_confirmed());
        assert!(!detected.is_probable());
    }

    #[test]
    fn test_source_identity_unset_by_public_constructors() {
        // R0001-0002: only the crate-internal detector, which holds the
        // open handle, can bind a result to a source identity. Results
        // built through the public constructors stay unbound so the
        // staging call sites fail closed instead of trusting a fresh stat.
        assert!(SfxDetectionResult::not_sfx().source_identity().is_none());
        let probable =
            SfxDetectionResult::probable(StubType::WindowsPE, ArchiveFormat::Zip, 64, Vec::new());
        assert!(probable.source_identity().is_none());

        let temp = tempfile::NamedTempFile::new().unwrap();
        let identity = crate::archive::read_file_identity(&std::fs::metadata(temp.path()).unwrap());
        assert_eq!(
            probable.with_source_identity(identity).source_identity(),
            Some(identity),
            "the crate-internal builder must round-trip the detection identity"
        );
    }

    #[test]
    fn test_evidence_accessor() {
        // Evidence passed to probable() is exposed verbatim.
        let evidence = vec![
            "executable stub: Windows PE executable".to_string(),
            "Zip signature matched Stage-3 structural probe at offset 4096".to_string(),
        ];
        let result = SfxDetectionResult::probable(
            StubType::WindowsPE,
            ArchiveFormat::Zip,
            4096,
            evidence.clone(),
        );
        assert_eq!(result.evidence(), evidence.as_slice());

        // not_sfx() carries no evidence.
        assert!(SfxDetectionResult::not_sfx().evidence().is_empty());
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
            StubType::ScriptInterpreter,
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
        let result =
            SfxDetectionResult::detected(StubType::ScriptInterpreter, ArchiveFormat::Zip, 0);
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
        assert_eq!(result.confidence, SfxConfidence::NotSfx);
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
        assert_eq!(original.evidence, cloned.evidence);
    }

    #[test]
    fn test_debug_format() {
        // Verify Debug trait produces readable output
        let result =
            SfxDetectionResult::detected(StubType::ScriptInterpreter, ArchiveFormat::Rar, 512);
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
        assert_eq!(result.confidence, SfxConfidence::Confirmed);
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
