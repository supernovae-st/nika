# Audit run 3 · the agent-loop budget + MCP pricing (BREADTH · first sweep)

> Protocol: `docs/plans/adversarial-audit-protocol.md`. Runs 1-2:
> `2026-07-28-verdict-coverage.md` (verdict/gate · 12 reproduced) ·
> `2026-07-29-audit-run-2-trace-chain.md` (trace/attestation · 1 P0).
> This run is a NEW domain (§7b #3) — it measures BREADTH, and its count
> must never be pooled with any other run's. Domain #2 (composition) was
> swept and closed earlier the same day; the protocol's own reason these
> two halves share one domain: « auditing these apart would miss the
> composition, which is where the damage is ».

## The five declared lines (written before the first probe · unedited after)

```
DOMAIN      the agent-loop budget + MCP pricing — what the loop's token
            budgets (turns · max_tokens_total) and the static cost ceiling
            actually bound for tool-fed transcripts, and what `check`
            claims about both (one seam: a tool result's size becomes
            prompt becomes dollars, adversarially reachable)
ORACLE      /opt/homebrew/bin/nika (0.106.1 · the pushed tree, Cellar
            payload swapped) · discriminating probe in §0 (a tool result
            that outgrows the declared budget MUST stop the loop loudly)
SURFACES    nika-verb-agent: lib.rs (the loop + accounting) · guard.rs ·
            errors.rs (464 verdicts) · router.rs · observe.rs ·
            nika-check/src/cost.rs (the agent ceiling arm) ·
            nika-providers usage parsing (who reports input_tokens and
            who does not) · the MCP path (nika-mcp sandbox/pin — tool
            result shape only, not its security seam, which is its own
            domain)
DENOMINATOR solo session · 10 probes planned (declared up front) · ~4h
EXCLUDED    the MCP security seam (sandbox · anti-rug-pull pins — ADR-080,
            its own domain) · the composed-workflow budget (run 2's
            session arc closed it: NIKA-1709 + composed pricing) · the
            trace/cost-replay leg (run 2's domain) · provider PRICING
            tables themselves (the catalog's factual rows, not the seam)
```

## The three load-bearing questions (the domain's substance)

1. **The ceiling's input blindness** — `cost.rs` prices the agent arm at
   `max_tokens_total` (output-side). A tool-fed loop's INPUT tokens
   outgrow output by the transcript factor (§7b: a conforming-but-
   malicious server inflated cost 658× measured by swarm 5599). What
   does `check` claim about an agent task's total exposure?
2. **The result-size guard** — is there a per-tool-result size cap
   before a result enters the transcript (guard.rs)? A 1 MiB result ×
   10 turns at input prices is adversarially-reachable money.
3. **The usage-absence meter** — the budget's `total_tokens` fold rides
   `response.usage.input_tokens + output_tokens`. Providers that report
   NO usage (or partial) — does the loop fail CLOSED (refuse/loud) or
   admit an invisible spend under a budget (fail-open)?

## §0 · Oracle calibration (protocol §0 — BEFORE any finding probe)

A continuing agent loop (stub: turn 1 tool_call + usage 900/900 → turn 2
final text) under `max_tokens_total: 1`, both directions on the oracle:

```
ARM A · usage reported   ✖ NIKA-AGENT-002 · agent exhausted
                         max_tokens_total (1800 spent) · $0.002025 recorded
ARM B · usage OMITTED    ✔ think agent · 2 turns · green, $0.00 recorded
                         (the budget never saw the 1,800 billed tokens)
```

**The fail-open discriminates both directions** (§4② satisfied). Oracle:
the installed daily binary, this session's pushed tree.

## Findings

| ID | Class | Sev | State | Seam | One line |
|---|---|---|---|---|---|
| **R3-F1** | **fail-open** (2nd face false-green) | **P0** | reproduced · **fixed this run** | `nika-kernel-ai` `InferResponse.usage_reported` + the two gates (`nika-verb-agent` `infer_turn` · `nika-verb-infer` `run`) | a billed backend that omits the usage block reads (0,0) to every budget and ledger — a 2-turn loop completed green under `max_tokens_total: 1` over 1,800 billed tokens, and the receipt recorded $0.00 |
| R3-Q1 | — | — | **not a defect** (already honest) | `check` COST line | the agent ceiling already narrows its claim: `worst-case OUTPUT ceiling · prompts … unpriced` — the input-blindness is declared, not hidden (measured) |
| R3-Q2 | — | — | **symptom** of R3-F1 | the transcript feed | no per-result size cap exists (draft 256 KiB · router query 4096 chars · schema render are capped — results are not), but a result MUST ride (the loop's job); the bound for it was the token budget, which R3-F1 blinded — closed by the same gate |

```
COUNT   1 reproduced defect requiring its own repair   ← run 3's headline
        + 1 symptom (Q2) · + 0 reported-unreproduced · + 0 regressions
EFFORT  solo session · 10 probes planned → 7 executed (3 folded) · ~3h
```

**Class histogram**: fail-open 1 (P0). Q1 verified already-honest (the
narrowing is on the line); Q2 recorded as symptom, closed by the fix.

## Probe log (verbatim outputs · each claim MEASURED, §4)

**P1 · the accounting reads usage at all** — `infer_turn` folds
`response.usage.input_tokens + output_tokens` into the budget (lib.rs:
648-652), and the budget gate `total_tokens >= max_tokens_total` sits at
the Dispatch continuation (lib.rs:1339). The meter EXISTS and counts
input — when it is reported.

**P2 · the omission reads as zero by construction** — `u64_at` returns 0
for an absent pointer (wire/mod.rs:173); every wire builds
`TokenUsage::new(presence_or_zero)`; NO layer distinguishes "absent"
from "free" (grep: the only "no usage" mentions are the failing-call
case and the mock's honest zero). A success with omitted usage ≡ $0.00.

**P3 · the documented carve-out is scoped to local/mock** — the NIKA-1704
explain + the preflight warning both say "the budget bounds METERED
spend — local/mock work never trips it". An openai+base_url backend
omitting usage is NOT in that class — it is metered-class spend the
budget cannot see.

**P4 · the Terminal-1 posture is documented, not a defect** — "budgets
stop CONTINUING, they don't fail a finished run" (classify_turn:1281):
a 1-turn over-budget completion is by design. The domain's real seam is
the CONTINUATION gate — where P1's gate lives.

**P5 · the loop discriminates (the finding, above)** — ARM A fires
NIKA-AGENT-002 (budget honored with usage) · ARM B completes green with
$0.00 recorded (fail-open reproduced).

**P6 · Q1 already narrows honestly** — `check agent2.nika.yaml`:
`COST $0.0000 – $0.0000 worst-case output ceiling · prompts … unpriced`
— the OUTPUT-only claim is on the line, the prompt gap is named.

**P7 · Q2 structural** — the transcript feed carries results of any
size (no cap found: draft · router query · schema render are the only
caps in the loop) — and a result MUST ride; the bound was the budget.

## The repair (this run)

- `InferResponse.usage_reported` (default `true`; the wires clear it
  when the backend omits usage OR sends an empty usage object — an
  empty object carries no signal, same class as the omission).
  anthropic / openai_compat / gemini mark presence; the mock keeps
  `true` (its zero is a TRUE zero, the documented carve-out).
- **The agent-loop gate** (`infer_turn`): `!usage_reported &&
  catalog-priced(model)` → `NIKA-AGENT-005` (fail-closed · NIKA_469 ·
  budget_error, never transient).
- **The ledger leg** (`nika-verb-infer`): the same predicate →
  `NIKA-INFER-003` (fail-closed · NIKA_434) — a run receipt never
  records $0 for a billed call.
- The fixture class `"usage":{}` (12 wire fixtures relying on the old
  empty-object-means-zero shape) was given real numbers; one spend
  assertion flipped to the honest polarity (`has_signal`).
- Spec `05-errors.md` + canon.yaml rows for both codes (source + mirror
  in sync).

**Verified on the oracle (post-fix)**:

```
ARM A · usage reported   ✖ NIKA-AGENT-002 (unchanged — the budget owns
                         its case: 1800 ≥ 1, $0.002025 recorded)
ARM B · usage OMITTED    ✖ NIKA-AGENT-005 · the provider reported no
                         token usage for priced model `openai/gpt-5-mini`
                         — the budget cannot meter this call (fail-closed)
infer · usage OMITTED    ✖ NIKA-INFER-003 · the ledger cannot bill this
                         call honestly (fail-closed)
mock/echo (carve-out)    ✔ green — the documented unmetered class
```

Unit locks: `an_omitting_backend_on_a_priced_model_fails_closed` +
`an_unreported_zero_on_an_unpriced_model_proceeds` (verb-agent) · the
wire marker presence tests (providers) · 93 agent + 67 infer + 150
providers + 65 kernel-ai + 57 error tests.
