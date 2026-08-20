//! Identification of the external `rar` binary: which program is it, and
//! is its version usable?
//!
//! # Why "not found" and "too old" must be different answers
//!
//! A caller that reacts to a missing tool installs it; a caller that
//! reacts to an old tool upgrades it; a caller that discovers it invoked
//! `unrar` by mistake fixes its configuration. Those are three different
//! remediations, so this module produces three different values and the
//! error module gives each its own variant — no string sniffing.
//!
//! # How the probe works
//!
//! Every RAR-family console binary, from 3.x onward, prints an
//! identifying banner as the first line of its help output when invoked
//! with no arguments:
//!
//! ```text
//! RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal   1 Oct 2023
//! UNRAR 6.24 freeware      Copyright (c) 1993-2023 Alexander Roshal
//! ```
//!
//! The probe therefore runs the binary with an empty argument vector and
//! parses that banner. It deliberately does **not** use the `-iver`
//! switch (which prints a bare version number) nor the probe's exit code:
//! `-iver` does not exist on the older builds this module most needs to
//! recognise, and a `rar` invoked with no command exits non-zero on some
//! builds and zero on others. The banner text is the one signal that is
//! stable across the whole version range.
//!
//! This module is deliberately free of any `crate::` reference so it can
//! be compiled and unit-tested on a non-Windows host — see
//! `tests/external_rar_cli_contract.rs`.

/// A `major.minor` version parsed from a RAR-family banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RarVersion {
    /// Major version (the `6` in `RAR 6.24`).
    pub major: u32,
    /// Minor version (the `24` in `RAR 6.24`). Compared numerically, so
    /// `6.24` is newer than `6.9`.
    pub minor: u32,
}

impl RarVersion {
    /// Construct a version.
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl core::fmt::Display for RarVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Which member of the RAR family the probed binary is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RarFlavor {
    /// The full `rar` archiver — the only one that can *create* archives.
    Rar,
    /// The freeware `unrar` extractor. Cannot create archives, so
    /// pointing this crate at it is a configuration error rather than a
    /// version problem.
    UnRar,
}

impl RarFlavor {
    /// `true` when this binary can create archives.
    pub const fn can_create(self) -> bool {
        matches!(self, Self::Rar)
    }

    /// Stable label for error messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rar => "rar (full archiver)",
            Self::UnRar => "unrar (extract-only freeware)",
        }
    }
}

impl core::fmt::Display for RarFlavor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

/// A successfully parsed banner: which program, and which version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RarBanner {
    /// Which RAR-family program printed the banner.
    pub flavor: RarFlavor,
    /// The version it reported.
    pub version: RarVersion,
}

/// Oldest `rar` release this crate will drive.
///
/// **5.0** is the floor because it is the first release in which the
/// `--` end-of-switches sentinel — the argument-injection guard the
/// [`super::argv`] contract depends on — is honoured, and the first that
/// understands the RAR5 format this crate's readers expect. Driving a
/// 4.x binary would silently produce RAR4 archives *and* leave a
/// leading-dash path parsed as a switch, so it is refused rather than
/// attempted.
pub const MINIMUM_RAR_VERSION: RarVersion = RarVersion::new(5, 0);

/// Parse the identifying banner out of a probe's captured output.
///
/// Accepts the whole captured stream (some builds write the banner to
/// stderr rather than stdout, so callers concatenate both) and scans for
/// the first line that starts with a RAR-family program name. Returns
/// `None` when no such line exists — which the caller reports as a
/// version-probe failure, never as a usable binary.
pub fn parse_banner(output: &str) -> Option<RarBanner> {
    output.lines().find_map(parse_banner_line)
}

/// Parse a single candidate banner line.
///
/// Split out so the line grammar is testable in isolation from the
/// "which line" search.
fn parse_banner_line(line: &str) -> Option<RarBanner> {
    let mut fields = line.split_whitespace();
    let name = fields.next()?;
    // Case-insensitive: builds have shipped both "UNRAR" and "unRAR".
    let flavor = if name.eq_ignore_ascii_case("rar") {
        RarFlavor::Rar
    } else if name.eq_ignore_ascii_case("unrar") {
        RarFlavor::UnRar
    } else {
        return None;
    };
    let version = parse_version(fields.next()?)?;
    Some(RarBanner { flavor, version })
}

/// Parse a `major.minor` token, tolerating a trailing patch component
/// (`5.0.1`) and rejecting anything that is not fully numeric.
fn parse_version(token: &str) -> Option<RarVersion> {
    let mut parts = token.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    // A third component is accepted and ignored; a fourth, or any
    // non-numeric trailer, means this is not a version token.
    if let Some(patch) = parts.next() {
        patch.parse::<u32>().ok()?;
        if parts.next().is_some() {
            return None;
        }
    }
    Some(RarVersion { major, minor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_rar_banners() {
        let cases = [
            (
                "RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal   1 Oct 2023",
                RarFlavor::Rar,
                RarVersion::new(6, 24),
            ),
            (
                "RAR 5.00   Copyright (c) 1993-2013 Alexander Roshal   22 Aug 2013",
                RarFlavor::Rar,
                RarVersion::new(5, 0),
            ),
            (
                "RAR 4.20   Copyright (c) 1993-2012 Alexander Roshal   9 Jun 2012",
                RarFlavor::Rar,
                RarVersion::new(4, 20),
            ),
            (
                "RAR 7.01 x64   Copyright (c) 1993-2024 Alexander Roshal",
                RarFlavor::Rar,
                RarVersion::new(7, 1),
            ),
        ];
        for (line, flavor, version) in cases {
            let banner = parse_banner(line).expect("banner parses");
            assert_eq!(banner.flavor, flavor, "line: {line}");
            assert_eq!(banner.version, version, "line: {line}");
        }
    }

    /// Pointing the creator at `unrar` must be reported as the wrong
    /// program, not as an old `rar`.
    #[test]
    fn distinguishes_unrar_from_rar() {
        let banner =
            parse_banner("UNRAR 6.24 freeware      Copyright (c) 1993-2023 Alexander Roshal")
                .expect("banner parses");
        assert_eq!(banner.flavor, RarFlavor::UnRar);
        assert_eq!(banner.version, RarVersion::new(6, 24));
        assert!(!banner.flavor.can_create());
        assert!(RarFlavor::Rar.can_create());
    }

    #[test]
    fn banner_search_skips_leading_noise() {
        let output = "\r\n\
             Some console codepage warning\n\
             RAR 6.24 x64   Copyright (c) 1993-2023 Alexander Roshal\n\
             \n\
             Usage:     rar <command> ...\n";
        let banner = parse_banner(output).expect("banner found past the noise");
        assert_eq!(banner.flavor, RarFlavor::Rar);
        assert_eq!(banner.version, RarVersion::new(6, 24));
    }

    #[test]
    fn rejects_output_without_a_banner() {
        for output in [
            "",
            "\n\n",
            "bash: rar: command not found",
            "7-Zip 23.01 (arm64) : Copyright (c) 1999-2023 Igor Pavlov",
            "RAR",
            "RAR version",
            "RAR x64 6.24",
            "RAR 6",
            "RAR 6.x",
            "RAR 6.24.1.7",
        ] {
            assert!(
                parse_banner(output).is_none(),
                "must not parse a banner out of {output:?}"
            );
        }
    }

    #[test]
    fn accepts_three_component_versions() {
        let banner = parse_banner("RAR 5.0.1 Copyright").expect("banner parses");
        assert_eq!(banner.version, RarVersion::new(5, 0));
    }

    /// Minor versions compare numerically, so `6.9 < 6.24`. A
    /// lexicographic comparison would call 6.24 the older build and
    /// refuse a perfectly good install.
    #[test]
    fn versions_order_numerically_not_lexicographically() {
        assert!(RarVersion::new(6, 24) > RarVersion::new(6, 9));
        assert!(RarVersion::new(7, 0) > RarVersion::new(6, 99));
        assert!(RarVersion::new(4, 20) < MINIMUM_RAR_VERSION);
        assert!(RarVersion::new(5, 0) >= MINIMUM_RAR_VERSION);
        assert!(RarVersion::new(5, 1) >= MINIMUM_RAR_VERSION);
    }

    #[test]
    fn minimum_version_is_five_zero() {
        assert_eq!(MINIMUM_RAR_VERSION, RarVersion::new(5, 0));
        assert_eq!(MINIMUM_RAR_VERSION.to_string(), "5.0");
    }

    #[test]
    fn flavor_labels_are_distinct() {
        assert_ne!(RarFlavor::Rar.label(), RarFlavor::UnRar.label());
        assert_eq!(
            RarFlavor::UnRar.to_string(),
            "unrar (extract-only freeware)"
        );
    }
}
