# Stdlib v0.1 · Builtins

> **<!-- canon:builtins -->24<!-- /canon --> canonical builtins** shipped with Stdlib v0.1-compliant engines.
> Invoked via `invoke: tool: "nika:<name>"`. Plus the remaining media
> builtins deferred to stdlib v0.x (opt-in feature flag).
>
> **Consolidation (« less but better »)** · was 42 → 26 (D-N6) →
> **22**. Step 1 (42 → 26 · D-N6) · `nika:jq` is THE data language · 13 thin
> wrappers that jq subsumes were cut (incl. `json_merge` = jq's recursive `*` ·
> jaq source-verified 2026-05-27 · `obj_merge` impl + test corpus + corelang
> docs) · the validators merged · `task_status`/`orchestrate`/`locale_lookup`
> cut. Step 3 (22 → 23 · 2026-06-13) · `nika:compose` · the agent loop's
> self-verification intrinsic (ADR-093 · loop-only like `done`). Step 2
> (26 → 22 · ADR-086/087/088 Rams sweep 2026-05-27) · `convert`
> replaces `csv_to_json` (multi-format · from:/to:) · `wait` unifies
> `sleep`+`wait_until` (−1) · `inspect` unifies `cost`+`records`+`dag_info`+
> `threads` (−3). ZERO capability loss (jq ⊇ the cuts · jaq-verified · the
> collapses preserve every behavior via mode args). See §"What jq subsumes".
> Step 4 (23 → 24 · 2026-07-05) · `nika:image_generate` · the FIRST §Media
> graduate. Providers v1.1 (local-first per the sovereignty review
> 2026-07-05) · `local` (any OpenAI-images-compatible self-hosted server —
> LocalAI · Ollama · sd.cpp · SGLang · vLLM-Omni) · openai `gpt-image-2` ·
> gemini `gemini-3.1-flash-image` · xai `grok-imagine-image` · `mock` for
> offline runs · assets land on disk, never inline base64.
> Step 5 (24 → 25 · 2026-07-05) · `nika:tts_generate` · the SECOND
> Media-class graduate (stdlib §Audio) · sovereign-first — `local` (any
> OpenAI-speech-compatible server: LocalAI · Kokoro-FastAPI · Speaches ·
> openedai-speech) · openai `gpt-4o-mini-tts` · elevenlabs · `mock` (a
> real deterministic WAV · offline CI) · ONE audio file on disk, never
> inline bytes.

---

## Builtin categories

| Category | Count | Status |
|---|---|---|
| Core | 6 | Required for execution (log · emit · assert · prompt · done · wait) |
| File | 5 | I/O primitives (read · write · edit · glob · grep) |
| Data | 8 | `jq` (THE data language) + 7 capabilities jq can't express (json_diff · validate · json_merge_patch · convert · uuid · date · hash) |
| Introspection | 2 | Self-awareness · `inspect` (runtime state · 4 views) · `compose` (static check of a drafted workflow · agent loops only) |
| Network | 2 | fetch (HTTP+extraction) · notify (alerts out) |
| Media | 2 | `image_generate` (§Media · 2026-07-05) · `tts_generate` (§Audio · same day) — the REST of the media class stays deferred to stdlib v0.x |
| **Total v0.1** | **25** | |

A Stdlib v0.1-compliant engine MUST ship these 25.

---

## Core builtins (6)

### `nika:log`
```yaml
invoke: { tool: "nika:log", args: { level: info, message: "Processing ${{ vars.user_id }}", data: { foo: "bar" } } }
```
Emit a log entry (`debug`/`info`/`warn`/`error`) to the workflow event stream (human-readable diagnostic).

Returns `null` · best-effort (no failure codes · a log that cannot land never fails the task).

### `nika:emit`
```yaml
invoke: { tool: "nika:emit", args: { event_type: custom.event, payload: { ... } } }
```
Emit a custom machine event (consumed by subscribers · journal). Distinct from `log` · `log` = human diagnostic · `emit` = machine event.

`event_type:` matches `^[a-z][a-z0-9_.-]*$` · `payload:` any JSON value. Returns `null`. Delivery is engine-side (journal · subscribers) · best-effort once shape-valid. Throws · `NIKA-BUILTIN-EMIT-001` (invalid event shape · `validation_error`).

### `nika:assert`
```yaml
invoke: { tool: "nika:assert", args: { condition: "${{ tasks.X.output.count > 0 }}", message: "Expected non-empty result" } }
```
Fail the task if `condition` (a **CEL `${{ }}` boolean**) is false · else no-op. The **fail-fast guard** (distinct from `when:` which is the **skip-guard**). `condition:` uses the canonical `${{ }}` CEL surface, never a legacy `$task` syntax.

Returns `true` on pass. Throws · `NIKA-BUILTIN-ASSERT-001` (assertion failed · `tool_error` · `transient: false` · `message:` lands in the error message · retryable via `retry.on_codes`, the polling pattern's lever).

### `nika:prompt`
```yaml
# confirm (default) — a yes/no gate
invoke: { tool: "nika:prompt", args: { message: "Approve deploy to production?", default: false } }
# input — collect a free-text value
invoke: { tool: "nika:prompt", args: { mode: input, message: "Paste the OTP:", default: "" } }
# choice — pick one of N
invoke: { tool: "nika:prompt", args: { mode: choice, message: "Which title?", choices: ["${{ tasks.a.output }}", "${{ tasks.b.output }}"] } }
```
Interactive human-in-the-loop · blocks until answered. **`mode:` selects what
is collected** (default `confirm`) ·

| `mode` | Returns | Notes |
|---|---|---|
| `confirm` (default) | **boolean** | `true` = confirmed · `false` = refused. A refusal is a VALUE, never an error: gate downstream with `when:`. |
| `input` | **string** | the free text the human typed (may be empty). |
| `choice` | **string** | the chosen element of `choices:` (required · non-empty array). Returns the chosen value (not its index) so it binds directly. |

Non-interactive contract (normative · all modes) · when no human can answer
(CI · daemon) the engine MUST use `default:` when present · and MUST fail
`NIKA-BUILTIN-PROMPT-001` (`validation_error` · non-interactive without a
`default:`) when absent: never hang forever · never silently pick an answer.
A `choice` whose `default:` is not an element of `choices:` is a parse error
(`NIKA-BUILTIN-PROMPT-002` · `validation_error`).

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
Read a file · returns **string** content (text mode · the default).
`binary: true` (explicit · no content sniffing) returns **opaque bytes**
([04 §value rendering](../spec/04-variables.md) · they flow tool→tool ·
never into a string position). Throws · `NIKA-BUILTIN-READ-001` (file not
found, the code the state-file first-run pattern scopes its recovery to) ·
`-002` (IO failure · permission) · `-003` (text mode on non-UTF-8 content ·
use `binary: true`). All `tool_error` · `transient: false`.

### `nika:write`
```yaml
invoke: { tool: "nika:write", args: { path: "./out.md", content: "...", create_dirs: true, overwrite: true } }
```
Write a file · returns the path. A binary `content` value (an opaque bytes output from an upstream tool · e.g. MCP image content) is written as-is · no `output_format` declaration needed (the value carries its own type).

`overwrite:` defaults **true** · `create_dirs:` defaults **false**. Throws · `NIKA-BUILTIN-WRITE-001` (IO failure) · `-002` (`overwrite: false` and the path exists). Both `tool_error` · `transient: false`.

### `nika:edit`
```yaml
invoke: { tool: "nika:edit", args: { path: "./file.md", find: "old", replace: "new" } }
```
In-place find/replace · returns the modified path. `find:` is a **literal
string** (not a regex: use `nika:grep` to locate · jq to transform) ·
replaces **all occurrences** · `count:` caps replacements when set. Throws ·
`NIKA-BUILTIN-EDIT-001` (`find:` matched nothing: an edit that edits
nothing is an authoring bug · `tool_error`) · `-002` (IO failure).

### `nika:glob`
```yaml
invoke: { tool: "nika:glob", args: { pattern: "./src/**/*.rs", exclude: ["**/target/**"] } }
```
Glob match · returns array of paths · **sorted lexicographically**
(deterministic across engines · filesystems). Throws ·
`NIKA-BUILTIN-GLOB-001` (invalid pattern · `validation_error`).

### `nika:grep`
```yaml
invoke: { tool: "nika:grep", args: { pattern: "TODO:", path: "./src", case_insensitive: false } }
```
Recursive grep · returns array of `{ path, line, match }` · `line` is the
1-based line **number** (integer) · `match` the matched line text · results
sorted by `(path, line)`. `pattern:` is a **Rust-regex-class** expression
(RE2-compatible · no backreferences · the portable subset). Throws ·
`NIKA-BUILTIN-GREP-001` (invalid pattern · `validation_error`).

---

## Data builtins (8) · `jq` is THE data language

### `nika:jq` · the transform + extraction primitive
```yaml
invoke: { tool: "nika:jq", args: { expression: ".items | map(.price) | add", input: "${{ tasks.X.output }}" } }
```
Run a jq expression. **The single data-transform-and-extraction language**: map · filter · select · group_by · reshape · string-interpolation `"\(.x)"` · `@base64`/`@base64d`/`@csv` encoders · array `flatten` · `leaf_paths`/`getpath`/`setpath`. The same jq used in `output:` bindings (see `04-variables.md`).

**`input` is any JSON value**: a single ref (`input: "${{ tasks.X.output }}"`) OR a **constructed array for multi-input ops**. Recursive merge of two objects (this is exactly why `json_merge` is NOT a builtin · jaq's `*` does it) ·
```yaml
invoke:
  tool: nika:jq
  args:
    input: ["${{ tasks.base.output }}", "${{ tasks.overlay.output }}"]
    expression: ".[0] * .[1]"      # recursive deep-merge · overlay wins
```
Same shape combines / zips N inputs · build the array, index inside jq.

**The arg is `expression:`**, exactly that name (not `query:` · not `expr:` ·
one name everywhere · the conformance oracle gates it). Throws ·
`NIKA-BUILTIN-JQ-001` (program error at runtime · `tool_error`). Compile
errors are caught statically (`NIKA-VAR-005`).

**Implementation** · reference engine uses `jaq` (Rust jq).

### `nika:json_diff`
```yaml
invoke: { tool: "nika:json_diff", args: { before: { ... }, after: { ... } } }
```
JSON diff · returns **RFC 6902** JSON Patch. (jq can't diff.) Throws · `NIKA-BUILTIN-JSON_DIFF-001` (non-JSON input · `validation_error`).

### `nika:validate` · schema validation (json OR yaml)
```yaml
invoke: { tool: "nika:validate", args: { data: { ... }, schema: { type: object, ... }, format: json } }
# format: json (default · validate a value) | yaml (parse a YAML string first, then validate)
```
Validate data against a **JSON Schema** · returns `{ valid: bool, errors: [...] }`. Invalid DATA is a **report, never a task failure** (gate on `.valid` downstream · or `nika:assert` it). Merges the former `json_verify` + `yaml_validate` (`format:` arg · one validator). Throws · `NIKA-BUILTIN-VALIDATE-001` (the `schema:` itself is not a valid JSON Schema · `validation_error`) · `-002` (`format: yaml` and the string does not parse as YAML).

### `nika:json_merge_patch`
```yaml
invoke: { tool: "nika:json_merge_patch", args: { target: { ... }, patch: { ... } } }
```
**RFC 7396** merge patch (`null` deletes a key) · the delete-on-null semantics jq's `*` recursive-merge does NOT provide (so this stays a genuine builtin). Plain recursive merge (no delete) is just `jq '.[0] * .[1]'` on a `[base, overlay]` input · no builtin needed. Throws · `NIKA-BUILTIN-JSON_MERGE_PATCH-001` (non-object target/patch · `validation_error`).

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
Universal format converter · 4 formats v0.1 (`json` · `yaml` · `toml` · `csv`) · 12 directions in scope (4×3 minus identity) · `from == to` is rejected (`NIKA-BUILTIN-CONVERT-001` · `validation_error` · an identity conversion is an authoring bug). Throws · `-002` (the input does not parse as `from:` · `tool_error`).

Pattern · `fetch+extract` symmetry · single super-powerful builtin · `from`/`to` mode parameters · all bidirectional pairs canonical · no per-direction builtin slot.

Replaces · legacy `nika:csv_to_json` (cut per ADR-086 · D-2026-05-27 Rams sweep · the « less but better » builtin-by-builtin review that cut the canonical set to 22). The reverse direction (JSON→CSV) is ALSO covered here · jq's `@csv` filter is the in-jq alternative for that specific direction · `nika:convert` is the canonical multi-format builtin.

Reference implementation · `serde_transcode` 1.1+ orchestrator (zero-allocation walk · serde-ecosystem canonical · 15M+ downloads · sfackler) + format-specific crates · `serde_json` (JSON · already nika dep) · `serde_yaml_bw` 2.5+ (YAML · modern + maintained 2026) · `toml` 1.1+ (TOML · spec 1.1.0 compliant) · `csv` 1.4+ (CSV · quoting-aware).

### `nika:uuid`
```yaml
invoke: { tool: "nika:uuid", args: { version: v7 } }   # v7 default (timestamped/sortable · RFC 9562) | v4 (random)
```
Generate a UUID. (Generators are not jq · jq is pure transform.) Returns the canonical lowercase-hyphenated string. No failure codes.

### `nika:date`
```yaml
invoke: { tool: "nika:date", args: { op: now } }
# op: now { tz } | add { base, duration } | subtract | format { input, format } | parse | diff { start, end, unit }
```
Timestamp arithmetic · op-discriminated single builtin · timezone-aware (IANA · default UTC) · ISO 8601 out. `format:`/`parse` use the **strftime** field grammar (`%Y-%m-%d` · the one cross-language constant). Every op returns a string EXCEPT `diff` (integer · in `unit:`). Throws · `NIKA-BUILTIN-DATE-001` (unparseable input / unknown op / bad tz · `validation_error`).

### `nika:hash`
```yaml
invoke: { tool: "nika:hash", args: { algo: blake3, content: "${{ tasks.X.output }}", encoding: hex } }
```
Content hashing · default **blake3** (fastest modern cryptographic hash · parallel · secure) · or `sha256`/`sha512`. md5/sha1 NOT supported (cryptographically broken · `NIKA-BUILTIN-HASH-001` `validation_error` on an unsupported algo). `encoding:` `hex` (default) | `base64`. Use cases · cache keys · content addressing · provenance.

---

## Network builtins (2)

### `nika:fetch`
HTTP request + content extraction (reached via `invoke:` because fetching a URL is *calling a tool*, not a verb · see `02-verbs.md`).
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

**Non-2xx is failure (normative)** · a non-2xx response throws
`NIKA-BUILTIN-FETCH-001` (`category: network_error` · `transient: true` for
5xx/408/429 · `false` for other 4xx · `details.status_code` carries the
status). To poll a pending resource · the jq-error pattern
([08 H19](../spec/08-out-of-scope.md)), not status-code inspection.
Redirects follow up to an engine cap · the FINAL status decides.

**Security (engine MUST)** · SSRF defense (reject private-net + cloud-metadata `169.254.169.254` unless configured) · honor task-level `timeout` · reject self-signed TLS by default.

### `nika:notify`
```yaml
invoke: { tool: "nika:notify", args: { channel: webhook, target: "https://hooks.slack.com/...", message: "Done · ${{ tasks.X.status }}", severity: info, data: { run: "${{ tasks.X.output }}" } } }
```
Send notifications · `channel:` enum (`webhook`/`slack`/`email`/`discord`/`sms` · one builtin not 5). The 1.0 engine MUST support `webhook` · others MAY be feature-gated. `data:` (OPTIONAL · any JSON value) carries structured context alongside the human `message`: the webhook payload is `{ message, severity, data? }` (the key is absent when not given · receivers branch on machine fields, never parse the message). Returns `null` on accepted delivery. Throws · `NIKA-BUILTIN-NOTIFY-001` (channel unconfigured · `validation_error`) · `-002` (delivery failed · `network_error` · transient engine-assessed).

---

## Introspection builtins (2)

### `nika:compose` · self-check a drafted workflow (agent loops only)
```yaml
agent:
  prompt: "Draft a workflow, then check it before you finish."
  tools: ["nika:compose"]            # the agent grants itself the self-check
```
The agent loop's self-verification intrinsic · the model passes a workflow
YAML draft it wrote, and gets the FULL `nika check` verdict back as JSON:
conformance violations (with codes + repair hints), secret-flow findings,
permits escapes, and the termination/cost certificate. It **never executes**
the draft (« generation is not permission » · the draft is an artifact + its
certificate · running it stays a separate, gated decision). Iterate until
`valid` is true, then deliver the draft.

Loop-served like `nika:done`: **valid only inside an `agent:` tool whitelist**
(a standalone `invoke: nika:compose` is rejected · `NIKA-BUILTIN-COMPOSE-001`).
The model never sees it unless it is whitelisted (default-deny).

Args · `workflow_yaml` (string · required · the complete draft).

### `nika:inspect` · workflow introspection (4 views · view-discriminated)
```yaml
invoke:
  tool: "nika:inspect"
  args:
    view: cost                     # REQUIRED · enum · cost | records | dag_info | threads
```
Workflow introspection · 1 builtin · 4 `view:` enum modes (Rams collapse per ADR-088 · 2026-05-27) ·

- `view: cost` → `{ total_usd, by_task, by_provider }`. Running workflow cost.
- `view: records` → `{ tasks: [{ id, status, duration_ms, ... }] }`. Full execution record. (Per-task status is also read directly via the `${{ tasks.X.status }}` namespace, same shape.)
- `view: dag_info` → `{ nodes, edges, waves }`. DAG topology.
- `view: threads` → `{ active, queued, completed }`. Engine task-pool state · **advisory** · counts reflect the engine's concurrency model (impl-dependent · use for coarse adaptive-throttling · not a portable contract-precise number).

Replaces · 4 legacy introspection builtins (`nika:cost` · `nika:records` · `nika:dag_info` · `nika:threads`) collapsed per « less but better » Rams sweep · same trust class (PURE) · same query-own-workflow-state semantic family · the split into 4 separate builtins was historical (one-per-shape) not structural · the unified `view:` discriminator + per-shape `args:` is the canonical « one super-powerful builtin · multi-mode args » pattern (matches fetch+extract · jq · convert · wait).

Throws · `NIKA-BUILTIN-INSPECT-001` if `view:` value not in the canonical enum.

---

## Media builtins (1)

### `nika:image_generate` · provider-backed image asset generation

```yaml
invoke:
  tool: "nika:image_generate"
  args:
    provider: mock                # mock (offline) | local (sovereign) | gemini | openai | xai — inferable from model: (local excepted)
    prompt: "OG hero — a monarch butterfly over a nebula, editorial photo"
    aspect_ratio: "16:9"
    output_dir: "./assets/og"
    filename_prefix: "launch-hero"
    metadata: { campaign: "spring", page_slug: "qr-menu" }
```

Text-to-image generation as an *asset pipeline*, not a blob pipe: images are
**saved to `output_dir:`** and the output carries `paths + dimensions +
sha256 (+ a provenance manifest)` — **image bytes NEVER ride workflow
outputs** (no base64 in `tasks.X.output`, logs, or traces · normative).

| Arg | Notes |
|---|---|
| `provider` | `local` · `openai` · `gemini` · `xai` · `mock` — optional when inferable from `model:` (`gpt-image*`→openai · `gemini-*`→gemini · `grok*`→xai · `mock*`→mock · `local` is NEVER inferred: its model names are server-specific) |
| `model` | per-provider default (reference engine 2026-07: `stablediffusion` for local — the LocalAI convention · SD-family servers also honor the `positive \| negative` split INSIDE `prompt:` (LocalAI pipe syntax) — no separate arg needed · `gpt-image-2` · `gemini-3.1-flash-image` · `grok-imagine-image` — the `-quality` tier is the model knob · `mock-image-1`) |
| `prompt` | **required** · the creative brief · may use `${{ … }}` |
| `mode` | `generate` (default) · `edit` is RESERVED (rejected loudly in v0.1 · media roadmap) |
| `n` | 1..=10 variants (engines MAY satisfy n via sequential provider calls · documented per adapter) |
| `aspect_ratio` | closed set `1:1 · 16:9 · 9:16 · 4:3 · 3:4 · 3:2 · 2:3 · 21:9` |
| `size` | exact `WIDTHxHEIGHT` or `auto` · an exact size WINS over `aspect_ratio:` (with a warning) · providers that render size CLASSES fold it (loudly) |
| `quality` | `auto · low · medium · high · ultra` — folded per provider capability, never silently |
| `format` | `png` (default) · `jpeg` · `webp` — **magic bytes are the authority**: what actually landed decides mime/extension, a mismatch is a warning |
| `compression` | 0..=100 · jpeg/webp only |
| `background` | `auto · transparent · opaque` · transparent REQUIRES an alpha-capable format (png/webp) and a supporting provider/model |
| `seed` | best-effort (providers without seed support warn + drop) |
| `reference_images` | RESERVED (rejected loudly in v0.1 · media roadmap) |
| `provider_options` | vetted pass-through (unknown keys warn, never crash) · openai `{moderation, user}` · gemini `{thinking_level, image_size}` · xai `{user, resolution: 1k\|2k}` |
| `output_dir` | **required** · rides the declared `permits.fs` boundary (`NIKA-SEC-004` · gated per final path BEFORE any I/O) |
| `filename_prefix` | filename stem (else `metadata.page_slug`, else `image`) — sanitized `[a-z0-9._-]`, traversal-free by construction |
| `save` | `true` (v0.1 contract · `save: false` is RESERVED — rejected loudly) |
| `manifest` | write the provenance manifest JSON beside the assets (default `true`) |
| `metadata` | free provenance object (campaign · page_slug · locale · …) echoed into output + manifest |
| `timeout_ms` | per-request deadline · default 180000 (local: 300000 — CPU renders run minutes) · 1000..=600000 |
| `debug` | echo the sanitized raw provider response (base64 payloads STRIPPED · headers never included) |

**Filenames (normative grammar)** ·
`{stem}-{provider}-{modelslug}-{index}-{sha8}.{ext}` · every component
sanitized to `[a-z0-9._-]` with no separators; name collisions probe
`-2..-99`; an IDENTICAL payload already on disk is an idempotent re-run
(no duplicate). The manifest lands beside the assets as
`{stem}-{provider}-{modelslug}-{batchsha8}.manifest.json`
(`manifest_version: 1` · resolved request echo · per-image
paths/dimensions/sha256 · usage · warnings · caller `metadata:` — and
NEVER a credential: keys live a composition layer away by construction).

**Output (normalized · every provider + mock)** · `{ provider, model, mode,
prompt, revised_prompt, provider_text, created_at, count, images: [{ index,
path, filename, mime_type, format, width, height, size_bytes, sha256,
provider, model, seed, variant_id, warnings, metadata }], usage:
{ input_tokens, output_tokens, total_tokens, thoughts_tokens }, cost_usd,
warnings, manifest_path, output_dir }` — absent usage axes are `null`,
never zero-that-looks-real. `cost_usd` is the render's REAL spend when
the provider reports it exactly (xai bills `cost_in_usd_ticks` in the
response · 1 cent = 10⁸ ticks) and `null` otherwise — never an estimate
dressed as truth. **Invoke-cost metering (normative)** · a tool whose
structured output carries a top-level numeric `cost_usd` reports real
spend; engines SHOULD meter it into the run's cost ledger through the
same honest-spend channel `infer:` rides.

**Security (engine MUST)** · provider endpoints are ENGINE-FIXED constants
(never workflow data — the provider egress is engine transport, exactly like
`infer:`; `permits.net.http` does not govern it, `permits.tools` +
`permits.fs` DO; the `local` base URL is ENGINE CONFIG resolved at the
composition root — `NIKA_IMAGE_LOCAL_URL`, default LocalAI's
`http://localhost:8080` — still never workflow data) · decode validation is HEADER-ONLY (magic bytes + PNG
IHDR / JPEG SOF / WebP VP8-VP8L-VP8X dimensions · no pixel decode → no
decompression-bomb surface · declared-vs-actual mismatch is a warning, a
non-image payload is a hard error) · atomic writes (temp+rename) · API keys
are engine-configured (env/config at the composition root), never workflow
args, never logged, never echoed.

**Warnings (normative shape)** · every tolerated-but-lossy mapping lands a
stable `code: message` warning string (`size_conflict:` ·
`compression_ignored:` · `seed_unsupported:` · `quality_folded:` ·
`format_mismatch:` · `gemini_size_class:` · `xai_size_class:` ·
`aspect_remapped:` · `count_shortfall:` · `revised_prompt_clamped:` ·
`provider_option_unknown:` · `provider_text_clamped:` · …) in `warnings` — silent degradation is
non-conformant.

**`provider_text`/`revised_prompt` are captions, not payload channels
(normative)** · a provider's accompanying text MUST be bounded by the
engine (the reference engine clamps at 2 000 chars with a
`provider_text_clamped:` warning) — a multimodal response interleaving
megabytes of text (or base64-shaped junk) must never ride workflow
outputs, the manifest, or the `debug:` echo unbounded.

**Result URLs are never fetched (normative)** · engines request
`response_format: b64_json` on url-capable wires (openai-compat · xai) and
MUST refuse a url-only response with an actionable error — fetching a
provider-supplied result URL would reopen the SSRF/net-boundary surface
the const-endpoint design closed. The provenance manifest and output
carry `endpoint_host` (which server actually rendered the asset — load-
bearing for `local`, where the endpoint is configurable).

**Provenance travels IN the file (PNG)** · engines SHOULD embed the
deterministic provenance core (tool · engine version · provider · model ·
clamped prompt · seed — no timestamp, so byte-determinism holds) as a
`nika` tEXt chunk in saved PNG renders — the ComfyUI/InvokeAI
interchange practice: a sidecar manifest answers « where does this come
from? » only until the file is copied without it. JPEG/WebP have no
equally-universal text container and are documented as manifest-only,
never silently faked.

Throws · `NIKA-BUILTIN-IMAGE_GENERATE-001` invalid arguments (incl. the v0.1
RESERVED options · `validation_error`) · `-002` provider unavailable
(missing credentials / image plane unwired · `validation_error`) · `-003`
provider request failed (`network_error` · `transient: true` for
5xx/408/429 + timeout/connection · `details.status_code`) · `-004` no
image / malformed response (`tool_error`) · `-005` content policy block
(moderation / safety finish reasons · `security_error` · never transient ·
`details {finish_reason? · moderation_details?}`) · `-006` save/manifest
write failed (`tool_error`) · `-007` image validation failed (magic
mismatch · dimension/byte bounds · `tool_error`). Plus the boundary
`NIKA-SEC-004` (an `output_dir:` outside `permits.fs.write`).

---

### `nika:tts_generate` · provider-backed speech synthesis (§Audio)

```yaml
- id: narrate
  invoke:
    tool: "nika:tts_generate"
    args:
      provider: local              # sovereign first — or openai · elevenlabs · mock
      text: "Bienvenue — the launch is live."
      voice: "alloy"
      output_dir: "./assets/audio"
```

The image family's contract, applied to audio: ONE audio file lands under
`output_dir:` (permit-gated per final path BEFORE I/O · atomic write ·
content-hash-named `{stem}-{provider}-{model}-{sha8}.{ext}` so identical
re-runs are idempotent), the output carries `{ provider, model, voice,
created_at, endpoint_host, audio: { path, filename, format, mime_type,
size_bytes, sha256, duration_ms }, cost_usd, warnings, manifest_path,
output_dir }` — **audio bytes NEVER ride outputs**, and a sidecar
provenance manifest (`manifest: true` default) echoes the resolved
request.

| Arg | Contract |
|---|---|
| `provider` | REQUIRED-or-inferred · closed: `local` (any OpenAI-speech-compatible self-hosted server — LocalAI · Kokoro-FastAPI · Speaches · openedai-speech · ONE wire: `POST {base}/v1/audio/speech` → raw bytes · base URL is ENGINE CONFIG `NIKA_TTS_LOCAL_URL`, default `http://localhost:8080`, never workflow data · never inferred) · `openai` · `elevenlabs` · `mock` (a REAL deterministic WAV · zero network/keys). Inference: `eleven*` → elevenlabs · `gpt-*`/`tts-*` → openai · `mock*` → mock. |
| `model` | per-provider default (reference engine 2026-07: `tts-1` local convention · `gpt-4o-mini-tts` · `eleven_multilingual_v2` · `mock-tts-1`) |
| `text` | REQUIRED · non-empty · ≤4096 chars (the strictest documented wire cap, held portably — fan longer scripts out with `for_each`) |
| `voice` | provider voice id · defaults: `alloy` (openai/local) · Rachel's public id (elevenlabs — ids are path components, so engines MUST restrict them to the id alphabet) · `sine` (mock) |
| `format` | `mp3 \| wav \| auto` (default auto = provider-native) — the saved EXTENSION follows the sniffed bytes (magic authority), never the ask; a lossy fold is a `format_mismatch:` warning |
| `speed` | 0.25–4.0 (openai/local native) — warned-dropped (`speed_unsupported:`) where the wire has no knob |
| `output_dir` · `filename_prefix` · `metadata` · `manifest` | as `image_generate` (stem falls back to `metadata.page_slug`, then `speech`) |
| `timeout_ms` | default 120000 · local 300000 (CPU synthesis) · max 600000 |

**Validation (engine MUST)** · the payload is sniffed (WAV `RIFF…WAVE` ·
MP3 `ID3`/frame-sync) — a non-audio payload is a hard `-007`; WAV
`duration_ms` is exact header math, MP3 duration is honestly `null`
(never a guess). Empty payloads are `-004`.

**Security (engine MUST)** · endpoints are engine-fixed constants (the
`local` base URL is engine config at the composition root — the same
sanction as `image_generate` and `infer:`) · keys are engine-configured
(`OPENAI_API_KEY` · `ELEVENLABS_API_KEY` · `NIKA_TTS_LOCAL_API_KEY` or
the `NIKA_`-prefixed forms), never workflow args, never logged · a
verbose local server reflecting the Bearer into an error body MUST be
scrubbed (the image family's rule).

Throws · `NIKA-BUILTIN-TTS_GENERATE-001` invalid arguments
(`validation_error`) · `-002` provider unavailable (`validation_error`) ·
`-003` request failed (`network_error` · `transient: true` for
5xx/408/429 + timeout/connection) · `-004` empty audio · `-005` content
policy · `-006` save failed · `-007` payload validation (all
`runtime_error` unless noted).

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

## Media builtins · the REST stays DEFERRED (stdlib v0.x)

`image_generate` graduated 2026-07-05 (above). The remaining media class
(pdf_extract · svg_render · chart · phash · provenance · image *editing* ·
…) is NOT enumerated in v0.1 (feature-flag in the reference engine · MAY
graduate builtin-by-builtin per the 3-razor admission test). Deliberate
« less but better » (Rams 10).

---

## Cross-builtin invariants

A v0.1-compliant builtin · takes a single `args` object · returns a
JSON-serializable value · reports errors as typed `NIKA-BUILTIN-<NAME>-NNN` codes (4-segment · per-builtin sub-namespace · [05](../spec/05-errors.md)) ·
honors task-level `timeout` · respects engine security policies.

---

## Forward-compat

New builtins MAY enter stdlib v0.x. Builtin removal is never allowed within a
stdlib v0.x lifetime (removal requires a new stdlib major). The v0.1 → 22
consolidation (42 → 26 → 22 · then +`compose` → 23 · then +`image_generate`
→ 24 · the additive §Media graduation path) happened **pre-public** (0
external users · before the forever-clock).

---

🦋 *<!-- canon:builtins -->24<!-- /canon --> builtins canonical · jq = the data language · 5-layer Rams symmetry (fetch+extract · jq · convert · wait · inspect) · assets land on disk, never inline · clear forever.*
