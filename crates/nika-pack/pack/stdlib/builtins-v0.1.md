# Stdlib v0.1 · Builtins

> **<!-- canon:builtins -->22<!-- /canon --> canonical builtins** shipped with Stdlib v0.1-compliant engines.
> Invoked via `invoke: tool: "nika:<name>"`. Plus media builtins deferred to
> stdlib v0.x (opt-in feature flag).
>
> **Consolidation (« less but better »)** · was 42 → 26 (D-N6) →
> **22**. Step 1 (42 → 26 · D-N6) · `nika:jq` is THE data language · 13 thin
> wrappers that jq subsumes were cut (incl. `json_merge` = jq's recursive `*` ·
> jaq source-verified 2026-05-27 · `obj_merge` impl + test corpus + corelang
> docs) · the validators merged · `task_status`/`orchestrate`/`locale_lookup`
> cut. Step 2 (26 → 22 · ADR-086/087/088 Rams sweep 2026-05-27) · `convert`
> replaces `csv_to_json` (multi-format · from:/to:) · `wait` unifies
> `sleep`+`wait_until` (−1) · `inspect` unifies `cost`+`records`+`dag_info`+
> `threads` (−3). ZERO capability loss (jq ⊇ the cuts · jaq-verified · the
> collapses preserve every behavior via mode args). See §"What jq subsumes".

---

## Builtin categories

| Category | Count | Status |
|---|---|---|
| Core | 6 | Required for execution (log · emit · assert · prompt · done · wait) |
| File | 5 | I/O primitives (read · write · edit · glob · grep) |
| Data | 8 | `jq` (THE data language) + 7 capabilities jq can't express (json_diff · validate · json_merge_patch · convert · uuid · date · hash) |
| Introspection | 1 | Self-awareness (inspect · view-discriminated · 4 views · cost / records / dag_info / threads) |
| Network | 2 | fetch (HTTP+extraction) · notify (alerts out) |
| Media | — | **Deferred to stdlib v0.x** (opt-in feature flag) |
| **Total v0.1** | **22** | |

A Stdlib v0.1-compliant engine MUST ship these 22.

---

## Core builtins (6)

### `nika:log`
```yaml
invoke: { tool: "nika:log", args: { level: info, message: "Processing ${{ vars.user_id }}", data: { foo: "bar" } } }
```
Emit a log entry (`debug`/`info`/`warn`/`error`) to the workflow event stream (human-readable diagnostic).

### `nika:emit`
```yaml
invoke: { tool: "nika:emit", args: { event_type: custom.event, payload: { ... } } }
```
Emit a custom machine event (consumed by subscribers · journal). Distinct from `log` · `log` = human diagnostic · `emit` = machine event.

### `nika:assert`
```yaml
invoke: { tool: "nika:assert", args: { condition: "${{ tasks.X.output.count > 0 }}", message: "Expected non-empty result" } }
```
Fail the task if `condition` (a **CEL `${{ }}` boolean**) is false · else no-op. The **fail-fast guard** (distinct from `when:` which is the **skip-guard**). `condition:` uses the canonical `${{ }}` CEL surface — never a legacy `$task` syntax.

### `nika:prompt`
```yaml
invoke: { tool: "nika:prompt", args: { message: "Approve deploy to production?", default: false } }
```
Interactive human confirm · blocks until answered. v0.1 conformance · engines MAY use `default` in CI/non-interactive mode.

### `nika:done`
```yaml
invoke: { tool: "nika:done", args: { result: { status: "complete" } } }
```
Mark the current `agent:` loop complete and exit · the loop-completion sentinel (**valid only inside an `agent:` tool whitelist** · error elsewhere).

### `nika:wait` · temporal wait (relative OR absolute)
```yaml
invoke:
  tool: "nika:wait"
  args:
    duration: "5s"                  # RELATIVE · Go-duration string ("500ms"/"30s"/"5m"/"1h30m")
    # OR (mutually exclusive · exactly-one-of) ·
    until: "2026-05-23T09:00:00Z"   # ABSOLUTE · ISO 8601 timestamp · MAY be CEL expression
    timeout: "1h"                   # OPTIONAL · cap for absolute wait (until: only)
```
Temporal pause · ONE builtin · 2 modes (relative `duration:` XOR absolute `until:` · exactly-one-of validated at parse time).

Replaces · legacy `nika:sleep` (relative-only) + `nika:wait_until` (absolute-only · cut per ADR-087 Rams sweep · 2026-05-27). Same family · same trust class (PURE) · same temporal-control semantics · the unified surface preserves both behaviors via mode arg.

Throws · `NIKA-BUILTIN-WAIT-001` on absolute timeout · `-002` if the timestamp is in the past · `-003` if neither `duration:` nor `until:` set (or both set).

---

## File builtins (5)

### `nika:read`
```yaml
invoke: { tool: "nika:read", args: { path: "./config.yaml", encoding: utf-8 } }
```
Read a file · returns string content.

### `nika:write`
```yaml
invoke: { tool: "nika:write", args: { path: "./out.md", content: "...", create_dirs: true, overwrite: true } }
```
Write a file · returns the path. A binary `content` value (an opaque bytes output from an upstream tool · e.g. MCP image content) is written as-is · no `output_format` declaration needed (the value carries its own type).

### `nika:edit`
```yaml
invoke: { tool: "nika:edit", args: { path: "./file.md", find: "old", replace: "new" } }
```
In-place find/replace · returns the modified path.

### `nika:glob`
```yaml
invoke: { tool: "nika:glob", args: { pattern: "./src/**/*.rs", exclude: ["**/target/**"] } }
```
Glob match · returns array of paths.

### `nika:grep`
```yaml
invoke: { tool: "nika:grep", args: { pattern: "TODO:", path: "./src", case_insensitive: false } }
```
Recursive grep · returns array of `{ path, line, match }`.

---

## Data builtins (8) · `jq` is THE data language

### `nika:jq` · the transform + extraction primitive
```yaml
invoke: { tool: "nika:jq", args: { expression: ".items | map(.price) | add", input: "${{ tasks.X.output }}" } }
```
Run a jq expression. **The single data-transform-and-extraction language** — map · filter · select · group_by · reshape · string-interpolation `"\(.x)"` · `@base64`/`@base64d`/`@csv` encoders · array `flatten` · `leaf_paths`/`getpath`/`setpath`. The same jq used in `output:` bindings (see `04-variables.md`).

**`input` is any JSON value** — a single ref (`input: "${{ tasks.X.output }}"`) OR a **constructed array for multi-input ops**. Recursive merge of two objects (this is exactly why `json_merge` is NOT a builtin · jaq's `*` does it) ·
```yaml
invoke:
  tool: nika:jq
  args:
    input: ["${{ tasks.base.output }}", "${{ tasks.overlay.output }}"]
    expression: ".[0] * .[1]"      # recursive deep-merge · overlay wins
```
Same shape combines / zips N inputs · build the array, index inside jq.

**Implementation** · reference engine uses `jaq` (Rust jq).

### `nika:json_diff`
```yaml
invoke: { tool: "nika:json_diff", args: { before: { ... }, after: { ... } } }
```
JSON diff · returns **RFC 6902** JSON Patch. (jq can't diff.)

### `nika:validate` · schema validation (json OR yaml)
```yaml
invoke: { tool: "nika:validate", args: { data: { ... }, schema: { type: object, ... }, format: json } }
# format: json (default · validate a value) | yaml (parse a YAML string first, then validate)
```
Validate data against a **JSON Schema** · returns `{ valid: bool, errors: [...] }`. Merges the former `json_verify` + `yaml_validate` (`format:` arg · one validator).

### `nika:json_merge_patch`
```yaml
invoke: { tool: "nika:json_merge_patch", args: { target: { ... }, patch: { ... } } }
```
**RFC 7396** merge patch (`null` deletes a key) · the delete-on-null semantics jq's `*` recursive-merge does NOT provide (so this stays a genuine builtin). Plain recursive merge (no delete) is just `jq '.[0] * .[1]'` on a `[base, overlay]` input · no builtin needed.

### `nika:convert` · universal multi-format conversion
```yaml
invoke:
  tool: "nika:convert"
  args:
    input: "${{ tasks.A.output }}"     # the data to convert · string for text formats · structured for json/yaml/toml
    from: csv                          # REQUIRED · enum · json | yaml | toml | csv
    to: json                           # REQUIRED · enum · json | yaml | toml | csv
    has_header: true                   # OPTIONAL · CSV only · default true
```
Universal format converter · 4 formats v0.1 (`json` · `yaml` · `toml` · `csv`) · 12 directions in scope (4×3 minus identity).

Pattern · `fetch+extract` symmetry · single super-powerful builtin · `from`/`to` mode parameters · all bidirectional pairs canonical · no per-direction builtin slot.

Replaces · legacy `nika:csv_to_json` (cut per ADR-086 · D-2026-05-27 Rams sweep · the « less but better » builtin-by-builtin review that cut the canonical set to 22). The reverse direction (JSON→CSV) is ALSO covered here · jq's `@csv` filter is the in-jq alternative for that specific direction · `nika:convert` is the canonical multi-format builtin.

Reference implementation · `serde_transcode` 1.1+ orchestrator (zero-allocation walk · serde-ecosystem canonical · 15M+ downloads · sfackler) + format-specific crates · `serde_json` (JSON · already nika dep) · `serde_yaml_bw` 2.5+ (YAML · modern + maintained 2026) · `toml` 1.1+ (TOML · spec 1.1.0 compliant) · `csv` 1.4+ (CSV · quoting-aware).

### `nika:uuid`
```yaml
invoke: { tool: "nika:uuid", args: { version: v7 } }   # v7 default (timestamped/sortable · RFC 9562) | v4 (random)
```
Generate a UUID. (Generators are not jq · jq is pure transform.)

### `nika:date`
```yaml
invoke: { tool: "nika:date", args: { op: now } }
# op: now { tz } | add { base, duration } | subtract | format { input, format } | parse | diff { start, end, unit }
```
Timestamp arithmetic · op-discriminated single builtin · timezone-aware (IANA · default UTC) · ISO 8601 out.

### `nika:hash`
```yaml
invoke: { tool: "nika:hash", args: { algo: blake3, content: "${{ tasks.X.output }}", encoding: hex } }
```
Content hashing · default **blake3** (fastest modern cryptographic hash · parallel · secure) · or `sha256`/`sha512`. md5/sha1 NOT supported (cryptographically broken). Use cases · cache keys · content addressing · provenance.

---

## Network builtins (2)

### `nika:fetch`
HTTP request + content extraction (reached via `invoke:` — fetching a URL is *calling a tool*, not a verb · see `02-verbs.md`).
```yaml
invoke: { tool: "nika:fetch", args: { url: "https://example.com/article", mode: article } }
```
| Arg | Notes |
|---|---|
| `url` | required · may use `${{ ... }}` |
| `method` | `GET` (default) · `POST` · `PUT` · `DELETE` · `PATCH` · `HEAD` |
| `headers` · `body` | extra headers · body (objects auto-JSON) |
| `mode` | extraction mode · see `extract-modes-v0.1.md` · default `markdown` |
| `jq` | a jq expression · only with `mode: jq` (structured JSON extraction · replaces the former JSONPath mode) |

**Security (engine MUST)** · SSRF defense (reject private-net + cloud-metadata `169.254.169.254` unless configured) · honor task-level `timeout` · reject self-signed TLS by default.

### `nika:notify`
```yaml
invoke: { tool: "nika:notify", args: { channel: webhook, target: "https://hooks.slack.com/...", message: "Done · ${{ tasks.X.status }}", severity: info } }
```
Send notifications · `channel:` enum (`webhook`/`slack`/`email`/`discord`/`sms` · one builtin not 5). v0.81 engine MUST support `webhook` · others MAY be feature-gated (`NIKA-BUILTIN-NOTIFY-001` if unconfigured).

---

## Introspection builtins (1)

### `nika:inspect` · workflow introspection (4 views · view-discriminated)
```yaml
invoke:
  tool: "nika:inspect"
  args:
    view: cost                     # REQUIRED · enum · cost | records | dag_info | threads
```
Workflow introspection · 1 builtin · 4 `view:` enum modes (Rams collapse per ADR-088 · 2026-05-27) ·

- `view: cost` → `{ total_usd, by_task, by_provider }`. Running workflow cost.
- `view: records` → `{ tasks: [{ id, status, duration_ms, ... }] }`. Full execution record. (Per-task status is also read directly via the `${{ tasks.X.status }}` namespace — same shape.)
- `view: dag_info` → `{ nodes, edges, waves }`. DAG topology.
- `view: threads` → `{ active, queued, completed }`. Engine task-pool state · **advisory** · counts reflect the engine's concurrency model (impl-dependent · use for coarse adaptive-throttling · not a portable contract-precise number).

Replaces · 4 legacy introspection builtins (`nika:cost` · `nika:records` · `nika:dag_info` · `nika:threads`) collapsed per « less but better » Rams sweep · same trust class (PURE) · same query-own-workflow-state semantic family · the split into 4 separate builtins was historical (one-per-shape) not structural · the unified `view:` discriminator + per-shape `args:` is the canonical « one super-powerful builtin · multi-mode args » pattern (matches fetch+extract · jq · convert · wait).

Throws · `NIKA-BUILTIN-INSPECT-001` if `view:` value not in the canonical enum.

---

## What jq subsumes (cut from v0.1)

These 13 former builtins are **expressible in `nika:jq`** (jaq-verified) · cut to
keep ONE data language (« no two ways to transform data »). Canonical recipes ·

| Former builtin | jq recipe |
|---|---|
| `nika:map` | `map(EXPR)` |
| `nika:filter` | `map(select(EXPR))` |
| `nika:group_by` | `group_by(.key)` |
| `nika:aggregate` | `map(.val) \| add` |
| `nika:enrich` | `. + { fetched_at: $now }` |
| `nika:chunk` | `[range(0; length; N) as $i \| .[$i:$i+N]]` |
| `nika:flatten` (object dot-path) | `[leaf_paths as $p \| {($p\|join(".")): getpath($p)}] \| add` |
| `nika:unflatten` | `reduce to_entries[] as $e (.; setpath($e.key/"."; $e.value))` |
| `nika:inject` (template) | `"Hello \(.name), age \(.age)"` (string interpolation) |
| `nika:base64_encode` / `_decode` | `@base64` / `@base64d` |
| `nika:json_to_csv` | `@csv` |
| `nika:json_merge` (recursive `*`) | `.[0] * .[1]` on a `[base, overlay]` input (jaq `obj_merge` · source-verified 2026-05-27) |

Also cut · `nika:task_status` (use `${{ tasks.X.status }}`) · `nika:orchestrate`
(use `for_each` for bounded fan-out · sub-workflow composition is deferred per
`08-out-of-scope.md`) · `nika:locale_lookup` (niche i18n → stdlib-extended).

---

## Media builtins · DEFERRED to stdlib v0.x

Not enumerated in v0.1 (feature-flag in the reference engine · MAY graduate as a
separate doc). Deliberate « less but better » (Rams 10).

---

## Cross-builtin invariants

A v0.1-compliant builtin · takes a single `args` object · returns a
JSON-serializable value · reports errors as typed `NIKA-BUILTIN-NNN` codes ·
honors task-level `timeout` · respects engine security policies.

---

## Forward-compat

New builtins MAY enter stdlib v0.x. Builtin removal is never allowed within a
stdlib v0.x lifetime (removal requires a new stdlib major). The v0.1 → 22
consolidation (42 → 26 → 22) happened **pre-public** (0 external users · before the forever-clock).

---

🦋 *<!-- canon:builtins -->22<!-- /canon --> builtins canonical · jq = the data language · 5-layer Rams symmetry (fetch+extract · jq · convert · wait · inspect) · clear forever.*
