---
description: Compile a reviewable workflow through the stateless engine core
argument-hint: "[exact-skeleton] [explicit-file.nika.yaml]"
allowed-tools: Bash(nika compile:*), Bash(nika try:*), Bash(nika check:*), Read
---

Use the single Compile core to preview the requested candidate and its questions.
Arguments: `$ARGUMENTS`.

1. `nika compile --list` lists exact skeletons; `nika try` is read-only discovery.
   Preserve the user's intent: an unsupported natural-language request remains
   incomplete, with no closest-template substitution or invented policy.
2. `nika compile <exact-skeleton> --json` previews source-only Check and stable
   questions. Answer explicitly with repeatable `--answer KEY=JSON_LITERAL`.
   `nika compile hello` previews the zero-network mock lesson.
3. Write only to an explicitly requested destination:
   `nika compile <exact-skeleton> <file>.nika.yaml --answer ...`.
   Do not infer overwrite consent; an existing file requires explicit `--force`.
4. For an accepted existing source, use
   `nika compile --base <file> --change 'Set const.NAME to JSON_LITERAL' --json`.
   Add `--output <file>` for an explicit edit destination.
   Only existing constants are supported; Graph gate-add and general natural
   language editing remain incomplete. Source revision ownership stays with the caller.
5. Return the candidate/questions and exact diagnostics. Source-only Check is
   not runtime admission. No provider calls, model selection, grants or runs
   follow from Compile; `nika check <file>` and an authorized `nika run <file>`
   remain separate gestures.
