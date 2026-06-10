#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 39: Gate-5 (mutation) attestation — no admitted crate DEFERS it.
#
# ADR-003 Gate 5 = "MUTATION ≥90% killed (or documented exemption)". An admitted
# crate has, by definition, PASSED all 12 gates — so no gate should still read
# "⏳ deferred / pending". The slow per-crate enforcer is
# scripts/ci/check-mutation-floor.sh (admission/nightly tier · cargo-mutants is
# minutes-slow); this FAST vector is its structural complement: it parses each
# admitted lib crate's spec and flags any whose Gate-5 row is still a DEFERRAL
# rather than a measured result or a documented exemption.
#
# This is the ADR-090 move: Gate 5 was enforced socially (check-mutation-floor
# was wired into no CI/hygiene run), so a crate admitted with "mutation pending"
# carried zero structural pressure. This vector projects the crate-spec Gate-5
# attestations and goes YELLOW until every admitted crate's Gate-5 is real.
#
# Attested (PASS) = the spec carries `<!-- GATE5-EXEMPT: N -->` (BUDGET mode) OR
# its Gate-5 table-row status is ✅ (a measured result). Deferred (FLAG) = the
# Gate-5 table-row status cell contains ⏳ / "deferred" / "pending". Crates with
# no Gate-5 table row (the kernel-split crates use "MUTATION inherited" prose,
# WIP crates aren't admitted) are not a deferral signal and are skipped.
#
# Exit: 0 green (no admitted crate defers Gate 5) · 1 yellow (some do · listed ·
# informational ratchet, does NOT gate the push) · 2 red (reserved · unused).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

# shellcheck source=scripts/ci/_lib.sh
. "$ENGINE_ROOT/scripts/ci/_lib.sh"
# _lib.sh sets `-e`; re-disable it — this warn-tier vector tolerates the
# non-zero greps (a crate with no Gate-5 row is a normal, handled case).
set +e

deferred=""
attested_n=0
checked_n=0

while IFS= read -r crate; do
  [ -z "$crate" ] && continue
  # Only admitted LIB crates (a binary-only crate has no public mutation surface
  # to attest the same way; WIP crates without src/lib.rs are not admitted).
  [ -f "crates/$crate/src/lib.rs" ] || continue
  spec="docs/crate-specs/$crate.md"
  [ -f "$spec" ] || continue

  checked_n=$((checked_n + 1))

  # A documented exemption (BUDGET mode marker) counts as attested.
  if grep -q 'GATE5-EXEMPT:' "$spec"; then
    attested_n=$((attested_n + 1))
    continue
  fi

  # The Gate-5 table row, in either spec format:
  #   `| 5 | MUTATION | <status> | …`   or   `| 5. MUTATION ≥ 90% | <status> | …`
  row="$(grep -iE '^\|[[:space:]]*5[.):[:space:]].*MUTATION' "$spec" | head -1)"
  if [ -z "$row" ]; then
    # No Gate-5 table row → not a deferral signal (inherited / metadata-only).
    continue
  fi

  # Deferral marker anywhere in the row → flag. (A measured row is `✅`.)
  if printf '%s' "$row" | grep -qiE '⏳|deferred|pending|TODO'; then
    deferred="$deferred $crate"
  else
    attested_n=$((attested_n + 1))
  fi
done < <(workspace_members)

if [ -n "${deferred# }" ]; then
  echo "RATCHET (${attested_n}/${checked_n} admitted crates attest Gate 5) · DEFERRED Gate-5 (measure → score, or add a GATE5-EXEMPT marker):"
  for c in $deferred; do
    echo "  · $c — run: bash scripts/ci/check-mutation-floor.sh $c"
  done
  exit 1
fi

echo "OK (all ${checked_n} admitted lib crates attest Gate 5 · measured score or documented exemption · no deferrals)"
exit 0
