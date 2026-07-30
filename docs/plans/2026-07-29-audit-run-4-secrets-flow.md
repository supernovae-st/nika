# Audit run 4 · the secrets flow / declassify seam (BREADTH · first sweep)

> Protocol: `docs/plans/adversarial-audit-protocol.md`. Runs 1-3:
> `2026-07-28-verdict-coverage.md` (12) · `2026-07-29-audit-run-2-trace-chain.md` (1 P0) ·
> `2026-07-29-audit-run-3-agent-loop-budget.md` (1 P0).
> This run is a NEW domain (§7b #4) — it measures BREADTH, and its count
> must never be pooled with any other run's. Fail-open here = a secret
> leaves, irreversibly.

## The five declared lines (written before the first probe · unedited after)

```
DOMAIN      the secrets flow + declassify seam — what the IFC taint pass
            (flow.rs) refuses as a leak, what the `egress:`/`declassify`
            clause sanctions, and whether check ≡ run on both directions
            (one seam: a secret value's path into an exec/invoke effect)
ORACLE      /opt/homebrew/bin/nika (0.106.1 · the pushed tree, Cellar
            payload swapped) · discriminating probe in §0 (an UNSANCTIONED
            exec touching secrets.X MUST be a blocking leak at check,
            a sanctioned egress MUST pass)
SURFACES    nika-check: secrets.rs (leak scan) · flow.rs (the Denning-
            lattice taint engine) · declass.rs (sanctioned egress · 3
            AND-composed layers) · content_flow.rs · spec 01-envelope
            §secrets/egress + 05-errors rows NIKA-SEC-00x · the runtime's
            RedactingSink (secret.rs scrub · run-side twin)
DENOMINATOR solo session · 10 probes planned (declared up front) · ~3h
EXCLUDED    MCP server-side secrecy (its own supply-chain domain) · the
            custody/signing plane (seal/anchor — run 2's domain) · the
            trifecta lane (run 1's domain, swept) · provider key
            RESOLUTION (env/config — operational, not a flow seam)
```

## The load-bearing questions

1. **check ≡ run on the leak** — a secret reaching an exec argv is
   refused at CHECK (NIKA-SEC flow) AND the run cannot ship it anyway
   (the RedactingSink scrubs — does it scrub ARGV-bound values or only
   output frames? an argv-embedded secret reaching a subprocess is out
   of the sink's reach — the whole reason leaks are BLOCKING).
2. **the sanctioned path teaches correctly** — an `egress:` clause that
   sanctions must hold all 3 layers (confidentiality · integrity ·
   capability) — and every doc/template says so (F13's class: the
   `egress:` ⊥ `permits.net.http` distinction taught wrong).
3. **the decidable "ANY host" question** (F13 follow-up) — an `egress:`
   clause naming a host while `permits.net.http` is EMPTY: statically a
   guaranteed-run-failure — does check say it today, or still silently
   pass a workflow that cannot run?

## §0 · Oracle calibration (protocol §0 — BEFORE any finding probe)

The discriminating pair on the oracle (verbatim):

```
A · unsanctioned exec touching secrets.api
    rc=2 · ✖ SECRETS leak into exec (task `send`) — secrets.api ·
           fix: sanction it — `egress: [{ to: "exec" }]` on `secrets.api`
B · egress: [{ to: "nika:fetch", host: "api.stripe.com" }] declared
    rc=0 · ✔ audited · 0 hints
```

**The seam discriminates both directions** (§4② satisfied). Oracle: the
installed daily binary, this session's pushed tree.

## Findings

| ID | Class | Sev | State | Seam | One line |
|---|---|---|---|---|---|
| **R4-F1** | **teaching defect** (2nd face misattribution) | **P1** | reproduced · **fixed this run** | `nika-check/src/declass.rs` (new `leak_reason`) · `secrets.rs` · `findings.rs` · `nika-display/src/check_render.rs` | a refused edge with an `egress:` already DECLARED was taught « sanction it — `egress: [{ to }]` » — the author is told to add what exists, and the actual missing layer (wrong sink · wrong host · capability/`permits.net.http`) goes unnamed. F13's « the egress: ⊥ permits.net.http distinction is being taught wrong », live |

```
COUNT   1 reproduced defect requiring its own repair   ← run 4's headline
        + 0 symptoms · + 0 reported-unreproduced · + 0 regressions
EFFORT  solo session · 10 probes planned → 8 executed · ~2.5h
```

**The domain's enforcement core is healthy** — every other arm returned
the honest verdict: unsanctioned leak blocks at check (NIKA-SEC-006 with
the taint path) · the 3-layer sanction clears exactly its shape ·
`host_from_self` sanctions the self-URL form · a second co-occurring
secret stays refused (non-occlusion) · an infer/agent prompt without an
egress clause leaks (the BUG#3 provider-egress arm) · an unsanctioned
`on_finally` cleanup leaks past a sanctioned one.

## Probe log (verbatim outputs · each claim MEASURED, §4)

```
A · unsanctioned exec leak          ✖ SECRETS leak into exec (task `send`)
                                      · fix: sanction it (NIKA-SEC-006)
B · 3-layer sanctioned egress       ✔ audited · 0 hints
D · host_from_self (secret IS url)  ✔ SECRETS no declared secret reaches an effect
E · egress: [{ to: "exec" }]        ✔ clean
F · infer prompt, no egress         ✖ SECRETS leak into infer (task `t`)
G · non-occlusion (2nd secret)      ✖ SECRETS leak into invoke — secrets.other
                                      refused even with secrets.hook's clause
H · unsanctioned on_finally cleanup ✖ leaks past a sanctioned first cleanup
```

**R4-F1's three live shapes** (BEFORE the fix — the same generic teach
on every refused edge):

```
C  · egress sink+host declared, net.http ABSENT
     ✖ SECRETS … fix: sanction it — `egress: [{ to: "nika:fetch" }]`
       (the file already carries exactly that clause; the missing layer
        is capability — PERMITS names it on the next line, SECRETS didn't)
C1 · egress to: "nika:notify", used in nika:fetch
     ✖ SECRETS … fix: sanction it — `egress: [{ to: "nika:fetch" }]`
C2 · egress to: "nika:fetch" host: "other.example.com", url api.stripe.com
     ✖ SECRETS … fix: sanction it — `egress: [{ to: "nika:fetch" }]`
```

## The repair (this run)

`LeakReason` (NoEgress · SinkNotCleared · HostMismatch ·
CapabilityMissing · DerivedDestination · SelfShapeBroken) computed
beside `is_sanctioned` in `declass.rs` (one seam, no drift), carried on
`SecretLeak`, rendered by `findings.rs::leak_fix` — and
`check_render.rs::secret_rows` switched to read the ONE findings fold
(its private second copy of the fix text deleted; the human voice IS
the `--json` voice again). Live, post-fix:

```
C  · fix: the `egress:` exists — the missing layer is capability:
          add "api.stripe.com" to `permits.net.http` (the sanction
          narrows, never widens, permits)
C1 · fix: add `"nika:fetch"` to the `to:` list of the existing
          `egress:` on `secrets.k`
C2 · fix: the `egress:` `host:` (other.example.com) must equal the
          sink's literal destination (api.stripe.com) — a host clears
          only itself
A  · fix: sanction it — `egress: [{ to: "exec" }]` (unchanged, correct)
```

Unit locks: 4 ladder tests (no-egress vs wrong-sink · host and
capability naming · derived + broken-self shapes · the rendered fix
teaches `permits.net.http` and never « sanction it ») · 564 nika-check
+ 101 nika-display tests · fn-length ratchet green (three extractions:
`findings::push_secret_rows` · `check_render::cost_empty_arm` +
`cost_task_rows` · `permits_escape_rows`).
