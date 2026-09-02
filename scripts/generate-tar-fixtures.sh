#!/usr/bin/env bash
#
# Generate the TAR test fixtures that this crate cannot produce itself.
#
# NEVER call this from build.rs, from a #[test], from a cargo alias, or from
# any other part of the compile/build/test pipeline. Run it by hand when a
# fixture must change, commit the bytes, and let the test suite consume the
# committed bytes. That is the same rule scripts/generate-rar-fixtures.sh
# follows, and for the same reason: a test that shells out to a CLI is a test
# that silently changes meaning with the host.
#
# WHY THIS EXISTS. The sparse-TAR lane needs an archive whose member is
# GNU-sparse-ENCODED — a hole recorded as a hole rather than as a run of
# zeros. macOS ships bsdtar, which writes a dense archive for the same input,
# so on this host the tripwire could not fire and the lane sat `#[ignore]`d
# waiting for CI that does not exist. Generating once on a GNU-tar host and
# committing the bytes removes the host dependency permanently.
#
# Requires GNU tar. On macOS: `brew install gnu-tar`, which installs `gtar`.
# bsdtar will NOT do — it accepts `-S` and ignores it.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixtures="${repo_root}/tests/fixtures"

tar_bin="${GNU_TAR:-gtar}"
if ! command -v "${tar_bin}" >/dev/null 2>&1; then
    if command -v tar >/dev/null 2>&1 && tar --version 2>/dev/null | head -1 | grep -q 'GNU tar'; then
        tar_bin=tar
    else
        echo "error: GNU tar not found. Install it (brew install gnu-tar) or set GNU_TAR." >&2
        exit 1
    fi
fi
echo "using $(command -v "${tar_bin}"): $("${tar_bin}" --version | head -1)"

force=0
[[ "${1:-}" == "--force" ]] && force=1

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

# --- sparse.tar -------------------------------------------------------------
#
# One 1 MiB member that is a single byte of data at the very end and a hole
# before it. The digest and stream lanes assert that the crate materialises the
# hole and delivers the full LOGICAL extent (1 MiB), rather than reading the
# archive's physical length and reporting a truncation.
target="${fixtures}/sparse.tar"
if [[ -e "${target}" && "${force}" -ne 1 ]]; then
    echo "skip  sparse.tar (exists; pass --force to regenerate)"
else
    mkdir -p "${workdir}/staging"
    python3 -c "
import sys
with open(sys.argv[1], 'wb') as f:
    f.seek(1024 * 1024 - 1)
    f.write(b'Z')
" "${workdir}/staging/sparse.bin"

    "${tar_bin}" -cSf "${target}" -C "${workdir}/staging" sparse.bin

    # Verify it is actually sparse-ENCODED rather than merely accepted. Two
    # independent checks, because either alone can pass on a dense archive:
    # the physical size must be far under the logical one, and the member's
    # typeflag must be 'S' (GNU sparse).
    physical=$(stat -f %z "${target}" 2>/dev/null || stat -c %s "${target}")
    if [[ "${physical}" -ge 1048576 ]]; then
        echo "error: wrote a DENSE ${physical}-byte archive — ${tar_bin} is not sparse-encoding." >&2
        rm -f "${target}"
        exit 1
    fi
    typeflag=$(python3 -c "
import sys
print(chr(open(sys.argv[1],'rb').read(512)[156]))
" "${target}")
    if [[ "${typeflag}" != "S" ]]; then
        echo "error: member typeflag is '${typeflag}', expected 'S' (GNU sparse)." >&2
        rm -f "${target}"
        exit 1
    fi
    echo "wrote sparse.tar (${physical} bytes, typeflag S, 1 MiB logical)"
fi

echo "done."
