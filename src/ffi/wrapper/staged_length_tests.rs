use super::check_staged_length;
use crate::error::{ArchiveError, ops};

/// The failing side of the R1 staged-length hardening. Before this test
/// the check shipped with accept-side coverage only, and that accept-side
/// test passed identically without the check present — so nothing pinned
/// the behaviour the DCR-006 Amendment 4 classification table depends on.
#[test]
fn staged_payload_shorter_than_the_declaration_is_corruption() {
    let err = check_staged_length("payload.bin", ops::EXTRACT_TO_STREAM, 512, Some(4096))
        .expect_err("a short staged payload must not be accepted");
    assert!(
        matches!(err, ArchiveError::Corruption { .. }),
        "truncation must be Corruption, got: {err:?}"
    );
    let text = err.to_string();
    assert!(
        text.contains("staged payload is 512 bytes, listing declares 4096"),
        "the error must name both counts, got: {text}"
    );
    assert!(
        text.contains(ops::EXTRACT_TO_STREAM),
        "the error must name the caller's operation (R5), got: {text}"
    );
}

#[test]
fn staged_payload_longer_than_the_declaration_is_corruption() {
    let err = check_staged_length("payload.bin", ops::EXTRACT_TO_MEMORY, 8192, Some(4096))
        .expect_err("an over-produced staged payload must not be accepted");
    assert!(matches!(err, ArchiveError::Corruption { .. }));
    assert!(
        err.to_string()
            .contains("staged payload is 8192 bytes, listing declares 4096")
    );
}

#[test]
fn staged_payload_matching_the_declaration_is_accepted() {
    check_staged_length("payload.bin", ops::EXTRACT_TO_STREAM, 4096, Some(4096))
        .expect("an exact match must pass");
}

/// Hard constraint 3: a header that declares no size gets no invented
/// declaration here — the facade's read-time exactness check stays the
/// sole authority for those entries.
#[test]
fn an_undeclared_size_invents_no_declaration() {
    check_staged_length("payload.bin", ops::EXTRACT_TO_STREAM, 4096, None)
        .expect("no declaration means nothing to violate");
    check_staged_length("payload.bin", ops::EXTRACT_TO_STREAM, 0, None)
        .expect("an empty staged payload is equally undeclared");
}
