---
id: ADR-125
title: "the native session is a grounded host runtime over the installed engine, never a workflow"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "session", "grounding", "intelligence", "one-door"]
affects_crates: ["nika-session", "nika-cli", "nika-cli-host"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-122", "ADR-123", "ADR-124", "ADR-126", "ADR-127", "ADR-128"]
requires: ["ADR-124"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.118"
follow_ups: ["project changes authored from the session (preview · consent · apply · check)", "run attachment and the human gate returning to the session", "the intelligence changed in-session", "a generated language digest from the canon"]
---

# ADR-125: the native session is a grounded host runtime, never a workflow

## Context

Bare `nika` opened a concierge card on every stream, and a hidden
`nika thread` opened a conversation that was a proof of reuse and not a
product: a hardcoded vendor model (`xai/grok-4`), a plaintext history
folded into one prompt, a temporary `agent:` workflow written to `/tmp`
per turn with `tools: []` and `permits: {}`, no grounding, no trace, no
guard. The pack's failure transcript showed the consequence: asked to
author a workflow, the model invented a JSON workflow language, confused
Nika with n8n, and finally claimed it did not know Nika. The one-door
pack's law: the native session must make that class of failure
structurally difficult, and it must be subject to Nika's own
data-boundary discipline before any workflow exists.

## Decision

1. **A new L4 crate, `nika-session`, is the runtime.** It owns the
   conversation and nothing the engine already owns: the grammar, the
   catalogs, the codes, the checker, the runtime and the project file
   grammar are queried, never mirrored (the crate depends laterally on
   `nika-cli-host` for the ONE probe and the ONE oracle facade, ADR-124's
   precedent).
2. **The session observes the project once** (`ProjectSnapshot`: the
   proven root — the git root, else the working directory — the project
   file and its ceiling, the workflows the ONE walker lists with the
   checker's verdicts, honest about truncation).
3. **The human chooses the intelligence** on the first run, in the
   atelier order and in human words (an AI app they already have · an
   API · a local engine · none); the choice is kept at
   `~/.nika/session-intelligence.json`, shared by every install channel.
   The census is deterministic (presence, never a dial, never a value).
   An explicit choice this machine cannot serve is refused with its fix
   and never replaced. Each path names its data locus before the first
   turn.
4. **The reasoner is ONE inference over the chosen path** — the same
   infer-grade seat adapter `nika run` dispatches through, the same
   provider registry and one-shot infer verb a workflow's `infer:`
   rides, or none. Never a temporary workflow, never a trace for a chat
   turn, never a hidden shell.
5. **The context broker hands the model a minimal typed bundle**: the
   identity core (six laws) and the language digest, the snapshot's
   compact facts, the snippets the HUMAN named (only `.nika.yaml` and
   `nika.yaml`, only inside the root, bounded, obvious secrets redacted
   with the kinds recorded), the diagnostics, the recent dialogue. The
   environment is never injected. The model does not decide its own read
   boundary.
6. **Nika facts are answered by the engine before any model is asked**:
   the workflows, the builtins, the providers, a workflow's verdict
   through the facade, a code through the explain ladder, a shape
   through the ONE router. A session without conversational intelligence
   still answers them.
7. **Every reply is read by the hallucination guard** before a human
   sees it: a `nika:<builtin>`, a `<provider>/<model>`, a `NIKA-…` code,
   an `mcp:<server>`, a `nika <verb>` or a top-level workflow field the
   engine does not carry is corrected under the reply; a claim of
   ignorance about Nika is corrected too. Nothing invented is presented
   as real.
8. **One door.** Bare `nika` on an interactive terminal enters the
   session; a pipe keeps the deterministic concierge (exit 0). `nika
   thread` is deleted in the same change, with no alias.

## Consequences

- The session is a product surface with a data boundary, not a chatbot
  over a workflow: what leaves the machine is the bundle, and the human
  read where it goes before typing.
- Proof: the crate's tests (a turn writes nothing · the reasoner
  receives only the bundle, redacted, never the environment · the
  seven inventions of the pack's adversarial corpus corrected · the
  unserved choice refused with its fix · the facts without any model ·
  the preference round trip · the first screen in the atelier order);
  `session_pty.rs` on the real binary (the pipe is the concierge, the
  TTY is the session, the first run asks once, the kept choice never
  asks again, `nika thread` is the parser's own refusal).
- The concierge's `nika thread` teaching and the `RunVerdict` fields the
  thread alone read are gone (zero legacy).

## Follow-ups

- Wave 5: project changes authored from the session (preview → consent
  → apply → the real check), run attachment, the human gate returning
  to the session as a pending decision.
- The intelligence changed in-session (`/intelligence` re-asks).
- The language digest generated from the canon at build time.
