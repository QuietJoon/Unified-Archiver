#!/usr/bin/env bash
#
# The release gate: every verification lane, run once, recorded.
#
# WHAT THIS IS FOR. AD-0070 defines "CI" for this project as recorded
# per-platform verification rather than a hosted service. This script is the
# committed recipe half of that definition — the thing that makes a run
# repeatable by someone who was not there. The owner runs it on each platform
# they can reach; the record it prints goes under docs/verification/.
#
# A property is CI-covered when all four of these hold, and this script gives
# you three of them:
#   1. committed recipe          <- this file
#   2. a host that can observe   <- YOUR job: run it on the platform in question
#   3. native exit status, last  <- LANE_EXIT written after each stage terminates
#   4. bound to a fingerprint    <- the preamble below
#
# RUN IT IN THE FOREGROUND of a terminal you own. Not from a Claude background
# task, not from a daemon, not detached. A gate whose lifecycle nobody owns
# produces evidence nobody can trust, and this project has already lost one run
# that way. If it must outlive your terminal, use tmux and say so in the record.
#
# NEVER call this from build.rs, from a #[test], or from a cargo alias — same
# rule as scripts/generate-*-fixtures.sh, for the same reason.
#
# CARGO_TARGET_DIR IS NEVER SET OR OVERRIDDEN HERE, and --target-dir is never
# passed. That setting is machine-critical on the owner's host. The script
# records where cargo says the target dir is; it does not choose it.

set -uo pipefail   # deliberately NOT -e: each lane's status is captured, not fatal

usage() {
    cat <<'USAGE'
usage: scripts/release-gate.sh [options]

  --with-windows-check   add the Windows cfg-compile lane (L6). OWNER-INVOKED
                         ONLY: the Windows lane is on hold by the 2026-09-01
                         ruling, and an agent must not enable this to "just
                         check". Off by default.
  --with-cross-check     add the Linux cross-compile check lane (L7). Only
                         meaningful once OI-0080-001 is unparked. Off by default.
  --allow-contention     proceed even when foreign cargo/rustc processes are
                         running. Off by default, because a contended run is
                         slow enough to look hung and this host has stalled a
                         gate for 66 minutes that way.
  --artifacts DIR        where lane logs go. Default:
                         /Volumes/Temp/claude/gate/release-<date>-<sha>
  -h, --help             this text
USAGE
}

with_windows=0
with_cross=0
allow_contention=0
artifacts=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-windows-check) with_windows=1; shift ;;
        --with-cross-check)   with_cross=1; shift ;;
        --allow-contention)   allow_contention=1; shift ;;
        --artifacts)          artifacts="${2:-}"; shift 2 ;;
        -h|--help)            usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo nogit)"
today="$(date -u +%Y-%m-%d)"
artifacts="${artifacts:-/Volumes/Temp/claude/gate/release-${today}-${short_sha}}"
mkdir -p "${artifacts}"
summary="${artifacts}/SUMMARY.md"

# Where cargo will actually build. Read, never set — that setting is
# machine-critical here. Used by the contention audit below to tell a build
# that would serialise against this one from a build that merely shares a CPU.
target_dir="$(cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

# --- Preamble: the fingerprint. Condition 4. -------------------------------
dirty="$(git status --porcelain 2>/dev/null)"
dirty_digest="clean"
if [[ -n "${dirty}" ]]; then
    dirty_digest="DIRTY sha256=$( { git status --porcelain; git diff; } | shasum -a 256 | cut -d' ' -f1 )"
fi

{
    echo "# Release gate — ${today}"
    echo
    echo '## Fingerprint'
    echo
    echo "- commit: \`$(git rev-parse HEAD 2>/dev/null || echo unknown)\`"
    echo "- worktree: ${dirty_digest}"
    echo "- host: \`$(uname -srm)\`$( [[ "$(uname -s)" == Darwin ]] && echo " / macOS $(sw_vers -productVersion 2>/dev/null)" )"
    echo "- rustc: \`$(rustc -V 2>/dev/null)\`"
    echo "- cargo: \`$(cargo -V 2>/dev/null)\`"
    echo "- target dir (as cargo reports it, never set by this script): \`${target_dir:-unknown}\`"
    echo "- TMPDIR: \`${TMPDIR:-<unset>}\`"
    echo "- temp volume free: \`$(df -h "${TMPDIR:-/tmp}" 2>/dev/null | tail -1 | awk '{print $4" of "$2}')\`"
    echo "- started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo
} > "${summary}"

if [[ -n "${dirty}" ]]; then
    echo "WARNING: worktree is dirty. The record will say so; a dirty record is" >&2
    echo "         evidence for nothing but the moment it was taken." >&2
fi

# --- Contention audit. Never kills anything. -------------------------------
#
# What actually serialises against this gate is another build using the SAME
# target directory: cargo takes a lock on it, so such a build does not slow
# this one down, it blocks it. A build in a different project uses a different
# target directory (this machine gives each project its own) and can only
# compete for CPU.
#
# This audit used to refuse on ANY cargo or rustc process anywhere on the
# host. On a machine that builds several projects at once that is almost
# always true, so the gate was refusing on evidence of nothing — measured
# 2026-09-03, it refused while the only other cargo was in an unrelated
# project and this project's target directory was untouched.
#
# The signal used instead: does the process hold any file under our target
# directory? Verified to separate the two cases cleanly.
same_target=()
other_builds=0
for pid in $(pgrep -x cargo 2>/dev/null; pgrep -x rustc 2>/dev/null); do
    if [[ -n "${target_dir}" ]] && lsof -p "${pid}" 2>/dev/null | grep -qF -- "${target_dir}"; then
        same_target+=("${pid}")
    else
        other_builds=$((other_builds + 1))
    fi
done

if [[ "${#same_target[@]}" -gt 0 ]]; then
    echo "contention: ${#same_target[@]} process(es) are building into ${target_dir}" >&2
    echo "  pids: ${same_target[*]}" >&2
    if [[ "${allow_contention}" -ne 1 ]]; then
        echo "Refusing to start: cargo locks the target directory, so this gate would" >&2
        echo "block rather than run. These may belong to someone else — DO NOT kill them." >&2
        echo "Wait for them, or re-run with --allow-contention." >&2
        exit 3
    fi
    echo "proceeding anyway (--allow-contention)." >&2
elif [[ "${other_builds}" -gt 0 ]]; then
    # Worth saying, not worth refusing over: these cost wall-clock, not
    # correctness, and a slow gate is still a valid gate.
    echo "note: ${other_builds} unrelated cargo/rustc process(es) running in other" >&2
    echo "      target directories. They compete for CPU only; proceeding." >&2
fi

# --- Lane runner. Condition 3: native status, written last. ----------------
lane_names=()
lane_cmds=()
lane_exits=()

run_lane() {
    local name="$1"; shift
    local log="${artifacts}/${name}.txt"
    echo "==> ${name}: $*"
    "$@" > "${log}" 2>&1
    local rc=$?
    # The marker goes in AFTER the command terminates. If it is missing, the
    # run was interrupted and the lane is `lost`, not green.
    printf 'LANE_EXIT=%s\n' "${rc}" >> "${log}"
    lane_names+=("${name}")
    lane_cmds+=("$*")
    lane_exits+=("${rc}")
    echo "    exit=${rc}  log=${log}"
    return 0
}

# L1-L2: cheap and fast, so they run first and fail early.
run_lane fmt    cargo fmt --all -- --check
run_lane clippy cargo clippy --all-targets --all-features -- -D warnings

# L2b: clippy on the minimal profile. Added 2026-09-03: this lane was being run
# by hand at every gate but was never in the recipe, so the recipe did not
# reproduce the gate — AD-0070 condition 1. The minimal profile is exactly where
# dangling items hide, because nothing else builds it.
run_lane clippy-no-default-features cargo clippy --all-targets --no-default-features -- -D warnings

# L3: the full profile. This is the lane the project has historically run.
run_lane test-all-features cargo test --all-features -- --test-threads=4

# L4: the other side of every default-feature flip. Cheap insurance against
# shipping a default change that breaks the minimal build.
run_lane test-no-default-features cargo test --no-default-features -- --test-threads=4

# L4b: one AD 0058 format feature on its own. L3 and L4 cover only the two
# extremes — everything on, everything off — and a feature can be wired to
# nothing and still pass both: with it on, the always-compiled code carries the
# tests; with it off, the tests are cfg'd away. This lane is the one that fails
# if `sevenzip` stops actually selecting the backend.
run_lane test-sevenzip-only \
    cargo test --no-default-features --features sevenzip -- --test-threads=4
run_lane test-zip-crypto-only \
    cargo test --no-default-features --features zip-crypto -- --test-threads=4
# `libarchive` is the odd one of the three: it adds no Cargo dependency at all,
# so a dependency-tree diff cannot see it. What it gates is the build-script
# probe for the system C library — the thing that makes this crate unbuildable
# on a host without libarchive installed. This lane is what fails if the
# `libarchive` feature stops selecting the backend, since L4 (everything off)
# would still be green with the backend wired to nothing.
run_lane test-libarchive-only \
    cargo test --no-default-features --features libarchive -- --test-threads=4
# `sfx` and `rar-support` complete the per-feature set, and the reason every
# format feature needs its OWN lane is not symmetry — it is that one feature's
# code can reference another feature's module. `Archive::first_rar_signature_before`
# is gated on `rar-support` and calls `crate::sfx::signatures`, so a build with
# rar on and sfx off did not compile. Neither extreme lane sees that: with
# everything on it compiles, and with everything off both sides are gone. It was
# found by sweeping combinations, and a `rar-support`-only lane is what keeps it
# found (AD 0058).
run_lane test-sfx-only \
    cargo test --no-default-features --features sfx -- --test-threads=4
run_lane test-rar-support-only \
    cargo test --no-default-features --features rar-support -- --test-threads=4

# L5: default features as a consumer gets them, compile-only. Links no test
# binaries, so it cannot hit the first-exec admission stall.
run_lane check-default cargo check --all-targets

# L6: Windows cfg-compile. OWNER-INVOKED ONLY — see --with-windows-check.
if [[ "${with_windows}" -eq 1 ]]; then
    run_lane check-windows-cfg \
        cargo check --target x86_64-pc-windows-msvc --no-default-features --features external-rar-create
fi

# L7: Linux cross-compile check. Only once OI-0080-001 is unparked.
if [[ "${with_cross}" -eq 1 ]]; then
    run_lane check-linux-musl cargo check --target x86_64-unknown-linux-musl --no-default-features
fi

# --- Record. --------------------------------------------------------------
gate_exit=0
{
    echo '## Lanes'
    echo
    echo '| lane | command | exit | result |'
    echo '|---|---|---|---|'
    for i in "${!lane_names[@]}"; do
        rc="${lane_exits[$i]}"
        [[ "${rc}" -ne 0 ]] && gate_exit=1
        totals="$(grep -h '^test result' "${artifacts}/${lane_names[$i]}.txt" 2>/dev/null \
            | awk '{p+=$4; f+=$6; ig+=$8; n++} END {if (n) printf "%d suites, %d passed, %d failed, %d ignored", n, p, f, ig; else print "—"}')"
        printf '| `%s` | `%s` | %s | %s |\n' "${lane_names[$i]}" "${lane_cmds[$i]}" "${rc}" "${totals}"
    done
    echo
    echo '## Not run'
    echo
    [[ "${with_windows}" -ne 1 ]] && echo '- `check-windows-cfg` — not run (owner-invoked only; 2026-09-01 hold).'
    [[ "${with_cross}" -ne 1 ]]   && echo '- `check-linux-musl` — not run (OI-0080-001 parked pre-v2).'
    echo '- Windows test run — **not runnable on this host.** Needs a Windows machine (AD-0070).'
    echo '- Linux test run — **not runnable on this host.** Needs a Linux host or container (AD-0070).'
    echo
    echo "- finished: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- artifacts: \`${artifacts}\`"
} >> "${summary}"

echo
echo "summary: ${summary}"
echo "GATE_EXIT=${gate_exit}"
exit "${gate_exit}"
