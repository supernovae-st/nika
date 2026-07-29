# Audit run 2 · the trace / attestation chain (BREADTH · first sweep)

> Protocol: `docs/plans/adversarial-audit-protocol.md`. Run 1 =
> `2026-07-28-verdict-coverage.md` (verdict/gate domain · 12 reproduced).
> This run is a NEW domain — it measures BREADTH, and its count must never
> be pooled with run 1's (§1 « never pooled »). Run 2's domain was ranked
> #1 of five in §7b: the only domain whose fail-open invalidates the other
> audits. Domain #2 (composition/budget) was swept and closed earlier the
> same day (the §4.1 arc: NIKA-1709 + composed pricing + amplification).

## The five declared lines (written before the first probe · unedited after)

```
DOMAIN      the trace/attestation chain — what `nika trace verify` and the
            sha256 chain prove about a run journal, and whether they observe
            it (one seam: what the chain attests vs what happened)
ORACLE      /opt/homebrew/bin/nika (0.106.1 + this session's fixes, swapped
            Cellar payload · the daily binary IS the tree) · discriminating
            probe in §0 (a tampered middle line MUST break the walk)
SURFACES    nika-dap: chain.rs (the walk) · journal.rs (the sink's chain
            write) · seal.rs · recover.rs · replay.rs · reproduce.rs ·
            cost_replay.rs · retention.rs · quarantine.rs ·
            nika-cli: verbs/trace_verify.rs · verbs/trace/* (store · manage ·
            retention · action) · the golden trace fixtures (what they
            actually compare — §7b's never-read question)
DENOMINATOR solo session · 12 probes planned (declared up front) · ~4h
EXCLUDED    the OTLP projection (a read-only lens, no attestation claim of
            its own) · resume/--resume (its own lifecycle domain, ADR-099) ·
            fs/net/secrets permit seams (run 1 + this session's arcs) ·
            MCP/agent-loop (domain #3) · the trace RENDER (peek/show UX —
            cosmetic unless it misattests)
```

## §0 · Oracle calibration (protocol §0 — BEFORE any finding probe)

The probe that must discriminate: a journal whose line-3 `chain` field is
edited after the fact MUST yield `Broken{line:3}`, and the untampered
original MUST yield `Intact` — the same walk, both directions (§4②).

(result recorded below, verbatim)

```
$ nika trace verify <intact.ndjson>
OK — 7 events · chain intact · head 44c75042be4f141d…
  internally consistent (tamper-evident, not tamper-proof) — compare the head
  against the one the run printed to close the loop

$ nika trace verify tampered.ndjson        # line-3 `chain` forged to 00…00
BROKEN at line 3 — recorded chain 0000000000000000 · computed fbb97c8577d4f261
  every line from here on is unverified (edited, inserted, dropped or reordered)
```

**The walk discriminates both directions** (§4② satisfied). Oracle: the
installed daily binary, this session's tree.

## Findings

| ID | Class | Sev | State | Seam | One line |
|---|---|---|---|---|---|
| **R2-F1** | **fail-open** (2nd face false-green) | **P0** | reproduced · **fixed this run** | `nika-dap/src/anchor/tier.rs` `evaluate()` — the `SealTier::Unsealed` early return | `--anchored` on an UNSEALED journal exited 0 with the requirement never evaluated (missing sidecar rc=0 · forged sidecar rc=0), while the sealed path honors 3/2 exactly |

```
COUNT   1 reproduced defect requiring its own repair   ← run 2's headline
        + 0 symptoms · + 0 reported-unreproduced · + 0 regressions
EFFORT  solo session · 12 probes (as declared) · ~3h
```

**Class histogram**: fail-open 1 (P0). No false-green, no fail-closed, no
contradiction, no teaching defect. The domain's attestation core is
healthy: 11 of 12 probes returned the honest verdict (see the log).

**Regression note.** None of run 1's seams recurs here — and the day's
own §4.1 fix was re-probed under this run's oracle before the record
closed (parent-under-budget refusal: holds).

## Probe log (verbatim outputs · each claim MEASURED, §4)

**P1 · the four tamper classes + legacy + truncation** — each breaks at
the right line; a pre-chain journal never reads as verified; a
retention-style truncation from the top breaks at line 1, loudly:

```
p1a-edit      BROKEN at line 5  (payload byte edited in line 4 — the NEXT
              line's chain no longer recomputes · the attestation boundary)
p1b-drop      BROKEN at line 4
p1c-insert    BROKEN at line 4  (forged line with a copied chain field)
p1d-reorder   BROKEN at line 4
p5-unchained  nika: unchained — predates the chain (pre-0.96 journal):
              nothing to verify, nothing to distrust      (never "Intact")
p7-truncated  BROKEN at line 1  (first 2 lines dropped — never re-genesis)
```

**P2 · chain write ≡ chain walk** — the head the run prints IS the head
the verifier recomputes (byte-equal, 684e4023…). The sink and the walk
share one genesis tag and one primitive, and it shows.

**P3 · corruption taxonomy stays three-way distinct** — a garbage LAST
line is TORN ("a crash mid-write, not tampering — the chain covers every
complete line"); a garbage MIDDLE line is `not a journal — the line is
not valid JSON` (never mis-filed as torn); an edited middle is BROKEN.

**P4 · a killed run attests INCOMPLETE, never Intact** — `kill -9` at
2 events: "the journal never reached a terminal frame (no
workflow_completed · failed · paused · cancelled · run_sealed) — the run
was killed or crashed: the chain attests every complete line; the
lifecycle end is unattested". The attestation rides the verifier.

**P6 · the DoS bound holds** — an 11 535 436-byte line: "beyond the
verifier's line bound (1048576 bytes)".

**P8 · the goldens read** — `spec/conformance/tests/runtime/trace/` is a
3-fixture corpus (`001 clean · 002 finding · 003 forged`), each
`expected-verify.json` READ from the fixture and replayed through the
real `trace verify` (`trace_witness_conformance.rs`). Class-level
pins, never byte-level — honest shape; THIN corpus (no torn / incomplete
/ oversized / unchained fixture — those classes are unit-covered in
chain.rs, not conformance-covered. Observation, not a finding).

**P10 · the documented boundary holds — and the ladder works where it
claims** — a forged + RE-CHAINED journal passes the walk (`OK — chain
intact`), exactly as the output itself declares ("tamper-evident, not
tamper-proof — compare the head to close the loop"); the forged head
differs from the run's (19be6160… vs 684e4023…), so the loop-closer
catches it. On the SEALED path the anchor contract is honored byte for
byte: missing sidecar → rc=3 "REQUIRED but no sidecar exists" · real
sidecar → rc=0 "checkpoint + inclusion proof verified offline" · forged
sidecar → rc=2 "ANCHOR FORGED … reported tier: SEALED (the anchor
vouches for nothing)" — demotion honesty included.

**P11 · reproduce is verify-before-trust, and the comparison catches
content lies** — a forged-output (re-chained) journal against a fresh
run: rc=2 "DIVERGED — NONDETERMINISTIC a — same def, same inputs,
different output". A broken-chain journal against a fresh run: rc=0
REPRODUCED but with the WARNING first ("the recorded journal fails
verification (chain broken at line 3); its claims are unverified").

**P12 · cost-replay re-judges** — a journal whose per-frame `$0.500000`
disagrees with its journaled `$0.400000` total: "totals: DIVERGES —
re-summed $0.500000 vs journaled $0.400000 (the journal's cost story
does not re-judge)" · exit 0 by the stated-verdict posture (F-P18 law).

**R2-F1's repro (the one dishonest verdict)** — BEFORE the fix:

```
$ nika trace verify --anchored real.ndjson        # unsealed, no sidecar
OK — 7 events · chain intact · …                  rc=0   (contract: rc=3)
$ nika trace verify --anchored forged.ndjson      # unsealed, forged sidecar
OK — 7 events · chain intact · …                  rc=0   (contract: rc=2)
```

AFTER (same run, tree binary):

```
$ nika trace verify --anchored real.ndjson
ANCHORED — REQUIRED but the journal is unsealed (no run_sealed line):
  the anchor verifies against the seal's key, so the tier is unattainable
                                                      rc=3   ✓
$ nika trace verify real.ndjson                     (no flag — unchanged)
OK — 7 events · chain intact · …                    rc=0   ✓
$ nika trace verify --key run-signing.pub --anchored sealed.ndjson
ANCHORED — rekor index 34612959 · checkpoint + inclusion proof verified
  offline                                           rc=0   ✓ (sealed path kept)
```

## The repair (this run)

`nika-dap/src/anchor/tier.rs` `evaluate()`: the `SealTier::Unsealed` early
return now consults `require_anchor` — a required anchor on an unsealed
journal is the ENV refusal (the tier needs a seal to build on), named in
plain words, with the unflagged path byte-identical. Tests:
`tier.rs::an_unsealed_journal_refuses_a_required_anchor_loudly` +
`trace_verify.rs::a_required_anchor_on_an_unsealed_journal_is_env`.
