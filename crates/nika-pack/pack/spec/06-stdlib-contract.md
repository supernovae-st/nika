# 06 · Stdlib contract

> The stdlib is the set of **providers** · **extract modes** · and
> **builtins** that ship with v0.1 conformant engines. The stdlib is
> versioned **independently** from the core language · it may evolve
> while the 5 pillars remain frozen.

---

## What is the stdlib?

```
CORE LANGUAGE                          STDLIB
─────────────                          ──────
envelope                                providers (infer · agent)
4 verbs (signatures only)               extract modes (the nika:fetch tool)
DAG semantics                           builtins (invoke)
variable substitution
error model

Locked forever at v1.                   Versioned · stdlib v0.x · evolves.
```

A workflow that uses **only** the core language is portable across
engines forever. Adding stdlib dependencies (e.g. specific provider names ·
specific builtins) constrains portability to engines that ship those
stdlib elements.

---

## Why split core vs stdlib?

Three reasons ·

1. **Forward compat** · we need to add providers (new LLMs ship every quarter), add builtins, add extract modes. Locking these in the core would mean breaking the language every few months. Splitting lets the core stay frozen.

2. **Optional surface** · an engine may choose not to ship the full stdlib. A minimal engine may support only `mock` provider for testing. A specialized engine may ship only a subset of builtins. The conformance level reflects this (see [07-conformance.md](./07-conformance.md)).

3. **Audience clarity** · the core is the contract between workflow author and engine. The stdlib is the curated library of building blocks an engine ships.

---

## Versioning

The stdlib has **independent versioning** ·

- `stdlib/providers-v0.1.md`, the <!-- canon:providers -->14<!-- /canon --> canonical providers for v0.1
- `stdlib/extract-modes-v0.1.md`, the <!-- canon:extract_modes -->9<!-- /canon --> canonical extract modes for v0.1
- `stdlib/builtins-v0.1.md`, the <!-- canon:builtins -->23<!-- /canon --> canonical builtins for v0.1

When the stdlib evolves to v0.2 · those files become `*-v0.2.md` and new versions are published. The core language contract (`nika: v1`) is unchanged.

A workflow MAY declare a stdlib version dependency · though v0.1 does not require this. v0.2 may introduce an optional `stdlib:` field in the envelope.

---

## Inclusion criteria

For an element to enter the stdlib · it MUST satisfy ·

1. **Empirical demand** · documented use in real workflows (canonical examples)
2. **Mature implementation** · battle-tested in at least one engine · bug-stable
3. **Forward-compat safe** · signature stable · no breaking semantic risk
4. **Sovereignty-aligned** · prefers local-first OR multi-vendor (no single-cloud lock-in)

The reference engine provides these elements behind strict quality gates. Other engines may match this bar.

---

## What's IN stdlib v0.1

### Providers (14)

`ollama` · `lmstudio` · `llamacpp` · `localai` · `vllm` (5 local) · `mistral` · `anthropic` · `openai` · `openrouter` · `groq` · `deepseek` · `gemini` · `xai` (8 cloud) · `mock` (test)

Selected via a single `model: <provider>/<name>` field. Any other OpenAI-compatible local/remote server routes through the `openai` + `base_url` escape hatch (no new provider name). See [stdlib/providers-v0.1.md](../stdlib/providers-v0.1.md).

### Extract modes (9)

`markdown` · `article` · `jq` · `text` · `selector` · `metadata` · `links` · `feed` · `sitemap`

See [stdlib/extract-modes-v0.1.md](../stdlib/extract-modes-v0.1.md).

### Builtins (23)

6 core (log · emit · assert · prompt · done · wait)
+ 5 file (read · write · edit · glob · grep)
+ 8 data (jq · json_diff · validate · json_merge_patch · convert · uuid · date · hash)
+ 1 introspection (inspect · view-discriminated · 4 views cost/records/dag_info/threads)
+ 2 network (fetch · notify)
= **22 canonical builtins** (Stdlib v0.1 · consolidated · was 42 · `jq` subsumes 13 data builtins · validators merged into `validate` · `task_status`/`orchestrate`/`locale_lookup` cut · `sleep`+`wait_until` merged into unified `nika:wait` per ADR-087 · `cost`+`records`+`dag_info`+`threads` merged into unified `nika:inspect` per ADR-088 · ZERO capability loss)
(+ media · **deferred** to stdlib v0.x · NOT in the v0.1 count)

See [stdlib/builtins-v0.1.md](../stdlib/builtins-v0.1.md).

---

## What's OUT of stdlib v0.1

Deliberately deferred to stdlib v0.x ·

- **24 media builtins** · pdf_extract · svg_render · chart · phash · thumbhash · provenance · etc. (heavy · high maintenance · niche audience)
- **Advanced agent presets** · multi-agent coordination patterns · supervisor/worker · etc.
- **Memory recall builtins** · awaiting the engine's memory subsystem (the Connectome · stdlib v0.5+)
- **Workflow include / import** · single-file workflows in v0.1

When these mature · they enter stdlib v0.x. The core language doesn't need to change.

---

## How a workflow references the stdlib

### Model selection

```yaml
model: anthropic/claude-sonnet-4-6   # <provider>/<name> · see stdlib/providers-v0.1.md
# model: ollama/qwen3.5:4b          # local · same shape
```

### Extract mode

```yaml
invoke:
  tool: "nika:fetch"
  args:
    url: "https://example.com"
    mode: article             # references stdlib/extract-modes-v0.1.md
```

### Builtin

```yaml
invoke:
  tool: "nika:read"            # nika:* namespace = builtin
  args:
    path: "./config.yaml"
```

---

## How a workflow references non-stdlib tools

For MCP tools (not in the stdlib · provided by external MCP servers) ·

```yaml
invoke:
  tool: "mcp:postgres/query"   # mcp:<server>/<tool> namespace (one colon · slash path)
  args:
    sql: "..."
```

These are not stdlib · they depend on the engine's MCP server registry being configured.

## Namespace ownership · `nika:` · `mcp:` (closed at v1)

The namespace set is **CLOSED at v1**: `nika:` and `mcp:` are the only two
([02 §tool reference grammar](./02-verbs.md#tool-reference-grammar-canonical) ·
any other prefix is rejected at parse time) ·

| Namespace | Owner | Source of truth | Examples |
|---|---|---|---|
| `nika:*` | **Spec-owned** | `stdlib/builtins-v0.1.md` (this directory's sibling) | `nika:read` · `nika:jq` · `nika:done` |
| `mcp:<server>/*` | Engine + user | Engine's MCP server registry config | `mcp:postgres/query` · `mcp:browser/navigate` |

The `nika:*` namespace is **spec-owned**. A custom engine MUST NOT add tools
to the `nika:*` namespace · it would violate portability (a workflow using a
vendor's `nika:custom` builtin would not run on a different engine).

**Engine-specific tools route through `mcp:`**: an engine that ships custom
capabilities exposes them as an MCP server it hosts (`mcp:myengine/research`).
That is exactly what the protocol is for · the tool is then *declared* in the
engine's MCP registry like any other (portability semantics intact · any
engine with that server configured runs the workflow) · and no third
namespace appears silently.

(An OpenAPI-style `x-<vendor>:` prefix was considered and is **reserved as a
possible future additive minor**: it does NOT exist in v0.1 · a parser that
accepts `x-anything:tool` today is non-conformant.)

---

## Conformance levels

See [07-conformance.md](./07-conformance.md). In summary ·

| Level | Stdlib requirement |
|---|---|
| Core | None · only parse + DAG + variable + error · no execution needed |
| Runtime | Must execute the 4 verbs · provider/tool implementations engine's choice |
| Stdlib v0.1 | Must ship the <!-- canon:providers -->14<!-- /canon --> providers + <!-- canon:extract_modes -->9<!-- /canon --> extract modes + <!-- canon:builtins -->23<!-- /canon --> builtins |
| Stdlib v0.1+media | RESERVED · enumerated when the media set publishes (stdlib v0.x · the 24 names are not yet normative) |

A v0.1-compliant engine for a workflow author depends on which level they need.

---

## Forward-compat

The **split between core and stdlib** is locked at v1. The contents of the stdlib v0.x evolve.

Out of scope for v0.1 · explicit stdlib version pinning in the workflow envelope · per-task stdlib version overrides · stdlib element discovery API. See [08-out-of-scope.md](./08-out-of-scope.md).

---

🦋 *Next · [07 · Conformance](./07-conformance.md)*
