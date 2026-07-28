---
description: Audit a .nika.yaml workflow before it runs — findings with their NIKA-XXXX codes, cost envelope, permits
argument-hint: <file.nika.yaml>
allowed-tools: Bash(nika check:*), Bash(nika explain:*), Read, Glob
---

Audit the workflow **before** any run — check is the oracle, the file is
the contract.

Target: `$ARGUMENTS` (no argument? `Glob` for `*.nika.yaml` — one match
runs, several ask).

1. Run `nika check $ARGUMENTS --native-strict --json` and read the
   payload, not the prose. **Two keys carry the verdict, not one.**
   `clean` covers the conformance findings ONLY: a workflow whose real
   work hides inside `exec python3 helper.py` comes back `clean: true`
   with `native_strict_clean: false` and exit code 2. Green means BOTH
   are true — reporting on `clean` alone hands the human a file the run
   gate will refuse. Then: `conformance[]` carries the findings ·
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
   - **Native path** — `hints[]` rows with `kind: native-first`. Each
     one names an `exec` that a builtin already covers. These are the
     rows `native_strict_clean` counts: replace the exec, or record it
     in the workflow's exec ledger comment with the unlock that would
     remove it. An `exec` of a real tool (`git`, `docker`) passes;
     an `exec` of a `.py`/`.mjs`/`.sh` wrapper does not.
   - **Permits** — the declared boundary, or ZERO AUTHORITY when no
     block is present: absent is the EMPTY boundary, never the
     unconfined floor, so an effect under it is `NIKA-AUTH-006`.
     Suggest `nika check $ARGUMENTS --infer-permits` to write the
     tightest one.
3. Exit code 2 with findings is the NORMAL red path — repair from the
   diagnostics and re-check. Never hand the human a file that does not
   pass. Only report a broken oracle (exit 3) as an error.
