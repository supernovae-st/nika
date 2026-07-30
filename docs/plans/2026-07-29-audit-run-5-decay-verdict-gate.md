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
| **F2 · SEC-009 perverse incentive** | native fetch→write→exec-read vs exec curl | **FIXED after the run (operator decision A · 2026-07-30)** | the law moved: `exec` is born-ingress (`content_flow.rs` · the runtime twin `integrity.rs` · leg ② counts the exec permit as an ingress channel). Measured on this run's own fixtures: exec-curl 0→2 SEC-009, native 1→1 unchanged. NEP-0002 amended v2.2 + the spec corpus/example follow (see « The repair ») |
| **F3 · TYPES vacuity** | `.total_usd` from `nika:inspect view: cost` (shapeless output) | **FIXED after the run (decision C · the A half)** | the verdict names its blind spot per file: `✔ TYPES … · 1 ref into unshaped task outputs are unverifiable — the run judges them (tasks.bill.output.total_usd (via with.bill))` — the `with:`-alias hop counted too. The B half (builtin `returns:` shapes) stays the named arc |
| **F4 · --answer headless** | `run --answer confirm=true` (no --resume) | **FIXED after the run** | `--answer` pre-seeds the gate map on a fresh run — the CI one-pass completes (2/2 done · the gate consumes the queued answer); an unknown task id still refuses at admission |
| **F14 · edit hook** | `NIKA="${NIKA_BIN:-nika}"` | **FIXED after the run** | `check-on-edit.sh` resolves the edited file's tree build (`target/debug/nika-cli`) by default when the tree builds its own nika — `NIKA_BIN` still wins, PATH is the fallback outside any nika tree (3 configurations proven) |
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

after the operator's four decisions (2026-07-30):
                       0 recurring standing — F2 · F3-A · F4 · F14 all
                       fixed and proven on this run's own fixtures
                       (F3's B half — builtin `returns:` shapes — stays
                       the named architectural arc)
                       
7 of 12 verified CLOSED on today's oracle — each with its discriminating
probe re-run green (F11 · F8-law · F13 · F1-contract · F7-narrowing ·
F15-parity · F5-causal-order).
```

**The total falls — and this time the severity histogram moved.** By
the protocol's own caveat (a systematic sweep finds more per hour, so
flat = decay), a fall to ~40 % said the class was being shed; the
post-run repair arc then took the recurring mass to ZERO — every one of
the four is fixed and re-proven on this run's own fixtures. The two P0s
turned out to be exactly what the sweep said they were: modeling
decisions, not bugs — F2 died the day the operator picked « the
subprocess is a trust boundary » (one law row + its runtime twin + the
grant twin), F3's vacuity died by narrowing (the verdict names its
blind spot per file). The regeneration question is now the run #6
question: does the false-green/fail-open family FIND new seams (#5's
never-audited surfaces), or is the class actually tarred?

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

**F2 — the exec verb is born-ingress (shipped after the run, operator
decision A):** three rows of one law, check≡run — `content_flow.rs`'s
classify arm (`exec → (true, …)`), the runtime F-O1 twin
(`integrity.rs::born_untrusted`), and the grant twin (`nika_cap`'s leg
② counts `permits.exec` as an ingress channel). Measured on this run's
own fixtures: the exec-curl shape goes 0→2 SEC-009 (the escape dies),
the native shape stays 1→1 (no collateral). NEP-0002 amended to v2.2
(governance + `05-errors.md` + the reference `trifecta_core.py` — whose
`with:`-alias/`for_each` sweep gap the new law exposed and which was
fixed to mirror the engine, differential back to 49/49), and the
spec's own showcase `pr-review-fanout` took the canonical early gate
(dominance is structural: a sibling `todo_sweep` branch bypassing the
prompt re-opened the finding until it too descended from it).

**The coherence tail F2 owed:** the spec corpus's
`trifecta-realized-flow-ungated` fixture (fetch → `nika:notify` with a
`target:` and NO `channel:`) passed the ENGINE clean while the
reference core refused it — `builtin_effect` judged notify net ONLY on
a literal `channel: webhook`, while the def's own contract makes
webhook the DEFAULT channel (`target` IS "the webhook URL"). The
absent-channel case now classifies net (a present-but-templated channel
stays unclassifiable, never a default). One false-green fewer, pinned
by the corpus fixture itself.

**One wave-order observation (recorded, not fixed):** the file channel
(a tainted writer → a later exec reader under `fs.read`) reads the
sweep's declaration order WITHIN a wave — a same-wave writer→reader
pair through a file with no DAG edge lands on whichever side the
declaration order picks (the native F2 shape's `render` task sits on
the unflagged side). The wave-frozen view is the declared
approximation; naming the seam is this run's honesty, judging it is a
future sweep's.

## Probe log (verbatim outputs · each claim MEASURED, §4)

(verbatim excerpts inline per row above — full fixtures + logs under
`engine/target/audit5/` (scratch, re-creatable from this file's rows).)

## Follow-on (named, not hidden)

- F3's B half — builtin output shapes (the stdlib's `returns:` surface)
  make the TYPES verdict CAPABLE, not just honest — architectural,
  named (the operator's decision C).
- The wave-order seam above (the file channel's same-wave
  writer→reader read) — recorded, its judgment belongs to a future run.
- Run #5 of the protocol — the never-audited surfaces — is the sweep
  that measures whether this arc tarred the false-green family or
  moved it (the operator picked « after F2/F3 land »).
