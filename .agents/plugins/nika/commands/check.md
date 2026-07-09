---
description: Audit a .nika.yaml workflow before it runs — findings with their NIKA-XXXX codes, cost envelope, permits
argument-hint: <file.nika.yaml>
allowed-tools: Bash(nika check:*), Bash(nika explain:*), Read, Glob
---

Audit the workflow **before** any run — check is the oracle, the file is
the contract.

Target: `$ARGUMENTS` (no argument? `Glob` for `*.nika.yaml` — one match
runs, several ask).

1. Run `nika check $ARGUMENTS --json` and read the payload, not the
   prose: `clean` is the verdict · `conformance[]` carries the findings ·
   `pricing` carries the rates (a `null` rate is UNKNOWN, never $0) ·
   `models_resolve` (0.99+) says every `model:` runs in THIS binary.
2. Summarize what the payload says, in this order:
   - **Verdict** — clean, or N findings.
   - **Findings** — one line each: `NIKA-XXXX · task <id> · <message>`,
     with the fix the diagnostic teaches. Unknown code? Run
     `nika explain NIKA-XXXX` and fold its teaching in.
   - **Cost** — the ceiling (`≤ $X`) or the floor (`≥ $X FLOOR` — name
     WHY it is unbounded: missing `max_tokens`, uncataloged model,
     expression fan-out). A local model is **unpriced, never free**.
   - **Permits** — declared boundary, or « engine floor only »
     (suggest `nika check $ARGUMENTS --infer-permits` to write one).
3. Exit code 2 with findings is the NORMAL red path — repair from the
   diagnostics and re-check. Never hand the human a file that does not
   pass. Only report a broken oracle (exit 3) as an error.
