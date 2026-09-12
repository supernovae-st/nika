---
description: Reconcile a workflow's declared permits with its intended effects.
argument-hint: <file.nika.yaml>
allowed-tools: Bash(nika check:*), Read, Edit, Glob
---

Declare the blast radius — permits are data in the file, reviewable
in a diff, default-deny once declared. An ABSENT block is not the
unconfined floor: it is ZERO authority, and any effect under it
refuses `NIKA-AUTH-006` at check, before a token is spent.

Target: `$ARGUMENTS` (no argument? `Glob` for `*.nika.yaml` — one
match runs, several ask).

1. `nika check $ARGUMENTS --infer-permits` — the engine prints the
   tightest `permits:` block the workflow actually needs (hosts ·
   paths · tools), derived from the tasks, not guessed.
2. Review inferred grants and unresolved notes against the intended scope,
   then apply the justified block. A body with no effects at all still
   states its zero explicitly: `permits: {}`. If a `permits:` block
   already exists, diff the two and narrate what widened or narrowed —
   a widening is a conscious decision, name what the new task
   touches. Every bound is a literal, never an interpolation
   (`NIKA-AUTH-007`), and a `*.` subdomain wildcard is refused —
   name exact hosts (`NIKA-AUTH-010`).
3. `nika check $ARGUMENTS` — must stay clean with the boundary in
   place. A permits finding here means a task reaches outside the
   declared boundary: either the task is wrong or the boundary is —
   resolve it from the user's existing mandate; ask only if intent remains
   ambiguous. Never widen a grant simply to silence the finding.
4. Close by narrating the boundary in one breath: which hosts it may
   call, which paths it may touch, which tools it may invoke — and
   that everything else is now refused at run time.
