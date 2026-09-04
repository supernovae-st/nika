#!/usr/bin/env bash
# run-ci-ratchets.sh — Tier 2 pre-push, engine context
#
# Runs the CI ratchets from scripts/ci/ in parallel (mirrors diamond-ci.yml
# matrix). Collects failures and reports them together rather than bailing
# at the first failure, so the user sees all failures in one push attempt.
#
# The list lives in the RATCHETS array below and NOWHERE else. This header
# used to carry a second copy that had drifted out of agreement with it:
# it said "all 9", listed `tests` — which is explicitly NOT run here — and
# never named `credential-headers`, which is. A prose list beside the real
# one is a claim nothing checks.
#
# Note: check-tests.sh is deliberately not in that array (cargo test runs
# separately at pre-push to keep the --lib flag and avoid --test, which
# triggers the keychain popup on macOS).
#
# Exit: 0 = all pass | 1 = one or more failed
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
CI_DIR="${SCRIPT_DIR}/../ci"

readonly RATCHETS=(
  'loc-limits'
  'crate-size'
  'fn-length'
  'unwrap'
  'expect'
  'dead-code'
  'no-default-features'
  'adr-coverage'
  'credential-headers'
  'version-uniform'
  # One file per change under changelog.d/, never a bullet hand-written into
  # `## [Unreleased]` — the shared append target that collided four PRs on
  # 2026-08-24 with zero source overlap between them.
  'changelog-fragments'
  # One executable identity (ADR-135): the bin target, the release line, the
  # flake and the tests all say `nika`, and the gate proves itself first.
  'public-binary'
)

# Four of the ratchets above (unwrap · expect · dead-code, plus hygiene's
# error-one-voice) do not read source files directly — they read what
# `strip_test_items` hands them. A bug in that filter does not make them
# fail; it makes them pass QUIETLY. So the filter proves itself honest
# BEFORE anything trusts its verdict (2026-08-02: one brace in a string
# blanked the rest of a file, hiding a real production `.unwrap()`).
if ! bash "${CI_DIR}/test-strip-test-items.sh" >/dev/null 2>&1; then
  printf '[ci-ratchets] FAIL — the shared test-item filter is not honest:\n' >&2
  bash "${CI_DIR}/test-strip-test-items.sh" >&2 || true
  printf '[ci-ratchets] the unwrap · expect · dead-code verdicts below would be meaningless\n' >&2
  exit 1
fi

# Pre-flight: every DECLARED ratchet must be runnable before any of them
# runs. This used to be decided per-ratchet inside the launch loop, where a
# missing — or merely non-executable — script printed a WARNING, hit
# `continue`, and dropped out of RATCHET_NAMES. Since the green line below
# counts RATCHET_NAMES, the denominator moved with it: `chmod 644` on one
# file turned "all 9 ratchets passed" into "all 8 ratchets passed", exit 0,
# and the push went through. The count was arithmetically true the whole
# time, which is exactly why nobody would catch it.
#
# `-x` is the right test here — line ~52 invokes "$script" directly, so the
# exec bit is genuinely required — it was the VERDICT that was wrong. The
# fail-closed shape is the one this file already uses above for the filter
# self-test: a dependency it cannot run means it cannot judge.
UNRUNNABLE=()
for ratchet in "${RATCHETS[@]}"; do
  [[ -x "${CI_DIR}/check-${ratchet}.sh" ]] || UNRUNNABLE+=("$ratchet")
done
if ((${#UNRUNNABLE[@]} > 0)); then
  printf '[ci-ratchets] FAIL — %d of %d declared ratchets cannot run:\n' \
    "${#UNRUNNABLE[@]}" "${#RATCHETS[@]}" >&2
  for ratchet in "${UNRUNNABLE[@]}"; do
    printf '  %-22s %s (missing or not executable)\n' \
      "$ratchet" "${CI_DIR}/check-${ratchet}.sh" >&2
  done
  printf '[ci-ratchets] a ratchet that did not run has not passed\n' >&2
  exit 1
fi

TMPDIR_BASE="$(mktemp -d)"
trap 'rm -rf -- "$TMPDIR_BASE"' EXIT

PIDS=()
RATCHET_NAMES=()

# ---------------------------------------------------------------------------
# Launch all ratchets in parallel
# ---------------------------------------------------------------------------
for ratchet in "${RATCHETS[@]}"; do
  script="${CI_DIR}/check-${ratchet}.sh"
  out_file="${TMPDIR_BASE}/${ratchet}.out"
  # P1-2 Batch H+: the subshell inherits `set -e` from the parent, so if
  # "$script" exits non-zero the subshell terminates before writing .exit.
  # Fix: capture exit code explicitly with `|| rc=$?` to prevent errexit
  # from short-circuiting.
  (
    rc=0
    "$script" >"$out_file" 2>&1 || rc=$?
    printf '%d' "$rc" >"${out_file}.exit"
  ) &
  PIDS+=($!)
  RATCHET_NAMES+=("$ratchet")
done

# ---------------------------------------------------------------------------
# Wait for all and collect results
# ---------------------------------------------------------------------------
# Wait for all background jobs (Bash 4.3+ wait -n not needed here; we track PIDs)
for pid in "${PIDS[@]}"; do
  wait "$pid" 2>/dev/null || true # failures are captured via .exit files
done

FAILED=()
for ratchet in "${RATCHET_NAMES[@]}"; do
  out_file="${TMPDIR_BASE}/${ratchet}.out"
  exit_file="${out_file}.exit"
  rc=0
  [[ -f "$exit_file" ]] && rc="$(cat "$exit_file")"
  if ((rc != 0)); then
    FAILED+=("$ratchet")
    printf '\n[ci-ratchets] FAILED: %s\n' "$ratchet" >&2
    cat "$out_file" >&2
  fi
done

if ((${#FAILED[@]} > 0)); then
  printf '\n[ci-ratchets] %d ratchet(s) failed: %s\n' "${#FAILED[@]}" "${FAILED[*]}" >&2
  printf 'Run individually: bash scripts/ci/check-<name>.sh\n' >&2
  exit 1
fi

# The green states the DECLARED count and asserts the attempted count
# matches it. Belt and braces after the pre-flight above: the number a gate
# reports must never be one it derived from what happened to run.
if ((${#RATCHET_NAMES[@]} != ${#RATCHETS[@]})); then
  printf '[ci-ratchets] FAIL — %d of %d declared ratchets were attempted\n' \
    "${#RATCHET_NAMES[@]}" "${#RATCHETS[@]}" >&2
  exit 1
fi
printf '[ci-ratchets] all %d ratchets passed\n' "${#RATCHETS[@]}" >&2
exit 0
