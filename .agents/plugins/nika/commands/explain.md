---
description: Narrate a workflow (waves · cost · touches · how to run) or teach a NIKA-XXXX error code
argument-hint: <file.nika.yaml | NIKA-XXXX>
allowed-tools: Bash(nika explain:*), Read
---

Explain `$ARGUMENTS` — the same command narrates both shapes:

- **A workflow file** → `nika explain $ARGUMENTS` tells the run story
  BEFORE it happens: the waves (what runs in parallel), the cost
  envelope (ceiling or floor — with the honest reason when unbounded),
  what the run will touch (files · network · tools), and how to run it.
  Relay that narration; add `--forecast` when the file has run before
  (the typed honesty ladder over recorded durations).
- **An error code** (`NIKA-XXXX`) → `nika explain $ARGUMENTS` teaches
  the class: what it means, why the engine refuses, the canonical fix.
  Every code also has its page at `https://nika.sh/errors/NIKA-XXXX`.

Ground every claim in the command's output — the narration is computed
from the file, never from memory of it. If the target is ambiguous
(no file, no code shape), say what you need instead of guessing.
