#!/usr/bin/env bash
#
# Generate the RAR test fixtures that this crate cannot produce itself.
#
# NEVER call this from build.rs, from a #[test], from a cargo alias, or from
# any other part of the compile/build/test pipeline. It shells out to the
# proprietary RARLAB `rar` CLI, which is not a build dependency, is not
# present on most machines, and on macOS is stalled by syspolicyd for roughly
# a minute per invocation. Run it by hand when a fixture must change, commit
# the bytes, and let the test suite consume the committed bytes.
#
# The crate can only *read* RAR; the external creation lane is gated on
# cfg(all(target_os = "windows", feature = "external-rar-create")). So the
# recovery-record and multi-volume fixtures have to come from `rar` — see
# ticgit 8c29f8 (fixture gap) and d3cfce (missing-volume behaviour).
#
# Usage:
#   scripts/generate-rar-fixtures.sh              # generate anything missing
#   scripts/generate-rar-fixtures.sh --force      # regenerate everything
#   RAR_BIN=/path/to/rar scripts/generate-rar-fixtures.sh
#
# After running, update the FIXTURE_RECOVERY table in
# tests/recovery_percentage_edge_cases.rs and the entries in
# tests/fixtures/README.md in the same commit.

set -euo pipefail

RAR_BIN="${RAR_BIN:-rar}"
UNRAR_BIN="${UNRAR_BIN:-unrar}"
PASSWORD="test123"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixtures="$repo_root/tests/fixtures"

force=0
[[ "${1:-}" == "--force" ]] && force=1

if ! command -v "$RAR_BIN" >/dev/null 2>&1; then
    cat >&2 <<MSG
error: the RARLAB \`rar\` CLI was not found (looked for: $RAR_BIN).

This script is the only way these fixtures can be produced; the crate itself
cannot create RAR archives outside the Windows-only external-rar-create lane.
Install the proprietary CLI (macOS: \`brew install --cask rar\`) or set RAR_BIN.
MSG
    exit 2
fi

# macOS: the first exec of an unnotarised binary is serialised behind
# syspolicyd and takes about a minute. Later execs in the same run are
# usually faster, but do not count on it — never put this on a timeout
# shorter than a couple of minutes, and never inside a test.
echo "note: each \`rar\` invocation may stall ~1 minute on macOS (syspolicyd)."

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

# Small payload: byte-identical to the one behind test.rar, so the recovery
# fixtures differ from it only by the recovery record.
printf 'Hello, RAR World!\n' > "$staging/test_file.txt"

# Large, incompressible-by-store payload for volume splitting. Deterministic
# so a regeneration produces the same content (not the same bytes — rar
# stamps timestamps into headers).
: > "$staging/volume_payload.bin"
for i in $(seq 0 1023); do
    printf 'unified-archive multi-volume fixture block %04d ' "$i" >> "$staging/volume_payload.bin"
done

# A second, tiny file to sit AFTER the split one. The three-volume fixture
# above holds a single logical entry, so every extraction walk reaches its
# target before any continuation header can be mis-counted. Index alignment
# only breaks when a split entry is SKIPPED and the walk carries on, which
# needs something after it to walk to (ticgit 3b4d15 remainder).
printf 'tail marker after the split file\n' > "$staging/tail_marker.txt"

want() {
    local target="$1"
    if [[ $force -eq 0 && -e "$fixtures/$target" ]]; then
        echo "skip  $target (exists; pass --force to regenerate)"
        return 1
    fi
    return 0
}

# rar `a` updates an existing archive rather than replacing it, so clear the
# target first or a regeneration silently keeps stale entries.
run_rar() {
    echo "rar   $*"
    ( cd "$staging" && "$RAR_BIN" "$@" )
}

made=()

# 1. test_recovery.rar — a real 5% recovery record, so MHFL_RECOVERY (0x0008)
#    is set in the RAR5 main header. Before this script existed the committed
#    file was byte-identical to test.rar and carried no recovery record at
#    all, which left the positive branch of the recovery-percentage contract
#    with no fixture anywhere in the repository (R0001-0088).
if want "test_recovery.rar"; then
    rm -f "$staging/test_recovery.rar"
    run_rar a -rr5p -ep -idq test_recovery.rar test_file.txt
    made+=("test_recovery.rar")
fi

# 2. test_encrypted_recovery.rar — header encryption (-hp) *and* a recovery
#    record, the combination that exercises parse_recovery_percentage's
#    second file open against an encrypted archive (R0001-0089). -hp, not -p:
#    the whole header is encrypted, so the archive cannot even be listed
#    without the password.
if want "test_encrypted_recovery.rar"; then
    rm -f "$staging/test_encrypted_recovery.rar"
    run_rar a "-hp$PASSWORD" -rr5p -ep -idq test_encrypted_recovery.rar test_file.txt
    made+=("test_encrypted_recovery.rar")
fi

# 3. test_encrypted_data_recovery.rar — data encryption (-p) *plus* a
#    recovery record. Unlike -hp the headers stay readable, so this is the
#    fixture that actually exercises parse_recovery_percentage's second file
#    open against an encrypted archive and gets an answer out of it
#    (R0001-0089 item 2). The -hp fixture above covers the case where the
#    header cannot be read at all.
if want "test_encrypted_data_recovery.rar"; then
    rm -f "$staging/test_encrypted_data_recovery.rar"
    run_rar a "-p$PASSWORD" -rr5p -ep -idq test_encrypted_data_recovery.rar test_file.txt
    made+=("test_encrypted_data_recovery.rar")
fi

# 4. test_multivol.part1.rar .. — a genuine multi-volume set. -m0 (store)
#    keeps the payload incompressible so -v20k actually splits it, which a
#    compressed 48 KiB of repeating text would not. Needed for the
#    missing-volume test: the UnRAR volume-change callback must fail with a
#    bounded typed error instead of retrying forever (ticgit d3cfce).
if want "test_multivol.part1.rar"; then
    rm -f "$fixtures"/test_multivol.part*.rar
    rm -f "$staging"/test_multivol.part*.rar
    run_rar a -m0 -v20k -ep -idq test_multivol.rar volume_payload.bin
    for part in "$staging"/test_multivol.part*.rar; do
        made+=("$(basename "$part")")
    done
fi

# 5. test_multivol_tail.part1.rar .. — a volume set whose FIRST entry is
#    split across volumes and whose SECOND entry follows it. This is the
#    shape that catches index drift in the extraction walks: UnRAR collapses
#    continuation headers only under RAR_OM_LIST, and this crate opens with
#    RAR_OM_EXTRACT, so a walk that skips the split entry (a selective
#    extract_by_ids, or extract_file seeking a later id) is handed the
#    part-2 header with RHDF_SPLITBEFORE set and counts it as another entry.
#    test_multivol.part1.rar cannot show this because it holds one entry.
#    Argument order is storage order, so volume_payload.bin splits and
#    tail_marker.txt lands after it (ticgit 3b4d15 remainder).
if want "test_multivol_tail.part1.rar"; then
    rm -f "$fixtures"/test_multivol_tail.part*.rar
    rm -f "$staging"/test_multivol_tail.part*.rar
    run_rar a -m0 -v20k -ep -idq test_multivol_tail.rar volume_payload.bin tail_marker.txt
    for part in "$staging"/test_multivol_tail.part*.rar; do
        made+=("$(basename "$part")")
    done
fi

if [[ ${#made[@]} -eq 0 ]]; then
    echo "nothing to do."
    exit 0
fi

for name in "${made[@]}"; do
    cp "$staging/$name" "$fixtures/$name"
done

echo
echo "=== generated ==="
for name in "${made[@]}"; do
    printf '%s  %s\n' "$(shasum -a 256 "$fixtures/$name" | cut -d' ' -f1)" "$name"
done

# unrar is a separate binary and is not quarantined, so these checks are
# fast even when `rar` itself is slow.
if command -v "$UNRAR_BIN" >/dev/null 2>&1; then
    echo
    echo "=== unrar verification ==="
    for name in "${made[@]}"; do
        case "$name" in
            test_multivol.part1.rar)
                "$UNRAR_BIN" t "$fixtures/$name" || echo "WARN: $name failed unrar t" ;;
            test_multivol.part*)
                : ;;  # covered by part1
            test_multivol_tail.part1.rar)
                "$UNRAR_BIN" t "$fixtures/$name" || echo "WARN: $name failed unrar t" ;;
            test_multivol_tail.part*)
                : ;;  # covered by part1
            test_encrypted_recovery.rar|test_encrypted_data_recovery.rar)
                "$UNRAR_BIN" t "-p$PASSWORD" "$fixtures/$name" || echo "WARN: $name failed unrar t" ;;
            *)
                "$UNRAR_BIN" t "$fixtures/$name" || echo "WARN: $name failed unrar t" ;;
        esac
    done
fi

echo
echo "Next: update FIXTURE_RECOVERY in tests/recovery_percentage_edge_cases.rs"
echo "      and the entries in tests/fixtures/README.md, in the same commit."
