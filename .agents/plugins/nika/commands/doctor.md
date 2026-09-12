---
description: Diagnose Nika installation, provider access and client wiring.
allowed-tools: Bash(nika doctor:*), Bash(nika explain:*)
---

Run `nika doctor --json`. Read `summary` and `findings[]`; distinguish tool
failure from advisory findings. Prioritize failures affecting the requested
work, then relevant kit drift or missing provider access. Relay the actual
per-client fix command: marketplace refresh, plugin refresh and host restart
can be distinct operations.

This command is diagnostic: its allowed tools do not install or mutate config.
Return fixes to the coordinating conversation, which may carry out setup
already authorized by the user. Interactive credential entry is the human's move
when the host requires it; do not request or print secret values. After
an actual repair, run the relevant diagnostic again and report its result.
