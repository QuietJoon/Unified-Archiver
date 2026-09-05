#!/usr/bin/env bash
#
# Generate the 7z test fixtures this crate cannot produce itself.
#
# NEVER call this from build.rs, from a #[test], from a cargo alias, or from
# any other part of the compile/build/test pipeline — same rule, and the same
# reason, as scripts/generate-rar-fixtures.sh: it shells out to an external
# archiver that is not a build dependency. Run it by hand when a fixture must
# change, commit the bytes, and let the suite consume the committed bytes.
#
# Usage:
#   scripts/generate-7z-fixtures.sh              # generate anything missing
#   scripts/generate-7z-fixtures.sh --force      # regenerate everything
#   SEVENZ_BIN=/path/to/7zz scripts/generate-7z-fixtures.sh

set -euo pipefail

SEVENZ_BIN="${SEVENZ_BIN:-7zz}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixtures="$repo_root/tests/fixtures"

force=0
[[ "${1:-}" == "--force" ]] && force=1

if ! command -v "$SEVENZ_BIN" >/dev/null 2>&1; then
    echo "error: the 7-Zip CLI was not found (looked for: $SEVENZ_BIN)." >&2
    echo "       Install it with: brew install sevenzip" >&2
    exit 2
fi

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

want() {
    if [[ $force -eq 0 && -e "$fixtures/$1" ]]; then
        echo "skip  $1 (exists; pass --force to regenerate)"
        return 1
    fi
    return 0
}

made=()

# test_split.7z.001 .. — a numerically split volume set (OI-0080-004).
#
# A 7z volume set is a PLAIN BYTE SPLIT of one archive: no part carries a
# header, a footer, or a volume number. Only part 1 begins with the 7z magic;
# every later part is raw payload bytes. That is why the crate reads a set by
# concatenating the members rather than by parsing per-volume metadata, and it
# is why this fixture is generated with a real archiver instead of by hand —
# the property under test is that OUR reassembly matches 7-Zip's.
#
# The payload is random so that -mx0 (store) leaves it incompressible and -v
# actually splits it; a compressible payload would collapse into one volume.
if want "test_split.7z.001"; then
    rm -f "$fixtures"/test_split.7z.0*
    head -c 200000 /dev/urandom > "$staging/split_payload.bin"
    printf 'tail entry after the split payload\n' > "$staging/tail_marker.txt"
    ( cd "$staging" && "$SEVENZ_BIN" a -bso0 -bsp0 -mx0 -v64k test_split.7z \
        split_payload.bin tail_marker.txt >/dev/null )
    for part in "$staging"/test_split.7z.0*; do
        made+=("$(basename "$part")")
    done
    # The reference single-file archive of the same content, so a test can
    # assert the two list identically.
    ( cd "$staging" && "$SEVENZ_BIN" a -bso0 -bsp0 -mx0 test_split_whole.7z \
        split_payload.bin tail_marker.txt >/dev/null )
    made+=("test_split_whole.7z")
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

echo
echo "=== verification: the parts concatenate to a valid archive ==="
cat "$fixtures"/test_split.7z.0* > "$staging/rejoined.7z"
"$SEVENZ_BIN" t -bso0 "$staging/rejoined.7z" >/dev/null && echo "rejoined set tests OK"
"$SEVENZ_BIN" t -bso0 "$fixtures/test_split_whole.7z" >/dev/null && echo "whole archive tests OK"

echo
echo "Next: update tests/fixtures/README.md in the same commit."
