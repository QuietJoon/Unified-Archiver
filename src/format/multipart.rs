//! Typed multipart volume-name parser with sequence-continuity validation
//! (OI-0080-004 / R0080-0092).
//!
//! # Why this module exists
//!
//! [`Archive::detect_multipart`](crate::archive::Archive::detect_multipart)
//! recognises volume names with per-scheme string predicates, sorts the
//! survivors by an extracted `u32`, and then declares the set multipart when
//! two or more files matched. Nothing between those steps looks at the
//! *sequence*: a set whose middle volume was never copied, a set that starts
//! at `part2`, a set holding both `part1` and `part01`, and a set mixing two
//! naming conventions all report as an ordinary complete set. The gap only
//! surfaces later as a backend-specific mid-extraction failure.
//!
//! This module lifts the name heuristics into a typed model — [`VolumeName`],
//! [`VolumeScheme`], [`Volume`], [`VolumeSet`] — and adds the continuity pass
//! that was missing, returning a typed [`VolumeSetReport`] whose defects name
//! *which* volume is absent rather than reporting a bare "something is wrong"
//! boolean.
//!
//! # Scope
//!
//! End-to-end split-volume extraction remains RAR/RAR5-only; this module does
//! not change that. The ZIP-split and numeric schemes are parsed and validated
//! because they must be *recognised* to tell a mixed-scheme set from a
//! single-scheme one, and because `detect_multipart` already surfaces ZIP split
//! names at the name level. Recognition here is not a promise of extraction
//! support — consult
//! [`ArchiveFormat::capabilities`](crate::format::ArchiveFormat::capabilities)
//! for that.
//!
//! # Name-level only
//!
//! Like `detect_multipart`, nothing here opens a file or touches the
//! filesystem: every decision is derived from file names. A set that passes
//! continuity validation can still fail to open (truncated volume, wrong
//! password, unrelated file that happens to be named like a volume).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::inspection::MultipartLayout;

/// Maximum digit-run width accepted in a volume suffix.
///
/// Mirrors the bound used by the `detect_multipart` predicates: every digit run
/// must fit the `u32` volume number so matching and ordering cannot disagree.
/// Nine digits stays below `u32::MAX`.
const MAX_VOL_DIGITS: usize = 9;

/// Volume-naming convention of a multipart set.
///
/// Ordering is the tie-break used when one candidate list contains volumes
/// from more than one convention: the earliest variant wins, so a RAR
/// new-style set with a stray old-style sibling reports the sibling as the
/// anomaly rather than the other way round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum VolumeScheme {
    /// WinRAR new-style volumes: `base.part1.rar`, `base.part2.rar`, ...
    /// There is no unnumbered main volume; volume 1 is `part1`.
    RarPart,
    /// WinRAR old-style volumes: `base.rar`, `base.r00`, `base.r01`, ...
    /// rolling into `base.s00` after `base.r99`. `base.rar` is volume 1.
    RarOldStyle,
    /// ZIP split volumes: `base.zip` plus `base.z01`, `base.z02`, ...
    ///
    /// Volume 1 is `base.zip`. This mirrors the volume order
    /// `detect_multipart` already returns (main archive first, numbered
    /// segments after). The PKWARE APPNOTE places the `.zip` segment *last*
    /// on disk; that distinction only matters for readers that seek across
    /// segments, which this crate does not support for ZIP.
    ZipSplit,
    /// Pure numeric suffixes: `base.001`, `base.002`, ... Volume 1 is `.001`.
    Numeric,
}

impl VolumeScheme {
    /// Human-readable name used in diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::RarPart => "RAR new-style (.partN.rar)",
            Self::RarOldStyle => "RAR old-style (.rar/.rNN/.sNN)",
            Self::ZipSplit => "ZIP split (.zip/.zNN)",
            Self::Numeric => "numeric (.001/.002)",
        }
    }

    /// Default digit width for the scheme's numbered suffix, used when the
    /// observed volumes give no better answer.
    const fn default_width(self) -> usize {
        match self {
            Self::RarPart => 1,
            Self::RarOldStyle | Self::ZipSplit => 2,
            Self::Numeric => 3,
        }
    }
}

impl fmt::Display for VolumeScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A single file name parsed as a member of a multipart set.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VolumeName {
    /// Naming convention this name belongs to.
    pub scheme: VolumeScheme,
    /// Everything preceding the volume suffix, in the name's original case
    /// (e.g. `backup.tar` for `backup.tar.001`).
    pub base: String,
    /// 1-based position of this volume within its set. Contiguous across a
    /// scheme's roll-overs: for [`VolumeScheme::RarOldStyle`] `base.rar` is 1,
    /// `.r00` is 2, `.r99` is 101 and `.s00` is 102.
    pub number: u32,
    /// `false` for a scheme's unnumbered main volume (`base.rar`,
    /// `base.zip`), `true` when the name carries an explicit digit run.
    ///
    /// A candidate list containing no numbered volume is not a multipart set:
    /// a lone `base.rar` is an ordinary single-volume archive, not volume 1 of
    /// a set of unknown length.
    pub numbered: bool,
    /// Digit-run width as written (`3` for `.001`, `2` for `.part01.rar`),
    /// `0` for an unnumbered main volume. Used to render the expected name of
    /// an absent volume with the same zero padding the set already uses.
    pub width: usize,
}

/// Parse one file name as a multipart volume name.
///
/// Returns `None` when the name carries no recognised volume suffix. Matching
/// is ASCII-case-insensitive (`.PART1.RAR` parses); [`VolumeName::base`] keeps
/// the original case so a rendered expected name matches the set on disk.
///
/// Every `.rar` and `.zip` name parses — as its scheme's *unnumbered* main
/// volume, with [`numbered`](VolumeName::numbered) `false`. That is not by
/// itself a multipart set: a lone `archive.rar` is an ordinary single-volume
/// archive, and [`parse_volume_set`] reports a candidate list holding no
/// numbered volume as [`VolumeSetReport::Unvolumed`].
///
/// Volume number `0` is rejected for every scheme: a `.part0.rar` / `.000`
/// name is outside all four conventions, and admitting it would make the
/// 1-based continuity arithmetic ambiguous. Likewise a digit run wider than
/// the `u32` volume number can carry (see `MAX_VOL_DIGITS`) is not a volume
/// number. In both cases a `.rar` name falls through to the old-style main
/// volume, taking the rejected run into its base name — `backup.part0.rar` is
/// the main volume of a set based on `backup.part0`, which is what the file
/// actually is.
///
/// # Examples
///
/// ```
/// use unified_archive::format::multipart::{parse_volume_name, VolumeScheme};
///
/// let v = parse_volume_name("Backup.Part02.RAR").unwrap();
/// assert_eq!(v.scheme, VolumeScheme::RarPart);
/// assert_eq!(v.base, "Backup");
/// assert_eq!(v.number, 2);
/// assert_eq!(v.width, 2);
/// assert!(parse_volume_name("notes.txt").is_none());
/// ```
pub fn parse_volume_name(name: &str) -> Option<VolumeName> {
    // ASCII lowercasing preserves byte length, so offsets computed on the
    // lowered copy slice the original name at the same boundaries.
    let lower = name.to_ascii_lowercase();

    // RAR new-style, anchored at the end so a base that itself contains
    // `.part` (`my.part9.data.part1.rar`) resolves to the terminal suffix.
    if let Some(without_rar) = lower.strip_suffix(".rar") {
        if let Some(part_pos) = without_rar.rfind(".part") {
            let digits = &without_rar[part_pos + ".part".len()..];
            if let Some(number) = parse_digits(digits) {
                return Some(VolumeName {
                    scheme: VolumeScheme::RarPart,
                    base: name[..part_pos].to_string(),
                    number,
                    numbered: true,
                    width: digits.len(),
                });
            }
        }
        // Old-style main volume.
        return Some(VolumeName {
            scheme: VolumeScheme::RarOldStyle,
            base: name[..without_rar.len()].to_string(),
            number: 1,
            numbered: false,
            width: 0,
        });
    }

    if let Some(without_zip) = lower.strip_suffix(".zip") {
        return Some(VolumeName {
            scheme: VolumeScheme::ZipSplit,
            base: name[..without_zip.len()].to_string(),
            number: 1,
            numbered: false,
            width: 0,
        });
    }

    // Every remaining scheme is `<base>.<tag><digits>` with a single trailing
    // suffix, so split once at the last dot.
    let dot = lower.rfind('.')?;
    let base = &name[..dot];
    let suffix = &lower[dot + 1..];
    let mut chars = suffix.chars();
    let tag = chars.next()?;
    let digits = chars.as_str();

    match tag {
        // `.zNN` — ZIP split segment. Segment 01 is volume 2 (the `.zip`
        // main archive is volume 1).
        'z' => parse_digits(digits).map(|n| VolumeName {
            scheme: VolumeScheme::ZipSplit,
            base: base.to_string(),
            number: n + 1,
            numbered: true,
            width: digits.len(),
        }),
        // `.rNN` / `.sNN` — old-style continuation volumes. WinRAR writes
        // exactly two digits and rolls `.r99` into `.s00`, so a wider or
        // narrower run is not this convention.
        'r' | 's' if digits.len() == 2 => parse_digits_allow_zero(digits).map(|n| VolumeName {
            scheme: VolumeScheme::RarOldStyle,
            base: base.to_string(),
            // `.rNN` occupies 2..=101, `.sNN` continues at 102.
            number: n + if tag == 'r' { 2 } else { 102 },
            numbered: true,
            width: 2,
        }),
        // `.NNN` — pure numeric split.
        d if d.is_ascii_digit() => parse_digits(suffix).map(|n| VolumeName {
            scheme: VolumeScheme::Numeric,
            base: base.to_string(),
            number: n,
            numbered: true,
            width: suffix.len(),
        }),
        _ => None,
    }
}

/// Parse a bounded, non-empty, all-ASCII digit run as a non-zero number.
fn parse_digits(digits: &str) -> Option<u32> {
    parse_digits_allow_zero(digits).filter(|n| *n != 0)
}

/// As [`parse_digits`], but `0` is a valid value — used by the old-style RAR
/// series where `.r00` is a legitimate (second) volume.
fn parse_digits_allow_zero(digits: &str) -> Option<u32> {
    if digits.is_empty()
        || digits.len() > MAX_VOL_DIGITS
        || !digits.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// One member of a [`VolumeSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Volume {
    /// Path as supplied by the caller (original case preserved).
    pub path: PathBuf,
    /// 1-based volume number within the set, normalised per
    /// [`VolumeName::number`].
    pub number: u32,
}

/// A group of paths that share one naming convention and one base name.
///
/// The volume list is sorted by `(number, path)` and may contain two entries
/// with the same `number` — a duplicate is reported as a defect rather than
/// silently dropped, so the set the caller sees matches the files on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VolumeSet {
    /// Naming convention shared by every member.
    pub scheme: VolumeScheme,
    /// Base name shared by every member, in the case of the lowest-numbered
    /// member.
    pub base: String,
    /// Digit width to use when rendering an absent volume's name, taken from
    /// the lowest-numbered *numbered* member so padding matches the set.
    pub width: usize,
    /// Members in volume order.
    pub volumes: Vec<Volume>,
}

impl VolumeSet {
    /// Paths of every member, in volume order.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.volumes.iter().map(|v| v.path.clone()).collect()
    }

    /// Bridge to the crate's existing typed multipart return.
    ///
    /// A set with two or more members is
    /// [`MultipartLayout::Multi`]; a set with a single member is
    /// [`MultipartLayout::Single`], matching `detect_multipart`'s contract
    /// that one file on its own is not a multi-volume set.
    ///
    /// Panics never occur: a `VolumeSet` is only constructed with at least one
    /// volume.
    pub fn to_layout(&self) -> MultipartLayout {
        let mut parts = self.paths();
        if parts.len() > 1 {
            MultipartLayout::Multi { parts }
        } else {
            MultipartLayout::Single {
                path: parts.pop().unwrap_or_default(),
            }
        }
    }

    /// Render the on-disk name this scheme would give volume `number`.
    ///
    /// Used to tell a caller which file to go and find.
    pub fn expected_name(&self, number: u32) -> String {
        render_volume_name(self.scheme, &self.base, self.width, number)
    }
}

/// Render the name volume `number` would carry under `scheme`.
fn render_volume_name(scheme: VolumeScheme, base: &str, width: usize, number: u32) -> String {
    let width = if width == 0 {
        scheme.default_width()
    } else {
        width
    };
    match scheme {
        VolumeScheme::RarPart => format!("{base}.part{number:0width$}.rar"),
        VolumeScheme::RarOldStyle => match number {
            0 | 1 => format!("{base}.rar"),
            2..=101 => format!("{base}.r{:02}", number - 2),
            n => format!("{base}.s{:02}", n - 102),
        },
        VolumeScheme::ZipSplit => match number {
            0 | 1 => format!("{base}.zip"),
            n => format!("{base}.z{:0width$}", n - 1),
        },
        VolumeScheme::Numeric => format!("{base}.{number:0width$}"),
    }
}

/// A way in which a candidate volume list fails to describe a complete,
/// single-convention multipart set.
///
/// Each variant is a distinct failure mode with its own diagnostic; a caller
/// that cannot open a set gets the identity of the offending volume, not just
/// the fact that the set is unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VolumeSetDefect {
    /// A run of volumes strictly inside the observed range is absent.
    ///
    /// One defect per contiguous run, so a set missing volumes 4 through 900
    /// reports one defect rather than 897.
    MissingVolumes {
        /// First absent volume number of the run.
        first: u32,
        /// Last absent volume number of the run (equal to `first` for a
        /// single missing volume).
        last: u32,
        /// Name volume `first` would carry.
        expected_first: String,
    },
    /// The set does not begin at volume 1 — e.g. a RAR set whose lowest
    /// member is `part2`, or an old-style set whose `base.rar` main volume
    /// was not copied.
    ///
    /// Reported instead of [`Self::MissingVolumes`] for the leading gap, so
    /// "this set starts in the middle" stays distinguishable from "this set
    /// has a hole".
    FirstVolumeMissing {
        /// Lowest volume number actually present.
        first_present: u32,
        /// Name volume 1 would carry.
        expected_first: String,
    },
    /// Two or more files claim the same volume number — e.g. `part1.rar`
    /// alongside `part01.rar`, or `.1` alongside `.001`.
    DuplicateVolume {
        /// Volume number claimed more than once.
        number: u32,
        /// Every path claiming it, sorted.
        paths: Vec<PathBuf>,
    },
    /// A candidate parses under a different naming convention than the rest
    /// of the set.
    MixedScheme {
        /// Path that does not belong.
        path: PathBuf,
        /// Convention the path parses as.
        found: VolumeScheme,
        /// Convention the rest of the set uses.
        expected: VolumeScheme,
    },
    /// A candidate shares the set's convention but not its base name — the
    /// caller grouped two different sets together.
    UnrelatedBase {
        /// Path that does not belong.
        path: PathBuf,
        /// Base name the path carries.
        found: String,
        /// Base name the rest of the set uses.
        expected: String,
    },
}

impl fmt::Display for VolumeSetDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVolumes {
                first,
                last,
                expected_first,
            } => {
                if first == last {
                    write!(f, "volume {first} is missing (expected `{expected_first}`)")
                } else {
                    write!(
                        f,
                        "volumes {first}..={last} are missing (expected `{expected_first}` first)"
                    )
                }
            }
            Self::FirstVolumeMissing {
                first_present,
                expected_first,
            } => write!(
                f,
                "set starts at volume {first_present}, not volume 1 (expected `{expected_first}`)"
            ),
            Self::DuplicateVolume { number, paths } => {
                write!(f, "volume {number} is claimed by {} files:", paths.len())?;
                for p in paths {
                    write!(f, " `{}`", p.display())?;
                }
                Ok(())
            }
            Self::MixedScheme {
                path,
                found,
                expected,
            } => write!(
                f,
                "`{}` uses {found} naming but the set uses {expected}",
                path.display()
            ),
            Self::UnrelatedBase {
                path,
                found,
                expected,
            } => write!(
                f,
                "`{}` has base name `{found}` but the set's base name is `{expected}`",
                path.display()
            ),
        }
    }
}

/// Outcome of parsing and validating a candidate volume list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VolumeSetReport {
    /// No candidate carries an explicit volume number, so the list does not
    /// describe a multipart set. A lone `archive.rar` or `archive.zip` lands
    /// here.
    #[non_exhaustive]
    Unvolumed {
        /// The candidates, as supplied.
        paths: Vec<PathBuf>,
        /// Candidates that parsed as volume names but belong to a different
        /// set. See [`Self::unrelated`].
        unrelated: Vec<VolumeSetDefect>,
    },
    /// A single-convention set numbered 1..=n with no gaps and no duplicates.
    ///
    /// Completeness is a statement about the *names*: the highest volume
    /// number present is taken as the set's length, because no naming
    /// convention records the total. A set truncated at the end (`part1` and
    /// `part2` of three) is indistinguishable from a complete two-volume set
    /// by name alone and reports as `Complete`.
    #[non_exhaustive]
    Complete {
        /// The validated set.
        set: VolumeSet,
        /// Files alongside the set that are not part of it. See
        /// [`Self::unrelated`] — these do **not** make the set incomplete.
        unrelated: Vec<VolumeSetDefect>,
    },
    /// A recognised set with at least one continuity defect.
    #[non_exhaustive]
    Incomplete {
        /// The set as parsed — members that triggered
        /// [`VolumeSetDefect::MixedScheme`] or
        /// [`VolumeSetDefect::UnrelatedBase`] are excluded, duplicates are
        /// retained.
        set: VolumeSet,
        /// The **continuity** defects only, in a deterministic order:
        /// leading gap, interior gaps ascending, then duplicates ascending.
        ///
        /// Files that merely share the directory are not here — they are in
        /// [`Self::unrelated`] and never affect the verdict.
        defects: Vec<VolumeSetDefect>,
        /// Files alongside the set that are not part of it. See
        /// [`Self::unrelated`].
        unrelated: Vec<VolumeSetDefect>,
    },
}

impl VolumeSetReport {
    /// The parsed set, or `None` for [`Self::Unvolumed`].
    pub fn set(&self) -> Option<&VolumeSet> {
        match self {
            Self::Unvolumed { .. } => None,
            Self::Complete { set, .. } | Self::Incomplete { set, .. } => Some(set),
        }
    }

    /// **Continuity** defects — a missing or duplicated volume. Empty unless
    /// [`Self::Incomplete`], and these are exactly what decides the verdict.
    ///
    /// A file that merely shares the directory is not a defect of this set
    /// and is not reported here; see [`Self::unrelated`].
    pub fn defects(&self) -> &[VolumeSetDefect] {
        match self {
            Self::Unvolumed { .. } | Self::Complete { .. } => &[],
            Self::Incomplete { defects, .. } => defects,
        }
    }

    /// Files that parsed as volume names but belong to a *different* set —
    /// a different naming scheme, or a different base name.
    ///
    /// Reported because "this file is not part of your set" is worth
    /// knowing, and **never** counted against completeness: a set whose
    /// volumes are all present is [`Self::Complete`] no matter what else
    /// shares the directory. Reading these as defects is what made a
    /// complete set report as incomplete in any directory holding a second
    /// archive, which is the normal case rather than an edge one.
    pub fn unrelated(&self) -> &[VolumeSetDefect] {
        match self {
            Self::Unvolumed { unrelated, .. }
            | Self::Complete { unrelated, .. }
            | Self::Incomplete { unrelated, .. } => unrelated,
        }
    }

    /// `true` for a complete set only. An unvolumed list is not a set, so it
    /// is not complete either.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }
}

/// Group `paths` into one multipart set and validate its sequence.
///
/// The set's identity — convention plus base name — is taken from the largest
/// group of candidates: the group with the most numbered volumes wins, ties
/// broken by [`VolumeScheme`] order and then base name. Candidates outside
/// that group are reported as [`VolumeSetDefect::MixedScheme`] or
/// [`VolumeSetDefect::UnrelatedBase`] rather than dropped. Names that parse
/// as no volume at all are ignored.
///
/// Use [`parse_volume_set_for`] when a specific archive is being opened and
/// the set must be the one that archive belongs to.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use unified_archive::format::multipart::{parse_volume_set, VolumeSetDefect, VolumeSetReport};
///
/// let paths: Vec<PathBuf> = ["set.part1.rar", "set.part3.rar"]
///     .iter()
///     .map(PathBuf::from)
///     .collect();
/// let report = parse_volume_set(&paths);
/// assert!(!report.is_complete());
/// assert_eq!(
///     report.defects(),
///     [VolumeSetDefect::MissingVolumes {
///         first: 2,
///         last: 2,
///         expected_first: "set.part2.rar".to_string(),
///     }]
/// );
/// # let _ = matches!(report, VolumeSetReport::Incomplete { .. });
/// ```
pub fn parse_volume_set(paths: &[PathBuf]) -> VolumeSetReport {
    build_report(paths, None)
}

/// As [`parse_volume_set`], but anchored on `source`: the set is the one
/// `source` itself belongs to, whatever the candidate counts say.
///
/// This is the shape a caller wants when it holds an open archive and asks
/// "is my archive part of a complete set?" — anchoring makes the answer
/// independent of which member was opened, so `base.rar`, `base.r00` and
/// `base.r01` of one old-style set all report the same thing.
///
/// Falls back to [`parse_volume_set`]'s largest-group rule when `source`
/// itself parses as no volume name.
pub fn parse_volume_set_for(source: &Path, paths: &[PathBuf]) -> VolumeSetReport {
    let anchor = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .as_deref()
        .and_then(parse_volume_name)
        .map(|v| (v.scheme, v.base.to_ascii_lowercase()));
    build_report(paths, anchor)
}

/// Parsed candidate: its volume name plus the path it came from.
struct Candidate<'a> {
    path: &'a PathBuf,
    name: VolumeName,
}

fn build_report(paths: &[PathBuf], anchor: Option<(VolumeScheme, String)>) -> VolumeSetReport {
    let candidates: Vec<Candidate<'_>> = paths
        .iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy();
            parse_volume_name(&name).map(|name| Candidate { path, name })
        })
        .collect();

    let Some(key) = anchor.or_else(|| dominant_key(&candidates)) else {
        return VolumeSetReport::Unvolumed {
            paths: paths.to_vec(),
            unrelated: Vec::new(),
        };
    };
    let (scheme, base_lc) = key;

    // Split the candidates into set members and foreign entries. A foreign
    // entry is reported, never silently dropped: the caller asked about these
    // files, and "this file is not part of your set" is information.
    let mut members: Vec<&Candidate<'_>> = Vec::new();
    let mut foreign: Vec<VolumeSetDefect> = Vec::new();
    for c in &candidates {
        if c.name.scheme != scheme {
            foreign.push(VolumeSetDefect::MixedScheme {
                path: c.path.clone(),
                found: c.name.scheme,
                expected: scheme,
            });
        } else if c.name.base.to_ascii_lowercase() != base_lc {
            foreign.push(VolumeSetDefect::UnrelatedBase {
                path: c.path.clone(),
                found: c.name.base.clone(),
                expected: base_lc.clone(),
            });
        } else {
            members.push(c);
        }
    }

    // Deterministic order for the unrelated list, which several callers print.
    foreign.sort_by(|a, b| defect_path(a).cmp(defect_path(b)));

    // An anchored call can select a group whose only member is the anchor's
    // unnumbered main volume (a lone `base.rar`). That is a single-volume
    // archive, not a set. It used to be reported as `Incomplete` whenever a
    // foreign sibling existed, which said "volumes are missing" about an
    // archive that has no volumes at all; the siblings now ride along on
    // `Unvolumed` instead, where they inform without misclassifying.
    if !members.iter().any(|c| c.name.numbered) {
        return VolumeSetReport::Unvolumed {
            paths: paths.to_vec(),
            unrelated: foreign,
        };
    }

    let Some(set) = assemble_set(scheme, &members) else {
        return VolumeSetReport::Unvolumed {
            paths: paths.to_vec(),
            unrelated: foreign,
        };
    };

    // Completeness is decided by continuity ALONE (ticgit cf5109). `foreign`
    // answers a different question — "what else is in this directory" — and
    // merging the two made a complete set report as incomplete whenever a
    // second archive shared the directory, which is the normal case.
    let defects = continuity_defects(&set);

    if defects.is_empty() {
        VolumeSetReport::Complete {
            set,
            unrelated: foreign,
        }
    } else {
        VolumeSetReport::Incomplete {
            set,
            defects,
            unrelated: foreign,
        }
    }
}

/// Path a foreign-member defect refers to, for deterministic ordering.
fn defect_path(defect: &VolumeSetDefect) -> &Path {
    match defect {
        VolumeSetDefect::MixedScheme { path, .. } | VolumeSetDefect::UnrelatedBase { path, .. } => {
            path
        }
        _ => Path::new(""),
    }
}

/// Pick the `(scheme, lowercased base)` of the largest candidate group.
///
/// Ranked by numbered-volume count first — an unnumbered `base.rar` on its own
/// never outvotes a real numbered series — then total count, then
/// [`VolumeScheme`] order, then base name, so the result never depends on the
/// caller's iteration order.
fn dominant_key(candidates: &[Candidate<'_>]) -> Option<(VolumeScheme, String)> {
    let mut groups: BTreeMap<(VolumeScheme, String), (usize, usize)> = BTreeMap::new();
    for c in candidates {
        let entry = groups
            .entry((c.name.scheme, c.name.base.to_ascii_lowercase()))
            .or_insert((0, 0));
        if c.name.numbered {
            entry.0 += 1;
        }
        entry.1 += 1;
    }
    // `BTreeMap` keys are unique, and the `Reverse` tie-breakers make the
    // ranking key unique too, so the winner never depends on iteration order.
    groups
        .into_iter()
        .max_by_key(|((scheme, base), (numbered, total))| {
            (
                *numbered,
                *total,
                std::cmp::Reverse(*scheme),
                std::cmp::Reverse(base.clone()),
            )
        })
        .map(|(key, _)| key)
}

/// Collect group members into a sorted [`VolumeSet`].
fn assemble_set(scheme: VolumeScheme, members: &[&Candidate<'_>]) -> Option<VolumeSet> {
    if members.is_empty() {
        return None;
    }
    let mut volumes: Vec<Volume> = members
        .iter()
        .map(|c| Volume {
            path: c.path.clone(),
            number: c.name.number,
        })
        .collect();
    volumes.sort_by(|a, b| a.number.cmp(&b.number).then_with(|| a.path.cmp(&b.path)));

    // Base and padding come from the lowest-numbered member, and the padding
    // from the lowest-numbered *numbered* member so an unnumbered main volume
    // (width 0) does not erase the set's own convention.
    let lowest = members
        .iter()
        .min_by_key(|c| (c.name.number, c.name.base.clone()))
        .expect("members is non-empty");
    let width = members
        .iter()
        .filter(|c| c.name.numbered)
        .min_by_key(|c| c.name.number)
        .map_or(0, |c| c.name.width);

    Some(VolumeSet {
        scheme,
        base: lowest.name.base.clone(),
        width,
        volumes,
    })
}

/// Validate that a set's numbers are `1..=max` with no gaps and no repeats.
///
/// Works from a number-keyed index rather than a positional scan, so a gap of
/// one million volumes costs one defect and one iteration step, not a million.
fn continuity_defects(set: &VolumeSet) -> Vec<VolumeSetDefect> {
    let mut by_number: BTreeMap<u32, Vec<PathBuf>> = BTreeMap::new();
    for volume in &set.volumes {
        by_number
            .entry(volume.number)
            .or_default()
            .push(volume.path.clone());
    }

    let mut defects = Vec::new();

    // Leading gap: reported as its own defect so "this set starts in the
    // middle" stays distinguishable from "this set has a hole".
    if let Some(&first_present) = by_number.keys().next() {
        if first_present > 1 {
            defects.push(VolumeSetDefect::FirstVolumeMissing {
                first_present,
                expected_first: set.expected_name(1),
            });
        }
    }

    // Interior gaps: one defect per contiguous run of absent numbers.
    let mut previous: Option<u32> = None;
    for &number in by_number.keys() {
        if let Some(prev) = previous {
            if number > prev + 1 {
                defects.push(VolumeSetDefect::MissingVolumes {
                    first: prev + 1,
                    last: number - 1,
                    expected_first: set.expected_name(prev + 1),
                });
            }
        }
        previous = Some(number);
    }

    // Repeats: two spellings of the same volume number (`part1` and `part01`,
    // `.1` and `.001`) would make the set look complete while the volume
    // order is genuinely ambiguous.
    for (number, paths) in by_number {
        if paths.len() > 1 {
            defects.push(VolumeSetDefect::DuplicateVolume { number, paths });
        }
    }

    defects
}

#[cfg(test)]
mod tests;
