---
description: Diagnose this machine's Nika surface — binary, plugin kits, providers, wiring — every problem with its exact fix command
allowed-tools: Bash(nika doctor:*), Bash(nika explain:*)
---

Health check of THIS machine — doctor diagnoses, the human keeps the
hand: nothing is mutated, every problem prints the command that fixes
it. Drift is advisory by design (warn · exit 0) — a diagnosis never
breaks automation.

1. Run `nika doctor --json` and read the payload, not the prose:
   `summary` carries the verdict counts, `findings[]` the rows
   (level · label · detail · fix).
2. Report, in this order:
   - **Verdict** — the one line: N ok · N warn · N fail.
   - **Fails first** — a fail means NO inference path at all; its fix
     blocks every run, surface it before anything else.
   - **Kit drift** — `kit` rows compare each installed plugin kit
     (Cursor · Claude Code · Codex) against the binary's release
     train. Relay the per-client fix VERBATIM — Claude Code climbs
     TWO rungs (marketplace update, then plugin update, then a
     restart); a half-climbed ladder is the proven trap.
   - **Warns that matter** — unset provider keys are advisory
     (doctor cannot know which provider your work needs): mention
     only the ones the current work touches. Unwired clients get
     their `nika wire <client>` line.
3. The fixes are the human's move — propose the commands, never run
   installs or config mutations yourself. Offer to re-run
   `/nika:doctor` after, to confirm green.
