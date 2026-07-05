# Stdlib v0.1 · Builtin coverage matrix

> The <!-- canon:builtins -->24<!-- /canon --> builtins audited **as a SET** (2026-06-10) · capability coverage ·
> overlap boundaries · naming grammar · deliberate absences. Per-builtin
> specs live in [builtins-v0.1.md](./builtins-v0.1.md); this file answers
> the set-level questions · « can it do everything? » and « is anything
> duplicated? ». SPDX-License-Identifier: Apache-2.0.

---

## Capability × builtin grid

| Capability class | Covered by | Status |
|---|---|---|
| data-shaping | `jq` · `json_diff` · `json_merge_patch` · `convert` · `validate` | ✅ full (jq is the one data language) |
| web I/O | `fetch` (inbound · <!-- canon:extract_modes -->9<!-- /canon --> extract modes · SSRF-guarded) · `notify` (outbound) | ✅ for content · APIs see boundary B1 |
| files | `read` · `write` · `edit` · `glob` · `grep` | ✅ full |
| databases | — | ✅ **deliberate** · `mcp:<server>/<tool>` (e.g. `mcp:postgres/query`) |
| memory / recall | — | ✅ **deliberate** · `mcp:memory-server/*` today · `nika:connectome/*` reserved ([08 §Connectome](../spec/08-out-of-scope.md)) |
| workflow composition | — | ✅ **deliberate** · deferred `nika:run` + recursion guard ([08 §composition](../spec/08-out-of-scope.md)) |
| time | `wait` (relative XOR absolute) · `date` | ✅ |
| hash / crypto | `hash` | ✅ hashing · signing/encryption = deliberate absence (B2) |
| notify / human | `notify` · `prompt` (blocking approval) | ✅ full |
| media / image generation | `image_generate` (openai · gemini · mock · assets land on disk + provenance manifest) | ✅ first §Media graduate (2026-07-05) · editing + the rest of the media class deferred |
| control / observability | `assert` · `done` · `log` · `emit` · `inspect` (+ DAG-side `when` · `for_each`) | ✅ full |

Every capability class is covered or carries a **written deliberate-absence
posture** with an escape hatch (`exec:` · `mcp:`). No silent gaps.

## Overlap boundaries · stated once

| Pair | The boundary |
|---|---|
| `fetch` vs « an HTTP client » | `fetch` is web-**content acquisition** (GET + extract modes + SSRF guards) · NOT an API client. API calls → `mcp:` or `exec: curl`. |
| `jq` vs `json_diff` / `json_merge_patch` | both *expressible* in jq · named for their RFC contracts (merge-patch = RFC 7396) · stable semantics worth a name. |
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
| B1 | generic HTTP builtin (`nika:http` · POST/PUT) | **rejected v0.1** · unconstrained HTTP is the SSRF/exfiltration surface `fetch` deliberately guards · `mcp:` servers and `exec: curl` cover it inside the trust model |
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

Per-builtin **formal args/returns schemas** are not yet published:
builtins-v0.1.md ships examples + prose. A per-builtin contract block
(`args:` JSON Schema · `returns:` shape · `throws:` codes) is the next
stdlib documentation milestone; until it lands, the YAML examples + the
reference engine define the precise shapes.

---

🦋 *<!-- canon:builtins -->24<!-- /canon --> builtins · zero duplicates · every absence written.*
