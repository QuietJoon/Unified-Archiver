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
    echo "- target dir (as cargo reports it, never set by this script): \`$(cargo metadata --format-version 1 --no-deps 2>/dev/null | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')\`"
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
foreign="$(pgrep -x cargo 2>/dev/null | wc -l | tr -d ' ')"
foreign_rustc="$(pgrep -x rustc 2>/dev/null | wc -l | tr -d ' ')"
if [[ "${foreign}" -gt 0 || "${foreign_rustc}" -gt 0 ]]; then
    echo "contention: ${foreign} cargo, ${foreign_rustc} rustc already running." >&2
    if [[ "${allow_contention}" -ne 1 ]]; then
        echo "Refusing to start. These may belong to someone else — DO NOT kill them." >&2
        echo "Wait, or re-run with --allow-contention and expect a much slower gate." >&2
        exit 3
    fi
    echo "proceeding anyway (--allow-contention)." >&2
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

# L3: the full profile. This is the lane the project has historically run.
run_lane test-all-features cargo test --all-features -- --test-threads=4

# L4: the other side of every default-feature flip. Cheap insurance against
# shipping a default change that breaks the minimal build.
run_lane test-no-default-features cargo test --no-default-features -- --test-threads=4

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
