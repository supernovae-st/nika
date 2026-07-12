---
name: nika-operating
description: Operate Nika workflows day-2 — spend caps, permits boundaries, secrets, model swaps (cloud/local), CI wiring, trace export. Use when hardening a working workflow for production, wiring it into CI or a scheduler, capping cost, tightening the permits boundary, swapping models, or exporting traces to OpenTelemetry.
---

# Operating Nika workflows

Authoring makes a file pass `nika check`. Operating makes it safe to
run unattended: bounded spend, a declared blast radius, masked
credentials, a model you chose, and a journal you can export.

## Spend (the envelope is part of the contract)

- `nika check <file>` prints the cost BEFORE any token: `≤ $X` is a
  ceiling · `≥ $X FLOOR` means at least one task is unbounded — fix
  the reason (a missing `max_tokens`, an uncataloged model, an
  expression fan-out), never ship a floor to production.
- Cap the run: `nika run <file> --max-cost-usd <n>` blocks BEFORE the
  call that would cross the cap.
- A local model is **unpriced compute, not free** — say "unpriced",
  never "$0".

## Permits (declare the blast radius)

```
nika check <file> --infer-permits
```

prints the tightest `permits:` block the workflow needs — paste it
into the file. From then on the boundary is default-deny: a new host,
path or tool must be added consciously, in a reviewable diff. Permits
are data, not config — they travel with the file through PR review.

## Secrets (masked, declared, sunk)

- Every credential rides `${{ secrets.X }}`, declared in the
  `secrets:` block with its `egress:` sink — the engine masks it in
  logs and refuses to send it anywhere but the declared sink.
- Values come from the environment at run time; CI injects them the
  same way a shell does. Never a literal in YAML, never in a trace.
- `nika doctor` audits the machine: binary, PATH, provider env vars.

## Models (a one-line swap, both directions)

- Models are `provider/name`. `nika catalog` is the embedded registry:
  providers · models · capabilities · which env var each needs.
- Shape with `mock/echo` (offline, deterministic, zero keys). Prove
  structure first, spend later.
- Sovereignty path: `--model ollama/<model>` runs local — same file,
  same check, same trace. Give local providers `timeout: "300s"`+.
- The file does not hardcode a vendor: swapping cloud↔local is a
  `--model` flag or one line in the YAML, never a rewrite.

## CI (the check is the gate, the golden is the pin)

- Gate every PR: `nika check <file> --json` — exit 0 clean · 2
  findings · 3 broken oracle. Parse `clean`, `conformance[]`,
  `pricing`, `models_resolve` from the payload.
- Pin behavior: `nika test <file> --update` writes
  `<file>.golden.json` from an offline mock run; `nika test <file>`
  replays and compares — deterministic, zero keys, CI-safe.
- `--native-strict` in CI keeps `exec:` honest: any shell task an
  embedded builtin covers fails the gate (the exec ledger documents
  the survivors).
- Schedule with the scheduler you already have (cron · CI · a
  systemd timer): the engine is a binary, the workflow is a file,
  `--var key=value` carries the parameters.

## Observability (the journal is exportable, not captive)

- `nika trace export <trace>` projects the journal to OTLP/JSON
  lines — drag into Jaeger UI (≥1.60) or POST to any OTLP/HTTP
  endpoint. Local file, zero collector, zero vendor.
- `nika trace ls` shows the store; retention never collects the `★`
  newest trace of each workflow. `nika trace rm --older-than <dur>`
  prunes deliberately.
- Audits cite `nika trace verify <trace>` (hash-chain intact), never
  a log screenshot.

## Production checklist

1. `nika check <file>` — clean, ceiling not floor.
2. `nika check <file> --native-strict` — exec ledger complete.
3. `permits:` pasted from `--infer-permits` — default-deny.
4. Secrets in `secrets:` with sinks — env-injected, never literal.
5. Golden pinned (`nika test <file> --update`, committed).
6. Spend cap on the run line (`--max-cost-usd`).
7. Trace store known (`.nika/traces/`) — export wired if anyone
   watches dashboards.
