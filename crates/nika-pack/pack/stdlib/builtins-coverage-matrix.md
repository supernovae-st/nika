# Stdlib v0.1 · Builtin coverage matrix

> The <!-- canon:builtins -->28<!-- /canon --> builtins audited **as a SET** (2026-06-10) · capability coverage ·
> overlap boundaries · naming grammar · deliberate absences. Per-builtin
> specs live in [builtins-v0.1.md](./builtins-v0.1.md); this file answers
> the set-level questions · « can it do everything? » and « is anything
> duplicated? ». SPDX-License-Identifier: Apache-2.0

---

## Capability × builtin grid

| Capability class | Covered by | Status |
|---|---|---|
| data-shaping | `jq` · `json_diff` · `json_merge_patch` · `convert` · `validate` | ✅ full (jq is the one data language) |
| web I/O | `fetch` (<!-- canon:extract_modes -->10<!-- /canon --> extract modes · all 6 methods · headers/body/form/multipart · SSRF-guarded) · `notify` (outbound) | ✅ content AND APIs · the two residual shapes are named in §overlap boundaries |
| files | `read` · `write` · `edit` · `glob` · `grep` | ✅ full |
| databases | — | ✅ **deliberate** · `mcp:<server>/<tool>` (e.g. `mcp:postgres/query`) |
| memory / recall | — | ✅ **deliberate** · `mcp:memory-server/*` today · `nika:connectome/*` reserved ([08 §Connectome](../spec/08-out-of-scope.md)) |
| workflow composition | — | ✅ **deliberate** · never a builtin — the `invoke: workflow:` tagged union ([14-composition](../spec/14-composition.md) · the once-proposed `nika:run` is abandoned) |
| time | `wait` (relative XOR absolute) · `date` | ✅ |
| hash / crypto | `hash` | ✅ hashing · signing/encryption = deliberate absence (B2) |
| notify / human | `notify` · `prompt` (blocking approval) | ✅ full |
| media / charts | `chart` (deterministic zero-dep renderer · byte-identical SVG · sha256 → trace chain · optional Vega-Lite sibling) | ✅ §Media graduate #3 (2026-07-09) · attested artifacts ([builtins-v0.1 §chart](./builtins-v0.1.md)) |
| media / image generation | `image_generate` (local · openai · gemini · xai · mock · assets land on disk + provenance manifest) | ✅ §Media (2026-07-05) · `mode: edit` specified ([builtins-v0.1 §edit](./builtins-v0.1.md)) · the rest of the media class deferred |
| media / speech synthesis | `tts_generate` (local · openai · elevenlabs · mock · assets land on disk + manifest incl. `watermark_declared`) | ✅ §Audio (2026-07-05) |
| media / artistic effects | `image_fx` (deterministic dither · palette · duotone · pixelate · halftone · grain · vignette · chromatic_aberration · scanlines · glitch · ascii — byte-identical artifacts · recipe `image_fx/v1` in-chunk) | ✅ §Media graduate #3 (2026-07-09) |
| control / observability | `assert` · `done` · `log` · `emit` · `inspect` (+ DAG-side `when` · `for_each`) | ✅ full |

Every capability class is covered or carries a **written deliberate-absence
posture** with an escape hatch (`exec:` · `mcp:`). No silent gaps.

## Overlap boundaries · stated once

| Pair | The boundary |
|---|---|
| `fetch` vs « an HTTP client » | **`fetch` IS the API client.** It carries `method` (GET · POST · PUT · DELETE · PATCH · HEAD), `headers` (auth rides here, masked · `x-api-key: "${{ secrets.KEY }}"`), `body`, `form`, and `multipart` file upload under the `permits.fs` read boundary — plus the extract modes and the SSRF floor. **An API call belongs in `fetch`.** `mcp:` is for a tool a server already exposes; `exec: curl` is for the two things `fetch` deliberately does not do (below). |
| `jq` vs `json_diff` / `json_merge_patch` | both *expressible* in jq · named for their RFC contracts (merge-patch = RFC 7396) · stable semantics worth a name. |
| **the two things `fetch` does NOT do** | ① **no response-body-to-file** · every `mode:` returns text or JSON, so a binary response (an image, an archive) has nowhere to land. ② **no failure tolerance** · a non-2xx is a normative throw (`NIKA-BUILTIN-FETCH-001`), so a call that must survive a 404 cannot. **Both are measured, not hypothetical** — a corpus sweep found 18 `exec: curl` sites and *every one* is one of these two (12 saving a binary body, 5 tolerating failure, 1 an adversarial deny-probe). Until they are closed, those two shapes are the honest `exec: curl` territory — and nothing else is. |
| `jq` vs `convert` | jq is JSON-in/JSON-out · `convert` crosses formats (yaml↔json↔toml…). |
| `validate` vs per-task `schema:` | `schema:` gates a task's OWN output (auto-retry) · `validate` checks any value mid-flow · same JSON Schema dialect. |
| `write` vs `edit` | whole-file vs in-place patch (mirrors agent-tool conventions). |
| `log` vs `emit` | human-facing line vs structured machine event. |
| `assert` vs `when:` | fail-fast guard vs skip-guard. |
| `wait` vs `timeout:` | a tool that *consumes* time vs a *bound* on time. |

**Zero duplicates** · the 42→22 consolidation (ADR-086/087/088) removed the
real ones (`sleep`+`wait_until`→`wait` · 4 introspections→`inspect` ·
13 data-shapers→`jq` recipes).

## Deliberate absences · examined and rejected

| # | Candidate | Verdict |
|---|---|---|
| B1 | an **UNGUARDED** HTTP builtin (`nika:http`) | **rejected forever** · and the wording once read « POST/PUT », which misread as « nika does not do POST ». It does: `fetch` carries all six methods. What B1 refuses is HTTP *without* the SSRF floor, the permits boundary and the secret-egress check — a second, ungoverned door beside the governed one. **The guarded client shipped; it is called `fetch`.** |
| B2 | crypto beyond hashing (sign · encrypt) | **rejected** · key handling in YAML is a trap · `exec:`/`mcp:` territory |
| B3 | archive ops (tar/zip) | **deferred** with the media builtins (stdlib v0.x) |
| B4 | randomness beyond `uuid` | **rejected** · an anti-feature for run determinism · jq/`exec:` territory |

## Naming grammar

`nika:<noun>` · single lowercase word · snake_case compound ONLY when
disambiguating a format-bound operation (`json_diff` · `json_merge_patch`).
Multi-format tools stay unprefixed (`validate` · `convert`). Multi-mode
tools are ONE builtin with a discriminating argument (`wait` mode ·
`inspect` view), never N siblings.

## Known set-level gap (work item)

~~Per-builtin **formal args/returns schemas** are not yet published.~~
**Stale — they ARE published.** `nika catalog --tools --json` ships a
full JSON Schema per builtin (`parameters` with `properties` and
`required`) for all <!-- canon:builtins -->28<!-- /canon -->. The
machine surface exists; this paragraph claiming otherwise was the drift.

What remains genuinely missing is the **prose** half: `builtins-v0.1.md`
carries examples and description, not a rendered contract block beside
each entry. That is a documentation projection of a machine surface that
already exists — not an unwritten contract.

---

🦋 *<!-- canon:builtins -->28<!-- /canon --> builtins · zero duplicates · every absence written.*
