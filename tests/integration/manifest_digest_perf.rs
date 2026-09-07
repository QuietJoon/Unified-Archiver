//! Empirical performance sentinel for R0075-0084 / OI-0001-009 —
//! `calculate_manifest_digest` on a CRC-less libarchive format must stay
//! linear in the archive's bytes, not quadratic in its entry count.
//!
//! **Shipped behaviour.** The libarchive backend overrides
//! `ReadBackend::visit_payloads_by_listing_id`, so every CRC-less entry's
//! payload is resolved in a *single* traversal: a compressed TAR is
//! decompressed once, and each member's CRC32 is taken from a borrowed
//! reader as the walk passes it. Before that override landed (ticgit
//! `82bf8fd4`) the digest re-opened the archive and re-walked it from byte
//! zero once per entry, which on a 1k-entry `tar.gz` cost ~150-200× a
//! single `list_files()`.
//!
//! **Why the guard is a ratio, not a wall-clock budget.** Both halves are
//! measured on the same archive on the same machine in the same run, so
//! the comparison survives slow CI hardware, debug builds and a cold page
//! cache — all of which scale the two measurements together. A regression
//! to per-entry re-opening does not: it multiplies only the digest half,
//! by roughly the entry count.
//!
//! The bound (50× the `list_files()` baseline) is deliberately generous.
//! The single-traversal digest genuinely does more work than a metadata
//! walk — it decodes every payload rather than skipping to the next header
//! — so a small constant factor is expected and correct. 50× sits far
//! above that factor and far below the ~150× a 1000-entry quadratic walk
//! produces, which is the whole point: it is a shape check, not a
//! stopwatch. Run with `--nocapture` to see the measured ratio; a debug
//! build on an Apple-silicon laptop reported 2.6× (639 µs listing,
//! 1.65 ms digest) when the `#[ignore]` was lifted, so the guard has
//! roughly nineteen times the headroom it needs.
//!
//! **This test runs in the default lane** — the `#[ignore]` it carried
//! while the walk really was quadratic has been lifted, because the
//! refactor it was waiting on merged. The archive is built in-process into
//! a `tempfile` directory and the whole test finishes in ~0.02 s, so it
//! costs about as much as an ordinary round-trip test while pinning a
//! property nothing else in the suite observes. Do not re-`#[ignore]` it
//! to silence a failure: a failure here means the payload walk regressed.

// The sentinel is defined on a CRC-less libarchive format, so the whole
// file needs that backend (AD 0058 format features).
#![cfg(feature = "libarchive")]

#[cfg_attr(
    not(all(any(feature = "create", feature = "modify"), feature = "integrity")),
    allow(unused_imports)
)]
use std::time::Instant;
#[cfg_attr(
    not(all(any(feature = "create", feature = "modify"), feature = "integrity")),
    allow(unused_imports)
)]
use unified_archive::{Archive, CompressionOptions, WritableFormat};

#[cfg_attr(
    not(all(feature = "integrity", any(feature = "create", feature = "modify"))),
    allow(dead_code)
)]
/// Entry count. Large enough that a quadratic walk is unmissable (the
/// pre-fix ratio scaled with it), small enough that the linear walk stays
/// in the low milliseconds.
const ENTRIES: usize = 1000;

#[cfg(feature = "integrity")]
#[cfg(feature = "create")]
#[test]
fn manifest_digest_does_not_reopen_archive_per_entry() {
    let dir = tempfile::tempdir().expect("temp dir");
    let archive_path = dir.path().join("big.tar.gz");
    {
        let opts = CompressionOptions::for_writable(WritableFormat::TAR_GZIP);
        let mut a = Archive::create(&archive_path, opts).unwrap();
        for i in 0..ENTRIES {
            a.add_file_from_data(&format!("file_{i:04}.txt"), b"x")
                .unwrap();
        }
        a.finish().unwrap();
    }

    let archive = Archive::open(&archive_path).unwrap();
    let t0 = Instant::now();
    let listing = archive.list_files().unwrap();
    let baseline = t0.elapsed();
    assert_eq!(listing.len(), ENTRIES, "fixture did not round-trip");
    // TAR carries no per-entry checksum, so every entry must take the
    // streaming arm — otherwise this test would measure nothing.
    assert!(
        listing.iter().all(|e| e.crc32.is_none()),
        "fixture stopped being CRC-less; the sentinel no longer exercises \
         the payload walk"
    );

    let t1 = Instant::now();
    let _ = archive.calculate_manifest_digest().unwrap();
    let digest_time = t1.elapsed();

    // Captured unless `--nocapture`: the measured ratio is the number to
    // look at when tuning the bound or diagnosing a flap.
    println!(
        "list_files {baseline:?}, manifest_digest {digest_time:?} over {ENTRIES} entries \
         (ratio {:.1}x, bound 50x)",
        digest_time.as_secs_f64() / baseline.as_secs_f64()
    );

    assert!(
        digest_time < baseline * 50,
        "manifest_digest ({digest_time:?}) is > 50x the baseline list_files \
         ({baseline:?}) over {ENTRIES} entries; the single-traversal payload \
         walk (OI-0001-009) has regressed to re-opening the archive per entry"
    );
}
