#!/usr/bin/env bash
# Generate committed ZIP fixtures that the test suite cannot build itself.
#
# Run by hand; never from the build. Most ZIP fixtures are written in-test
# through the `zip` crate, which is simpler and keeps the fixture next to
# the assertion. The one below is the exception: it is a WinZip-AES
# archive, and the whole point of the test that consumes it is to run in a
# build compiled WITHOUT `zip-crypto` — which is exactly a build that
# cannot write one. So it has to be produced out of band and committed.
#
# Usage:  scripts/generate-zip-fixtures.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixtures="${repo_root}/tests/fixtures"
work="$(mktemp -d "${TMPDIR:-/tmp}/zipfix.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

if ! command -v 7zz >/dev/null 2>&1; then
    echo "7zz not found; install p7zip (brew install sevenzip)" >&2
    exit 1
fi

# AES-256 encrypted, stored (not deflated) so the payload is long enough
# to be AE-1 rather than AE-2 and the shape stays easy to reason about.
printf 'a longer payload so this is AE-1 not AE-2' > "${work}/secret.txt"
7zz a -tzip -mm=Copy -mem=AES256 -pfixturepw \
    "${work}/aes256.zip" "${work}/secret.txt" >/dev/null

install -m 644 "${work}/aes256.zip" "${fixtures}/test_aes256.zip"
echo "wrote ${fixtures}/test_aes256.zip ($(wc -c < "${fixtures}/test_aes256.zip" | tr -d ' ') bytes)"
