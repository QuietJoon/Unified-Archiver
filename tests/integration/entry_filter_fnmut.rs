//! Regression test for R0075-0080 — `EntryFilter` is `FnMut`.
//!
//! The earlier `Box<dyn Fn>` rejected closures whose captures were
//! mutable (counters, accumulators, dedup maps). Widening to `FnMut`
//! lets stateful filters drive selective extraction directly without
//! `Cell`/`RefCell` wrappers.

use unified_archive::{Archive, EntryFilter, ExtractionOptions};

#[test]
fn entry_filter_supports_mutable_state() {
    let archive = Archive::open(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.zip"),
    )
    .expect("open test.zip");

    // Stateful filter: only take entries at even visit-counts. The
    // closure mutates `counter` on every invocation, which would not
    // compile under the old `Fn` bound.
    let mut counter: u32 = 0;
    let filter: EntryFilter = Box::new(move |_entry| {
        counter += 1;
        counter % 2 == 0
    });

    let dest = std::env::temp_dir().join("entry_filter_fnmut_dest");
    let _ = std::fs::remove_dir_all(&dest);
    let mut opts = ExtractionOptions::new(&dest);
    opts.filter = Some(filter);

    // The extract path takes `mut options` and consumes the filter.
    // Just confirm it runs without complaint — the type-system wins
    // are pinned by virtue of the call compiling.
    let _ = archive.extract_all(opts);
    let _ = std::fs::remove_dir_all(&dest);
}

#[test]
fn entry_filter_from_fn_lifts_fn_closure() {
    // The compatibility shim takes a plain `Fn` closure and produces
    // the `FnMut`-typed alias, so callers don't have to relearn the
    // boxing ceremony when they want a stateless filter.
    let filter = unified_archive::entry_filter_from_fn(|e| e.path.ends_with(".txt"));
    // Fully type-annotated to confirm the alias resolves to FnMut.
    let _: EntryFilter = filter;
}
