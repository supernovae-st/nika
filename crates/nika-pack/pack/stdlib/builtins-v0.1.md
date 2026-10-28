# Stdlib v0.1 · Builtins

> **<!-- canon:builtins -->31<!-- /canon --> canonical builtins** shipped with Stdlib v0.1-compliant engines.
> Invoked via `invoke: { tool: "nika:<name>" }`. Plus the remaining media
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
| File | the `category: file` rows of [`canon/builtins.yaml`](../canon/builtins.yaml) | I/O primitives (read · write · edit · remove_file · glob · grep) |
| Data | 8 | `jq` (THE data language) + 7 capabilities jq can't express (json_diff · validate · json_merge_patch · convert · uuid · date · hash) |
| Introspection | 2 | Self-awareness · `inspect` (runtime state · 4 views) · `compose` (static check of a drafted workflow · agent loops only) |
| Network | 2 | fetch (HTTP+extraction) · notify (alerts out) |
| Media | 6 | `chart` (§Media #4 · deterministic/attested · 2026-07-09) · `image_compose` (§Media #5 · deterministic composition · 2026-10-26) · `image_generate` (§Media · 2026-07-05) · `image_fx` (§Media #3 · deterministic ops chain · 2026-07-09) · `tts_generate` (§Audio · 2026-07-05) · `video_compose` (§Media #6 · native video assembly · 2026-10-28) — the REST of the media class stays deferred to stdlib v0.x |
| **Total v0.1** | **<!-- canon:builtins -->31<!-- /canon -->** | |

A Stdlib v0.1-compliant engine MUST ship these <!-- canon:builtins -->31<!-- /canon -->.

---

## Core builtins (6)

### `nika:log`
```yaml
invoke: { tool: "nika:log", args: { level: info, message: "Processing ${{ inputs.user_id }}", data: { foo: "bar" } } }
```
Emit a log entry (`debug`/`info`/`warn`/`error`) to the workflow event stream (human-readable diagnostic).

Returns `null` · best-effort (no failure codes · a log that cannot land never fails the task).

### `nika:emit`
```yaml
invoke: { tool: "nika:emit", args: { event_type: custom.event, payload: { ... } } }
```
Emit a custom machine event (consumed by subscribers · journal).

> **Why this is not `log` with a different word (normative rationale · 2026-08-11).**
> The separation was stated as *"`log` = human diagnostic · `emit` = machine
> event"* — a claim about the intended READER, and a reader is a property of the
> consumer, not of the write. That reasoning would not have survived the
> collapse sweep that merged `sleep`+`wait_until` and four introspection
> builtins. **The load-bearing difference is the FAILURE MODE**, and it is
> structural: `nika:log` has **no failure codes at all** — a log that cannot
> land never fails its task — while `emit` **validates the event shape** and
> refuses `NIKA-BUILTIN-EMIT-001`. Collapsing them forces one of two
> regressions: either every diagnostic becomes failable (73 files in the
> measured corpus rely on it not being), or machine events lose their shape
> gate. Two builtins, two contracts, one event stream — **kept for what they
> promise on failure, not for who reads them.**

`event_type:` matches `^[a-z][a-z0-9_.-]*$` · `payload:` any JSON value. Returns `null`. Delivery is engine-side (journal · subscribers) · best-effort once shape-valid. Throws · `NIKA-BUILTIN-EMIT-001` (invalid event shape · `validation_error`).

### `nika:assert`
```yaml
with: { result: "${{ tasks.X.output }}" }      # the binding is the edge · the args read the local name
invoke: { tool: "nika:assert", args: { condition: "${{ with.result.count > 0 }}", message: "Expected non-empty result" } }
```
Fail the task if `condition` (a **CEL `${{ }}` boolean**) is false · else no-op. The **fail-fast guard** (distinct from `when:` which is the **skip-guard**). `condition:` uses the canonical `${{ }}` CEL surface, never a legacy `$task` syntax.

Returns `true` on pass. Throws · `NIKA-BUILTIN-ASSERT-001` (assertion failed · `tool_error` · `transient: false` · `message:` lands in the error message · retryable via `retry.on_codes`, the polling pattern's lever). A red last assert **quarantines** writes from that run (`out/` moves to `.nika/quarantine/<trace>/` in the reference engine — hint `assert-quarantine`). Keep the assert off the last wave if you need the artifacts from a red run.

### `nika:prompt`
```yaml
# confirm (default mode) — require an answer before deployment
invoke: { tool: "nika:prompt", args: { message: "Approve deploy to production?" } }
# input — collect a free-text value
invoke: { tool: "nika:prompt", args: { mode: input, message: "Paste the OTP:", default: "" } }
# choice — pick one of N (the candidates arrive through with:)
with: { a: "${{ tasks.a.output }}", b: "${{ tasks.b.output }}" }
invoke: { tool: "nika:prompt", args: { mode: choice, message: "Which title?", choices: ["${{ with.a }}", "${{ with.b }}"] } }
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
`PROMPT-001` names only that unanswerable prompt, the one failure an engine
may turn into a durable pause awaiting an answer. Every configuration error
is a parse error (`NIKA-BUILTIN-PROMPT-002` · `validation_error`), judged
before anyone is asked, attended or not, and never a pause, since no answer
repairs it: a missing `message:`, an unknown `mode:`, an empty `choices:`, and
a `default:` the mode cannot use — a `choice` default that is not an element
of `choices:`, an `input` default that is not a string, a `confirm` default
that is neither a boolean nor a word a person answers a confirm with (`yes` ·
`y` · `no` · `n` · `true` · `false`, case-insensitive, read as that answer;
YAML 1.2 reads an unquoted `yes` as a string).

For a workflow that requires fresh human approval, omit `default:` from the
confirmation prompt and gate the effect on its answer. `default: false` is an
automatic refusal in a non-interactive run; it does not request an answer or
pause for a human. Keep an unattended refusal in the invocation instead of
changing the workflow's human gate; for example, the reference engine accepts
`nika run workflow.nika --answer approval=false` when the prompt task is named
`approval`. See the [human-gated shipping template](../templates/human-gated-ship.nika).

### `nika:done`
```yaml
agent:
  prompt: "Fix the failing test, then finish"
  tools: ["nika:read", "nika:done"]   # the model calls nika:done with { result: { status: "complete" } }
```
Mark the current `agent:` loop complete and exit · the loop-completion sentinel (**valid only inside an `agent:` tool whitelist** · error elsewhere).

### `nika:wait` · temporal wait (relative OR absolute)
```yaml
invoke:
  tool: "nika:wait"
  args:
    duration: "5s"                  # RELATIVE · Go-duration string ("500ms"/"30s"/"5m"/"1h30m")
    # OR, exactly one of the two (both is NIKA-BUILTIN-001) ·
    # until: "2026-05-23T09:00:00Z" # ABSOLUTE · ISO 8601 timestamp · MAY be CEL expression
    # timeout: "1h"                 # OPTIONAL · cap for absolute wait (until: only)
```
Temporal pause · ONE builtin · 2 modes (relative `duration:` XOR absolute `until:` · exactly-one-of validated at parse time).

Replaces · legacy `nika:sleep` (relative-only) + `nika:wait_until` (absolute-only · cut per ADR-087 Rams sweep · 2026-05-27). Same family · same trust class (PURE) · same temporal-control semantics · the unified surface preserves both behaviors via mode arg.

Throws · `NIKA-BUILTIN-WAIT-001` on absolute timeout · `-002` if the timestamp is in the past · `-003` if neither `duration:` nor `until:` set (or both set).

---

## File builtins

### `nika:read`
```yaml
invoke: { tool: "nika:read", args: { path: "./config.yaml" } }
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
Write a file · `path:` names a FILE, never a directory · returns the path. A binary `content` value (an opaque bytes output from an upstream tool · e.g. MCP image content) is written as-is · no `output_format` declaration needed (the value carries its own type). A directory is never the target: it is made by writing its first file inside it with `create_dirs: true`, never by an empty write at the directory's path.

`content:` strings are written verbatim. Numbers, booleans, arrays and ordinary objects are written as compact JSON, with no added newline. An object containing a `bytes_base64` key is treated as binary: that value must be a valid base64 string, it is decoded to bytes, and the other keys are ignored; an invalid value is refused with `NIKA-BUILTIN-WRITE-001`. The file extension does not select a serializer: an integer value `42` writes `42`, an object `{ "sum": 42 }` writes that object, and a string already containing JSON is not encoded a second time.

The reference engine serializes object keys in sorted order, including nested objects; field order in a constructed JSON object does not promise byte order in the file. A value computed as a floating-point number retains its float form (`3.0`, `100.0`, or `0.30000000000000004`): writing adds no decimal rounding or arithmetic exactness. A string built by the program retains its own byte order and number text.

A present null `content:` is refused with `NIKA-BUILTIN-WRITE-001`; pass the string `"null"` to write that literal text. In a checked workflow, an absent required `content:` argument is refused by `check` with `NIKA-BUILTIN-001`; a reference to a missing upstream value fails before the write with `NIKA-VAR-001`. A direct builtin call without `content:` is also refused with `NIKA-BUILTIN-WRITE-001`.

`overwrite:` defaults **true** · `create_dirs:` defaults **false** — and a false `create_dirs:` is ENFORCED, not ignored: a missing parent directory is refused loudly (a typo'd path surfaces rather than silently materializing a directory tree), so pass `create_dirs: true` to create the parent. **One carve-out, by decision (engine #433 option C): a declared `permits.fs.write` entry covering the target path IS the intent** — the boundary creates the parent tree before the builtin's gate, and the write lands without `create_dirs:` (measured on the reference engine: a literal grant `deep/dir/new.txt` and a glob grant `out/**` both materialize their tree · run green). The refusal keeps its teeth where no declared grant covers the path — and there the boundary itself speaks first: an uncovered write is `NIKA-SEC-004` before the parent gate can. A typo'd path under a *narrow* grant still surfaces (it falls outside the grant → SEC-004); the tree-scatter the enforcement exists to catch cannot hide inside a boundary the author drew. Throws · `NIKA-BUILTIN-WRITE-001` (IO failure, or a missing parent while `create_dirs: false` outside a covering grant · a directory-shaped `path:` — a trailing `/` is `write failed: path not found`, an existing directory is `filesystem I/O error: Is a directory` · a file in the way of `create_dirs:` — `create_dirs failed: path already exists`, the mark an earlier empty write at the directory's path leaves behind, which `nika:write` never removes; all three measured on the reference engine 0.118.7) · `-002` (`overwrite: false` and the path exists). Both `tool_error` · `transient: false`.

### `nika:edit`
```yaml
invoke: { tool: "nika:edit", args: { path: "./file.md", find: "old", replace: "new" } }
```
In-place find/replace · returns the modified path. `find:` is a **literal
string** (not a regex: use `nika:grep` to locate · jq to transform) ·
replaces **all occurrences** · `count:` (when set) caps replacements to the
first N. `count:` MUST be a non-negative integer — a present non-integer
(e.g. the string `"2"`) is a loud `-001` arg error, NEVER silently the
replace-all default (that silent fall-through would over-edit the file · the
intent-inversion footgun class). Throws ·
`NIKA-BUILTIN-EDIT-001` (`find:` matched nothing: an edit that edits
nothing is an authoring bug · `tool_error` · also a non-integer `count:`,
and an empty `find:`, refused before the file is read: it would match
between every character and write the replacement between every byte) ·
`-002` (IO failure).

### `nika:remove_file` · exact regular-file removal
```yaml
invoke: { tool: "nika:remove_file", args: { path: "./out/scratch.txt" } }
```
Remove ONE existing regular file named by `path:` · returns the requested
path string. The arguments are exactly `{ path: string }`: there is no
recursive, glob, force, missing-ok or destination option, no alias and no
`nika:delete`.

**Authority.** Removal is an external filesystem **write** effect. It needs
the callable's tool grant (`permits.tools`) AND a `permits.fs.write` bound
containing the exact resolved path. It needs no read grant: no content is
read, and no parent directory is created. A preceding copy (`nika:read` →
`nika:write`) needs its own independent read and write grants. Removal joins
the ordinary authority, taint, consent and composition laws exactly like
`nika:write`, and the same-path mutation law (`NIKA-SEC-012`): ordered
mutations of one path are permitted; an incomparable write/remove or
remove/remove pair on one path, or a `for_each` fan onto one constant path,
is refused. There is no global one-writer rule.

**Path shape** (judged on the RAW spelling, before any normalization). The
components are delimited by `/` and the target platform's main separator.
The path MUST be a non-empty string; it is not trimmed (a single-space file
name is a name). A trailing separator, a final `.` or `..` component, or a
root or platform prefix with no file name names a directory and is refused —
`out/.` is refused even though a pure path API normalizes the final dot
away. On POSIX a backslash is an ordinary file-name character, never
silently turned into a separator. `out/./note`, `out/part/../note`,
`out/...` and ` ` pass the shape rule; the boundary judges them next.
Wildcard-looking characters (`*` · `?` · `[`) are literal file-name
characters: one path names one target and is never expanded.

**Execution.** The engine validates, without following links, that an
existing **regular** entry sits at the resolved path. A missing entry, a
directory, a symlink (dangling or not), a FIFO or any other special entry
fails; there is no recursive removal, no content-opening probe and no
link-following metadata fallback. Confinement to the granted boundary is
descriptor-relative or carries an equivalent guarantee; a backend that cannot
provide it refuses. Success reports the requested path only after the backend
reports the removal. That returned path, or a permit allow, is not
independent proof of disk state. No atomic inode comparison is promised: the
leaf may change between validation and unlink, but a substituted symlink
never redirects the removal to its target.

**Not a transaction.** Copy-then-remove is two effects. A failed publication
prevents a dependent removal; a removal that fails after a successful
publication leaves a visible copy. There is no rollback, inode-preserving
rename, whole-batch atomicity or automatic cleanup, and a cancellation
without a settled backend result does not prove that nothing was removed.

| Condition | Code | Category · transient |
|---|---|---|
| invalid argument object, keys or types, or an invalid **literal** path shape | `NIKA-BUILTIN-001` | `validation_error` · false |
| invalid resolved args or path shape | `NIKA-BUILTIN-REMOVE_FILE-001` | `validation_error` · false |
| the permitted operation fails: absent or nonregular entry, unsupported backend, ordinary OS error | `NIKA-BUILTIN-REMOVE_FILE-002` | `tool_error` · false |
| tool grant or write boundary escaped | `NIKA-SEC-004` | `security_error` · false |
| the whole `permits:` block absent | `NIKA-AUTH-006` | `security_error` · false |
| a statically resolved untrusted value escapes | `NIKA-AUTH-008` | `security_error` · false |
| unordered shared mutation or constant-target fan | `NIKA-SEC-012` | `security_error` · false |

The static path-shape judgment covers literal paths only. The shape of a
templated path string is deferred whole to the judgment of its resolved
value, even when a trailing `/` or a final `/.` is visible in the template
(`REMOVE_FILE-001` then refuses it); deferral never means the path is safe
or runnable. A missing or non-string `path`, a non-object `args` and any
extra argument key stay static refusals, even beside a templated path.

An authority refusal is never reported as `REMOVE_FILE-002`.

### `nika:glob`
```yaml
invoke: { tool: "nika:glob", args: { pattern: "./src/**/*.rs", exclude: ["**/target/**"] } }
```
Glob match · returns array of paths · **sorted lexicographically**
(deterministic across engines · filesystems). A markdown glob
(`held/*.md`) also matches `README.md` in the same directory — pin
it (`exclude: "**/README.md"`) before a paid infer classifies the
table of contents (hint `glob-readme`). Throws ·
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

The read grants cover both the directory being walked and the files read
inside it. A literal directory grant does not grant its children. For the
recursive example above, declare the root and the descendant glob:

```yaml
permits:
  tools: ["nika:grep"]
  fs: { read: ["./src", "./src/**"] }
```

For one level of files, `./src/*` covers those files while `./src` still
covers the walk root. The root grant alone does not authorize their
contents; a file read outside the declared boundary is a permission
refusal, not evidence that the pattern has no matches.

---

## Data builtins (8) · `jq` is THE data language

### `nika:jq` · the transform + extraction primitive
```yaml
with: { order: "${{ tasks.X.output }}" }
invoke: { tool: "nika:jq", args: { expression: ".items | map(.price) | add", input: "${{ with.order }}" } }
```
Run a jq expression. **The single data-transform-and-extraction language**: map · filter · select · group_by · reshape · string-interpolation `"\(.x)"` · `@base64`/`@base64d` encoders (the embedded jaq has NO `@csv`/`@tsv` — use `nika:convert to: csv`) · array `flatten` · `leaf_paths`/`getpath`/`setpath`. The same jq used in `extract:` bindings (see `04-variables.md`). **jq is also the law after an extract**: the model emits facts (`infer.schema:` as `type: integer` for numeric enums); this builtin (or `nika:decide`) names the level. Do not pay a second `infer:` to score. Teaching shape · `examples/13-extract-then-law.nika`. After `. as $c`, write `($c | map(...))` — a bare `map(` maps the current value (hint `jq-as-map`).

**`input` is any JSON value**: a single bound value (`input: "${{ with.order }}"`) OR a **constructed array for multi-input ops**. Recursive merge of two objects (this is exactly why `json_merge` is NOT a builtin · jaq's `*` does it) ·
```yaml
with: { base: "${{ tasks.base.output }}", overlay: "${{ tasks.overlay.output }}" }
invoke:
  tool: nika:jq
  args:
    input: ["${{ with.base }}", "${{ with.overlay }}"]
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
Validate data against a **JSON Schema** · returns `{ valid: bool, errors: [...] }`. Invalid DATA is a **report, never a task failure** (gate on `.valid` downstream · or `nika:assert` it). Merges the former `json_verify` + `yaml_validate` (`format:` arg · one validator). `schema:` may be an object **or** a JSON/YAML string (a `nika:read` of a `.json` file is a string — the builtin parses it). Throws · `NIKA-BUILTIN-VALIDATE-001` (the `schema:` itself is not a valid JSON Schema · `validation_error`) · `-002` (`format: yaml` and the string does not parse as YAML).

### `nika:json_merge_patch`
```yaml
invoke: { tool: "nika:json_merge_patch", args: { target: { ... }, patch: { ... } } }
```
**RFC 7396** merge patch (`null` deletes a key) · the delete-on-null semantics jq's `*` recursive-merge does NOT provide (so this stays a genuine builtin). Plain recursive merge (no delete) is just `jq '.[0] * .[1]'` on a `[base, overlay]` input · no builtin needed. Throws · `NIKA-BUILTIN-JSON_MERGE_PATCH-001` (non-object target/patch · `validation_error`).

### `nika:convert` · universal multi-format conversion
```yaml
with: { data: "${{ tasks.A.output }}" }
invoke:
  tool: "nika:convert"
  args:
    input: "${{ with.data }}"          # the data to convert · string for text formats · structured for json/yaml/toml
    from: csv                          # REQUIRED · enum · json | yaml | toml | csv
    to: json                           # REQUIRED · enum · json | yaml | toml | csv
    has_header: true                   # OPTIONAL · CSV only · default true
    columns: [id, status, amount]      # OPTIONAL · CSV emit only · explicit header order
    formula_guard: false               # OPTIONAL · CSV emit only · default false (see below)
```
Universal format converter · 4 formats v0.1 (`json` · `yaml` · `toml` · `csv`) · 12 directions in scope (4×3 minus identity) · `from == to` is rejected (`NIKA-BUILTIN-CONVERT-001` · `validation_error` · an identity conversion is an authoring bug). Throws · `-002` (the input does not parse as `from:` · `tool_error`).

**CSV cells (normative)** · reading CSV yields every cell as a **string** (`"10"`, never `10`): convert before arithmetic or ordering, e.g. `map(.amount | tonumber) | add` in `nika:jq`, since `add` over strings concatenates and `max_by` over strings orders text. The field separator is counted on the first line outside double quotes: a semicolon (a European spreadsheet export, whose decimals take the comma) or a tab that outnumbers the comma is the separator, otherwise the comma (RFC 4180). Emitting CSV writes a JSON `null` as an empty cell, as an absent key already is, never the text `null`.

**`columns`** (CSV emit only) · an array of column names in the required order. JSON objects carry no key order: without `columns`, output keys are sorted alphabetically. Named columns come first (even for empty rows); any other keys follow sorted. To preserve a CSV header through filtering, pass its observed columns in their original order to the JSON→CSV conversion. Duplicate names retain their first position; a named column absent from every row is emitted with empty cells. A non-array or non-string member is a `NIKA-BUILTIN-CONVERT-001` error, never a silent fallback to the default order. `formula_guard` still applies to the emitted headers and cells.

**`formula_guard`** (CSV emit only · default `false`) · opt-in **CSV formula-injection guard** (CWE-1236). A spreadsheet (Excel · Sheets · LibreOffice) interprets a cell whose FIRST non-whitespace character is `=` `+` `-` `@` (or a leading `\t`/`\r` control char) as a **formula** — so `=HYPERLINK(…)` or `=cmd|…` in untrusted data executes when the file is opened. With `formula_guard: true`, such a cell (data OR header key) is prefixed with a single quote `'` — the OWASP mitigation those apps render as literal text. **Opt-in because it ALTERS data**: a legitimate negative number `-5` becomes the text `'-5`. Enable it when the CSV carries untrusted data AND is destined for a spreadsheet; leave it off (the default) for clean machine round-trips — matching the Rust/Python `csv` ecosystem, where the spreadsheet is the consumer's trust boundary. A non-boolean value is a loud `-001` arg error (never silently read as `false`).

Pattern · `fetch+extract` symmetry · single super-powerful builtin · `from`/`to` mode parameters · all bidirectional pairs canonical · no per-direction builtin slot.

Replaces · legacy `nika:csv_to_json` (cut per ADR-086 · D-2026-05-27 Rams sweep · the « less but better » builtin-by-builtin review that cut the canonical set to 22). The reverse direction (JSON→CSV) is ALSO covered here · `nika:convert` is the canonical multi-format builtin for BOTH directions (the embedded jaq has no `@csv`/`@tsv` filter, so there is no in-jq alternative — `nika:convert to: csv` is the one path).

Reference implementation · `serde_transcode` 1.1+ orchestrator (zero-allocation walk · serde-ecosystem canonical · 15M+ downloads · sfackler) + format-specific crates · `serde_json` (JSON · already nika dep) · `serde_yaml_bw` 2.5+ (YAML · modern + maintained 2026) · `toml` 1.1+ (TOML · spec 1.1.0 compliant) · `csv` 1.4+ (CSV · quoting-aware).

### `nika:uuid`
```yaml
invoke: { tool: "nika:uuid", args: { version: v7 } }   # v7 default (timestamped/sortable · RFC 9562) | v4 (random)
```
Generate a UUID. (Generators are not jq · jq is pure transform.) Returns the canonical lowercase-hyphenated string. No failure codes.

### `nika:decide`
```yaml
with: { snapshot: "${{ tasks.collect.output }}" }
invoke:
  tool: "nika:decide"
  args:
    bundle: "./decisions/pr-triage.bundle.json"   # path OR inline bundle object
    evidence: "${{ with.snapshot }}"              # the EvidenceSnapshot { t, evidence: [...] }
```
The deterministic decision kernel (spec [11-decision.md](../spec/11-decision.md)) · evaluates a portable **Decision Bundle** against an **EvidenceSnapshot** and returns the full Decision Receipt (outcome ∈ {recommend · defer · human_required · opted_out · overridden} · per-dimension term-by-term contributions · intervals · conflicts+witnesses · determination provenance). PURE compute — zero required effects (a `bundle:` path reads like any declared `fs.read`; an inline object needs no filesystem) · zero floats (fixed-point basis-points) · same inputs, same bytes, both evaluators (the stdlib-Python reference interpreter is the conformance oracle — byte-equal canonical JSON). **The LLM never decides**: `infer:` produces closed cited facts; this kernel applies the rubric. The lighter teaching shape (no bundle) is `nika:jq` after the extract — `examples/13-extract-then-law.nika`. Throws · `NIKA-DECIDE-001` (the bundle violates its own laws) · `NIKA-DECIDE-002` (the snapshot does not satisfy the evidence schema) · both `validation_error`, deterministic.

### `nika:date`
```yaml
invoke: { tool: "nika:date", args: { op: now } }
# op: now { tz } | add { base, duration } | subtract | format { input, format } | parse | diff { start, end, unit }
```
Timestamp arithmetic · op-discriminated single builtin · timezone-aware (IANA · default UTC) · ISO 8601 out. `format:`/`parse` use the **strftime** field grammar (`%Y-%m-%d` · the one cross-language constant). Every op returns a string EXCEPT `diff` (integer · in `unit:`). Throws · `NIKA-BUILTIN-DATE-001` (unparseable input / unknown op / bad tz · `validation_error`).

### `nika:hash`

> **Reference-engine availability at `6ac427c5e62dc450d1d2393e3eec169e9566f6a3`:**
> digest calls accept `content`, `algo` and `encoding`. The `op` argument,
> including explicit `op: hash`, and the `sign`/`verify` forms below are
> **spec-ahead of this engine pin**. Their normative contract is retained;
> these examples do not announce implementation availability.

```yaml
with: { report: "${{ tasks.X.output }}" }
invoke: { tool: "nika:hash", args: { algo: blake3, content: "${{ with.report }}", encoding: hex } }

# Normative op forms · spec-ahead at the engine pin named above
invoke: { tool: "nika:hash", args: { op: sign,   content: "${{ with.manifest }}", key: "${{ secrets.release_key }}" } }
invoke: { tool: "nika:hash", args: { op: verify, content: "${{ with.manifest }}", public_key: "${{ const.release_pub }}", signature: "${{ with.sig }}" } }
```
Content hashing · default **blake3** (fastest modern cryptographic hash · parallel · secure) · or `sha256`/`sha512`. md5/sha1 NOT supported (cryptographically broken · `NIKA-BUILTIN-HASH-001` `validation_error` on an unsupported algo). `encoding:` `hex` (default) | `base64`. Use cases · cache keys · content addressing · provenance. `content:` accepts a string **or** a structured value (an object/array is hashed as compact JSON — do not pre-pipe `| tojson`).

**Digest arguments (`op` omitted or `op: hash`).** `content` MUST be present and non-null. Strings are hashed verbatim; every other non-null JSON value (object, array, number or boolean) is hashed as compact JSON. An empty string, `0`, `false`, an empty object or array, and null members inside an object or array are valid content. `algo` and `encoding` use their defaults only when absent. When present, each MUST be a string in its listed closed set; null, another JSON type, an empty string or an unsupported value is a validation error (`NIKA-BUILTIN-HASH-001`). Statically known invalid literals MUST be refused before execution. A valid substitution expression whose result is not yet known is checked after resolution; it does not exempt other, independently decidable arguments from validation.

**`op:` · `hash` (default) · `sign` · `verify` (normative · v0.1 surface).**
The engine already signs and verifies Ed25519 — the run seal is one Ed25519
signature over the journal, minted by `nika key init` and checked by the
evidence verifier. **The language could not reach any of it.** A capability
that ships and stays unreachable from the file is not a shipped capability,
so `op:` exposes it through the builtin that already owns the digest.

| Arg | Ops | Notes |
|---|---|---|
| `content` | all | required · the bytes under the operation |
| `algo` | `hash` | `blake3` (default) · `sha256` · `sha512` |
| `encoding` | `hash` · `sign` | `hex` (default) · `base64` — the output encoding |
| `key` | `sign` | required · the Ed25519 private key · **MUST be a `${{ secrets.X }}` reference** |
| `public_key` | `verify` | required · the Ed25519 public key (a `const:`/`inputs:` value — a public key is not a secret) |
| `signature` | `verify` | required · the signature to check, in `encoding:` |

`sign` returns the signature string; `verify` returns a boolean and never
throws on a bad signature (a *false* verdict is data, not an error — a
workflow branches on it with `when:`). A malformed key, an unknown `op:`, or
a missing per-op required arg is `NIKA-BUILTIN-HASH-001` (`validation_error`).

**The key rides `secrets:` — that is a hard rule, not a convention.** A
private key reached from the ambient environment or an implicit on-disk
keyring would make `nika:hash` read state its declaration does not name,
which is exactly the pure-compute-that-is-not-pure defect the permits
boundary exists to refuse. Declaring the key under `secrets:` keeps it inside
the three-authority family, keeps it masked in logs, and keeps its reach
governed by `egress:` like any other secret.

**What this closes, and what it does NOT (bounded honestly).** It closes
**Ed25519** signing and verification — detached signatures over content the
workflow already holds: release manifests, provenance attestations,
inter-workflow handoffs, checkpoint verification. It does **not** close
RS256/JWT: that is RSA with a different key format, a different padding
scheme and a token envelope, none of which this surface has. A GitHub App
credential exchange is RS256 and stays out of reach — do not read this
section as authenticating anything that mints a JWT. `capability_classification`
for the builtin moves from `pure_internal` to secret-reading under `op: sign`;
the classification row in `canon/builtins.yaml` follows when the engine wires
the op (today the engine's `ARGS` gate declares `content · algo · encoding`
only, so `op:` is spec-ahead-of-engine and this file says so rather than
implying a green that does not exist).

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
| `headers` · `body` | extra headers · body (objects auto-JSON: a non-string `body` is sent as JSON with `content-type: application/json` unless `headers` names a content type · a string body is sent as written) · auth rides `headers` (`x-api-key: "${{ secrets.KEY }}"` · masked) |
| `form` | `application/x-www-form-urlencoded` object of scalar fields (string · number · boolean — nesting refused · reshape with `nika:jq`) · body-bearing method required |
| `multipart` | `multipart/form-data` parts array · `{name, value}` text XOR `{name, path, filename?, content_type?}` file · every `path` rides the `permits.fs` READ boundary (`NIKA-SEC-004` on escape) · ≤64 parts · ≤32 MiB total · body-bearing method required |
| `mode` | extraction mode · see `extract-modes-v0.1.md` · default `markdown` (a body served as text, such as CSV or JSON, comes back as served · [extract-modes §a body served as text](./extract-modes-v0.1.md#the-canonical-modes)) |
| `jq` | a jq expression · only with `mode: jq` (structured JSON extraction · replaces the former JSONPath mode) |
| `traverse` | bounded same-origin crawl · `{ max_pages: 1..=25 (required), respect_robots?: bool (default true) }` · GET only · excludes `mode`/`selector`/`jq`/`body`/`form`/`multipart`/`headers`/`response` |
| `response` | optional closed object `{ accept: [200, 404] }` · `accept` is required: 1..=16 distinct integer final HTTP statuses in 200..=599 · no ranges, wildcards, strings, booleans or floats · excludes `traverse` |

**Payload exclusivity (normative)** · at most ONE of `body` · `form` ·
`multipart`; `form`/`multipart` require a body-bearing method (`POST` ·
`PUT` · `PATCH`) and OWN their `content-type` (a user-supplied
`content-type` header alongside them is a refused conflict).

**The data-as-code sink (normative pointer · NEP-0006)** · a fetch whose
resolved URL path names a code-bearing class (serialized-executable ·
script/interpreter · executable binary/module · the closed list in
`10-authority.md` §the data-as-code sink) is refused at check
(`NIKA-SEC-008`) and at run (`NIKA-SEC-004`) unless the task declares the
read inert (`lift: [{law: data-as-code, because: "…"}]` · the honest
door · it never lifts the host boundary or the SSRF floor).

**`traverse` semantics (normative)** · same-origin BFS from `url` ·
fragment-stripped dedup · per-page output is the fixed page digest
`{url, status, title, description, headings ≤16, links ≤30, images ≤24,
colors ≤20, text ≤4000}` · crawl output is `{url, page_count, pages[],
assets: {images ≤40, colors ≤30}}` · `robots.txt` honored by default
(RFC 9309 group semantics · `User-agent: *` `Disallow` prefixes · the
probe's outcome follows RFC 9309 §2.3.1: a 4xx is *unavailable* and
allow-all · a 5xx or a transport failure is *unreachable* and a COMPLETE
DISALLOW, refused loudly and transiently before a page is spent · rules
are per origin, so a root that lands on another origin re-reads that
origin's robots before any descendant is enqueued · a disallowed ROOT is
a loud failure · disallowed descendants are silently skipped) · the page
digest passes the same depth admission every HTML mode passes (a
depth-bomb root is a loud failure · a depth-bomb descendant is an honest
`{url, status, error}` entry) · discovery reads every link a page
carries (the `links ≤30` facet is a preview, never the frontier's bound)
· every hop rides
the engine's SSRF defense · a failing descendant page becomes an honest
`{url, status|error}` entry and the crawl continues · the effect
certificate bounds logical GET calls per invocation: `max_pages` with
literal `respect_robots: false`, otherwise `max_pages + 2` (the seed's
robots, then at most one probe of the landed root's different origin).
An unchanged origin needs only one robots probe, but a literal initial
URL and the same-origin link filter do not exclude a root redirect;
an unknown final origin therefore requires the two-probe bound. Dynamic
page bounds use the valid runtime maximum 25, hence at most 27 logical
GETs (25 with literal robots false); dynamic robots options reserve both
probes. Invalid resolved options still refuse before any GET. These are
logical calls, not a bound on physical transmissions, redirect hops or
transport retries; aggregate saturation is not a proven global bound.
`crawl`/`http`/`website` are intentions, not
tools — they all route HERE (no `nika:crawl`).

**Non-2xx is failure by default (normative)** · without `response`, a non-2xx response throws
`NIKA-BUILTIN-FETCH-001` (`category: network_error` · `transient: true` for
5xx/408/429 · `false` for other 4xx · `details.status_code` carries the
status) — **with the effect-safe carve-out (normative · issue #1371)**: a
keyless effect-capable method (`POST` · `PUT` · `DELETE` · `PATCH` without
an `idempotency-key` header) is `transient: false` on EVERY failure,
status-table and transport alike — the failure may be ambiguous (the
server may have committed before the socket dropped or the error was
emitted) and a blind retry replays the effect; a declared `retry:` on such
a call is the static `NIKA-SEC-016` refusal ([05 §the effect-safe retry
law](../spec/05-errors.md#the-effect-safe-retry-law-normative--issue-1371)).
`GET`/`HEAD` and keyed calls keep the status table.
To poll a pending resource · the jq-error pattern
([08 H19](../spec/08-out-of-scope.md)), not status-code inspection.
Redirects follow up to an engine cap · the FINAL status decides.

**Status as data (normative)** · a present `response` opts into an exact
accepted-status set. Every listed status produces the same output shape,
including a 2xx only when explicitly listed:

```json
{"status_code":404,"url":"https://example.com/missing","body":"not found"}
```

`body` is the result of the existing `mode` extraction, with its existing
failure rules. `url` identifies the final response route after redirects,
serialized as an HTTP(S) URL without userinfo, query or fragment; it is
`null` when that identity is unavailable or invalid. An engine MUST NOT
substitute the requested URL for an unknown final destination. Response
headers are not emitted. The presence of `response`, never the status
itself, determines this output shape. Without `response`, the extracted
body and existing diagnostics are unchanged.

An unlisted status remains `NIKA-BUILTIN-FETCH-001`, with the existing
effect-safe transient classification and `details.status_code`; an opted-in
call additionally reports `details.accepted`. Transport, permission, SSRF,
TLS, redirect-limit, body-limit and extraction failures do not become
successful observations. An extraction failure after a received response
also carries its known `details.status_code` and `details.accepted`; a
transport failure MUST NOT fabricate a status. A missing final URL alone
does not discard a known status or body.

An accepted status is a successful observation, never a skipped or
recovered failure, and does not trigger `retry:`. Successful sibling
outputs therefore remain durable in the absence of another unhandled
failure. An unaccepted status or other unhandled failure retains the
engine's existing failure and output-quarantine behavior. Declaring a
keyless effect-capable call's error status as data does not prove whether
the server committed its effect, and does not relax the effect-safe retry
law. Followed redirect statuses are intermediate responses; only the final
status is compared with `accept`.

Literal malformed policies refuse at check. Resolved policies, including
agent tool calls, are revalidated before any HTTP request. Dynamic values
are unknown at check, never permission: invalid literal siblings still
refuse. Unknown keys, a present null, empty or oversized sets, duplicates,
non-integers and out-of-range statuses are errors, never silently coerced,
deduplicated or ignored.

For a health probe, use `mode: raw` and `response: { accept: [200, 404] }`.
A downstream task binds the observed status through `with` before its `when`
condition reads it:

```yaml
with:
  observed_status: ${{ tasks.probe.output.status_code }}
when: ${{ with.observed_status == 404 }}
```

Its 404 follows the success edge; a failure edge is reserved for actual failures.
JSON extraction of an HTML error body still fails, and a HEAD or 204
response with an empty body remains an empty string under `mode: raw`.

**Security (engine MUST)** · SSRF defense (reject private-net + cloud-metadata `169.254.169.254` unless configured) · honor task-level `timeout` · reject self-signed TLS by default.

### `nika:notify`
```yaml
with: { status: "${{ tasks.X.status }}", run: "${{ tasks.X.output }}" }
invoke: { tool: "nika:notify", args: { channel: webhook, target: "https://hooks.slack.com/...", message: "Done · ${{ with.status }}", severity: info, data: { run: "${{ with.run }}" } } }
```
Send notifications · `channel:` enum (`webhook`/`slack`/`email`/`discord`/`sms` · one builtin not 5). The 1.0 engine MUST support `webhook` · others MAY be feature-gated. `data:` (OPTIONAL · any JSON value) carries structured context alongside the human `message`: the webhook payload is `{ message, severity, data? }` (the key is absent when not given · receivers branch on machine fields, never parse the message). Returns `null` on accepted delivery. Throws · `NIKA-BUILTIN-NOTIFY-001` (channel unconfigured · `validation_error`) · `-002` (delivery failed · `network_error` · transient engine-assessed).

**Chat webhooks (normative · `webhook`)** · a chat service's incoming webhook reads its own text field and refuses the payload above (a Slack incoming webhook answers HTTP 400 `no_text`). The engine MUST shape the `webhook` body for the receivers it recognizes from `target:` and MUST keep `{ message, severity, data? }` for every other target:

| `target:` | Body |
|---|---|
| a Slack incoming webhook (host `hooks.slack.com` or `hooks.slack-gov.com`, path `/services/…`) | `{ text, mrkdwn: false }` |
| a Discord webhook (host `discord.com`, `discordapp.com` or their `canary.`/`ptb.` subdomains, path `/api/webhooks/{id}/{token}` or `/api/v{N}/webhooks/{id}/{token}`) | `{ content, allowed_mentions: { parse: [] } }` |
| any other target, including a Slack Workflow Builder trigger (`/triggers/…`) and Discord's `/slack` or `/github` adapters | `{ message, severity, data? }` |

The text a person reads is the `message`, prefixed `[<severity>] ` when `severity` is not `info`, then the `data:` context on its own line (a string as written, any other value as compact JSON). For Slack, `&` `<` `>` become `&amp;` `&lt;` `&gt;`. For Discord, every character that opens markdown, a masked link or a mention (`\` `*` `_` `~` `` ` `` `|` `[` `]` `<` `>`, and a line's opening `#` or `-`) is backslash-escaped outside a bare `http://`/`https://` URL, and `content` is bounded to 2,000 UTF-16 units, ending `…` when cut, without splitting an escape. A refused delivery (`-002`) names the receiver's answer as one bounded line, with control and invisible characters removed and every part of `target:` withheld (a webhook URL carries its credential in its path or query), and carries `details.status_code`.

**Injection defense (normative)** · the `webhook` payload is JSON-encoded, so the `message` cannot break its transport (serde escaping neutralizes `"`/newlines). A channel that renders the `message` into a DIFFERENT medium MUST neutralize that medium's injection surface when implemented · **email** MUST reject/strip CR/LF in any header-bound field (`target:`/subject) against RFC 5322 header injection · **slack**/**discord** MUST treat the `message` as text, never a trusted mrkdwn/mention control string · **sms** MUST bound length to the carrier segment limit. The `webhook target:` rides the same SSRF boundary as `nika:fetch` (private-network / cloud-metadata rejected). Same discipline as the `nika:fetch` SSRF requirement · an output channel that reaches an interpreter neutralizes that interpreter's control surface.

---

## Introspection builtins (2)

### `nika:compose` · self-check a drafted workflow (agent loops only)
```yaml
agent:
  prompt: "Draft a workflow, then check it before you finish."
  tools: ["nika:done", "nika:compose"]   # done first · mock/echo closes at turn one
```
The teaching shape is
[`examples/15-compose-self-check.nika`](../examples/15-compose-self-check.nika).
The agent loop's self-check intrinsic: the model passes a complete workflow
YAML draft and receives its in-memory static report as JSON. `valid` means
that parsing succeeded and Core conformance has no violations. The report
also carries counts of secret-flow findings and capability escapes, plus a
bounded termination/cost certificate summary; those counts do not become an
execution-admission verdict.

The intrinsic reads no child files and does not resolve the composition
graph. A draft that names a missing child can therefore have `valid: true`
here and still fail the file-aware `nika check`. Save the draft and check the
actual file and its children before presenting it as ready to run.

It **never executes** the draft or grants its effects. Even a valid draft
that declares filesystem writes performs no write during this self-check.
Deliver the draft with this limited report; running it remains a separate,
gated decision (« generation is not permission »).

Loop-served like `nika:done`: **valid only inside an `agent:` tool whitelist**
(a standalone `invoke: { tool: "nika:compose" }` is rejected · `NIKA-BUILTIN-COMPOSE-001`).
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

The reference engine injects a live cell at composition and writes it
as tasks settle. `available: true` once the DAG is seeded at run start.
The teaching shape is
[`examples/16-inspect-self.nika`](../examples/16-inspect-self.nika).
Probe any new builtin with `mock/echo` *before* wiring it after a paid
infer.

---

## Media builtins (6)


### `nika:chart` · deterministic chart artifacts (§Media graduate #4)

The task binds the rows produced by `metrics` before invoking the chart.
`period` values are Unix epoch milliseconds when their semantic is `timestamp`.

```yaml
with:
  metrics: "${{ tasks.metrics.output }}"
invoke:
  tool: "nika:chart"
  args:
    data: "${{ with.metrics }}"   # rows · array of flat objects (strings + numbers) · or { path: <json file> }
    semantics: { period: timestamp, cost: usd, provider: category }
    chart:
      type: line                          # bar | line | area_band | scatter | heatmap (closed)
      x: period                           # field names · y_lo/y_hi = area_band bounds · y2 = actual overlay
      y: cost                             # color = series split (heatmap: the value field)
      title: "Cost over time"
    out: report/cost-over-time.svg        # .svg REQUIRED — SVG is the attestation surface
    compile_to: vega_lite                 # optional · also writes the .vl.json sibling
```

Pure compute + ONE permit-gated write (`out:` rides `permits.fs.write` ·
a `data.path` source rides `permits.fs.read`). No network, no clock, no
keys — identical `(spec, data)` produce **byte-identical SVG** on every
platform, re-runs are idempotent (hash-equal artifacts are never
rewritten), and `outputs:` carries the receipts: `path · sha256 ·
data_sha256 · width · height · bytes · wrote · warnings`.

The semantic map drives presentation (`usd` money grain · `duration_ms`
humanized · `tokens` k/M · `delta` ⇒ diverging palette anchored at 0 ·
`percent` · `timestamp` · `count` · `category` — closed set). Encoding
honesty is built in: bars always anchor zero · **predictions are dashed,
observations solid** (area_band) · decimation (LTTB) keeps extremes and
warns — the DATA hash still covers every row.

Errors are `NIKA-BUILTIN-CHART-001..008` (001 empty data · 002 unknown
field · 003 non-finite · 004 invalid spec/closed-enum · 005 domain
collapsed · 006 type mismatch · 007 too many points · 008 I/O).

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
| `mode` | `generate` (default · text→image) · `edit` (source image(s) + instruction → image · M2.2 · 2026-07-06) |
| `image` / `images` | mode:edit source · one path (`image:`) XOR many (`images:` · capped per provider: openai 16 · gemini 14 · xai 3) · **read + permit-gated** (`permits.fs.read` must cover them — the mirror of the save boundary) |
| `mask` | mode:edit optional pixel-mask path · openai/local only — a mask on an instruction-only provider (gemini/xai) is REFUSED loudly, never silently dropped (the output would be wrong outside the region) |
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
never zero-that-looks-real. `cost_usd` is the render's spend, and a
non-null one rides with `cost_basis`, the origin of the figure: either the
provider's own exact report (`provider_reported` · xai bills
`cost_in_usd_ticks` in the response · 1 cent = 10⁸ ticks), or the
engine's dated price catalog applied to what the provider reported:
`catalog_rate_x_usage` (the model's catalog token rates × the usage the
responses reported, a response that reported none adding its images at
the catalog price per image) or `catalog_price_x_images` (the model's
catalog price per image × the images delivered, when no response reports
usage). The price per image is the catalog's for the requested size class
and quality; where the request leaves either open (`quality: auto`, a
size the catalog does not list), the highest the catalog lists for what it
could have rendered, never below. A catalog figure also carries
`cost_as_of`, the date its rates were read from the provider's public
price page (`YYYY-MM-DD`). When neither the provider's report nor the
catalog yields a figure (a model the catalog does not price, or one it
prices by tokens only whose response reports no usage), `cost_usd` is
`null` and both markers are absent — never an estimate dressed as truth,
never a zero. The catalog itself is engine
data, not language surface (spend-honesty law ·
[07](../spec/07-conformance.md)). **Invoke-cost metering (normative)** · a
tool whose structured output carries a top-level numeric `cost_usd`
reports real spend; engines SHOULD meter it into the run's cost ledger
through the same honest-spend channel `infer:` rides.

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

**Content credentials: detect-and-PRESERVE (normative)** · upstream
generators sign their returned bytes (OpenAI images and Google's media
models emit C2PA manifests as of mid-2026 · carriage: PNG `caBX` chunk ·
JPEG APP11 JUMBF · `WebP`/WAV RIFF `C2PA` · MP3 `ID3v2` GEOB
`application/c2pa`), and C2PA hard bindings hash the asset's byte
ranges — **any engine-side insert into a signed payload converts valid
credentials into « present but tampered », which is worse than
stripping them**. Engines MUST detect these signals before any in-file
write, MUST stand their own embed down when credentials are present,
and SHOULD surface presence as `content_credentials` in output +
manifest (values are DETECTION labels — an engine that has not
cryptographically validated the manifest must never say « verified »).
Provider-declared watermarking (e.g. `SynthID` — detectable only by the
vendor) MAY be surfaced as a `watermark_declared` manifest fact, marked
as declared, never as verified. Preserving machine-readable marks is
also the EU AI Act Article 50 robustness expectation (in force
2026-08-02).

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

### `nika:image_fx` · deterministic artistic image effects

```yaml
invoke:
  tool: "nika:image_fx"
  args:
    input: out/hero-openai-gptimage2-0-a1b2c3d4.png   # source PNG (read-gated)
    out: out/hero-styled.png                          # artifact path (write-gated)
    seed: 42                                          # optional · default 0
    ops:                                              # ordered · single-key op maps
      - resize: { width: 320 }                        # linear-light · auto|nearest|bilinear
      - dither: { mode: floyd_steinberg, palette: gameboy }
      - grain: { intensity: 32 }
      - scanlines: { strength: 110, period: 4 }
      - vignette: { strength: 140 }
```

The §Media graduate #3 (the `image editing` deferred row) — the
DETERMINISTIC sibling of `image_generate`: pure pixel transform, NO
provider, NO network, NO clock. **Byte-identical output forever** for
identical `(input bytes, args)` — the artifact's sha256 joins the
hash-chained trace, re-render IS the tamper check, and the full recipe
(contract tag `image_fx/v1` · input sha256 · seed · ops) rides the artifact itself
as a PNG `nika` tEXt chunk (no timestamp — determinism holds).
Identical bytes already at `out:` = idempotent skip (`skipped_existing`).

| Arg | Contract |
|---|---|
| `input` | REQUIRED · source path · **PNG v1** (depth 8 · gray/RGB/RGBA · no Adam7) — non-PNG fails `-003` with the honest hint (produce PNG upstream via `image_generate format: png`) · read rides `permits.fs.read` · decoded-pixel budget gated from the header BEFORE any decompression (`-005`) |
| `out` | REQUIRED · artifact path · extension must match the pipeline (`.png` · `.txt`/`.ans` for ascii text/ansi emits) · write rides `permits.fs.write` |
| `ops` | REQUIRED · 1..=32 ordered single-key maps over the closed v1 vocabulary: `resize {width,height,filter}` · `crop {x,y,width,height}` · `levels {brightness,contrast}` · `grayscale` · `palette_map {palette}` · `dither {mode: bayer2\|bayer4\|bayer8\|blue_noise\|ign\|floyd_steinberg\|atkinson\|jjn, palette}` · `duotone {dark,light}` · `pixelate {block}` · `halftone {cell, angle: 0\|15\|45\|75}` · `grain {intensity}` · `vignette {strength}` · `chromatic_aberration {shift}` · `scanlines {strength,period}` · `glitch {line_shift,channel_shift,blocks}` · `ascii {cols, emit: png\|text\|ansi}` (ascii MUST be last — it changes the artifact type). Branching/fan-out is the WORKFLOW's job (`with:`/`after:` edges · `for_each`) — `ops` is a linear pipeline by design · unknown op parameters are rejected (`-001` — no silent style lies) |
| `palette` (in ops) | preset (`bw` · `gray4` · `gameboy` · `cga` · `okabe_ito`) or inline list of `#rrggbb` / `[r,g,b]` (2..=256 colors) · nearest-color mapping is perceptual (Oklab distance) |
| `seed` | stochastic-op seed (`grain` · `glitch`) · default 0 · **the seed IS the style** — same seed, same bytes |

Per-op defaults + acceptance ranges (NORMATIVE — for a byte-deterministic
builtin the defaults ARE the wire contract; a second engine choosing other
defaults emits different bytes for the same YAML) ·

| Op | Defaults | Ranges |
|---|---|---|
| `resize` | `filter: auto` | width/height 1..=16384 (≥1 required) |
| `crop` | `x: 0` · `y: 0` | width/height ≥1 · rect inside image |
| `levels` | `brightness: 0` · `contrast: 0` | brightness −255..=255 · contrast −128..=128 |
| `dither` | `mode: floyd_steinberg` · `palette: bw` | palette 2..=256 colors |
| `palette_map` | `palette: bw` | palette 2..=256 colors |
| `pixelate` | — | block 2..=256 |
| `halftone` | `cell: 8` · `angle: 45` | cell 3..=64 |
| `grain` | `intensity: 48` | 0..=128 |
| `vignette` | `strength: 160` | 0..=255 |
| `chromatic_aberration` | `shift: 4` | 1..=16 |
| `scanlines` | `strength: 96` · `period: 4` | strength 0..=255 · period 2..=64 |
| `glitch` | all 0 | line_shift ≤64 · channel_shift ≤16 · blocks ≤64 · ≥1 non-zero |
| `ascii` | `cols: 96` · `emit: png` | cols 2..=1024 |

Input caps (normative) · per-dimension ≤16384 · decoded pixels ≤2^26
(gated from the header BEFORE any decompression · `-005`).

Output · `{ input, input_sha256, path, sha256, size_bytes, width, height,
format, ops_applied, seed, skipped_existing }` — artifact bytes NEVER
ride outputs (the disk law, inherited verbatim). Text artifacts (ascii
`text`/`ansi` emits) report `width`/`height` `0` and `format` `txt`/`ans`.

Determinism contract (normative) · no wall-clock · no randomness beyond
the seeded stream · no float transcendentals · integer/fixed-point pixel
math · identical `(input, args)` MUST produce byte-identical artifacts
across platforms and releases within a stdlib major (the recipe carries
the stable contract tag `image_fx/v1`, never the engine build). An
engine that cannot honor this is not conformant (07-conformance §3).

Throws · `NIKA-BUILTIN-IMAGE_FX-001` invalid arguments (unknown op ·
out-of-range param · extension mismatch · `validation_error`) · `-002`
input read failed (`tool_error`) · `-003` unsupported input format
(non-PNG · exotic depth/interlace · `validation_error`) · `-004` decode
failed (structural corruption · CRC/Adler mismatch · `tool_error`) ·
`-005` decoded-pixel budget exceeded (`security_error` · never
transient) · `-006` artifact save failed (`tool_error`). Plus the
boundary `NIKA-SEC-004` (paths outside `permits.fs`).

---

### `nika:image_compose` · deterministic image composition (§Media graduate #5)

```yaml
invoke:
  tool: "nika:image_compose"
  args:
    inputs:                              # ordered · 1..=64 PNG paths (each read-gated)
      - out/fx/sunset-duotone.png
      - out/fx/sunset-gameboy.png
      - out/fx/harbour-duotone.png
      - out/fx/harbour-gameboy.png
    out: out/contact-sheet.png           # the artifact · .png (write-gated)
    layout: grid                         # grid (default) | row | column
    scale: fit                           # none (default) | fit | fill
    cell: { width: 256, height: 256 }    # fit · fill only · default: the largest input box
    gap: 6
    padding: 12
    background: "#10141c"
```

The §Media graduate #5 — several PNG images become ONE: a contact sheet
or grid, a side-by-side (`row`), a vertical stack (`column`). Like
`image_fx` it is pure pixel work: NO provider, NO network, NO clock, NO
keys, and **byte-identical output** for identical `(input bytes, args)` —
the artifact's sha256 joins the hash-chained trace, re-render IS the
tamper check, and the recipe (contract tag `image_compose/v1` · each
input's sha256 and size · the resolved layout) rides the artifact as a
PNG `nika` tEXt chunk. The recipe carries no timestamp and no path: the
same bytes composed from another folder are the same file. Identical
bytes already at `out:` = idempotent skip (`skipped_existing`).

| Arg | Contract |
|---|---|
| `inputs` | REQUIRED · an ordered list of 1..=64 source paths · each **PNG v1** (the `image_fx` input law: depth 8 · gray, gray+alpha, RGB or RGBA · no Adam7 — anything else `-003`) · each read rides `permits.fs.read` · placed row-major, `inputs[0]` first |
| `out` | REQUIRED · the artifact path · `.png` · the write rides `permits.fs.write` |
| `layout` | `grid` (default) · `row` (one row, left to right) · `column` (one column, top to bottom) |
| `columns` · `rows` | `grid` only (`-001` with `row` / `column`) · 1..=64 each · both absent: `columns` = ⌈√n⌉ and `rows` = ⌈n ÷ columns⌉ · one given: the other is ⌈n ÷ given⌉ · both given: `columns × rows` ≥ n (`-001` otherwise · trailing cells stay background) |
| `scale` | `none` (default · each image at its own size) · `fit` (scaled up or down, aspect kept, to the largest box inside its cell · letterboxed on `background`) · `fill` (scaled, aspect kept, to cover its cell, then center-cropped to it) |
| `cell` | `{ width, height }` · 1..=16384 each · `fit` / `fill` only (`-001` with `scale: none`) · default: the largest input box (the widest width × the tallest height) |
| `gap` · `padding` | pixels between cells · around the sheet · 0..=1024 each · default 0 |
| `background` | `#rrggbb` or `[r, g, b]` · default `#ffffff` · fills the canvas, the gaps, the letterboxes and the empty trailing cells |

Geometry (NORMATIVE — for a byte-deterministic builtin the arithmetic IS
the wire contract; `round(a ÷ b)` is `⌊(a + ⌊b ÷ 2⌋) ÷ b⌋`, never below 1) ·

- the placed size of a `w × h` input in a `cw × ch` cell: `none` → `w × h` ·
  `fit` → `round(w·ch ÷ h) × ch` when `w·ch ≤ h·cw`, else `cw × round(h·cw ÷ w)` ·
  `fill` → the cover box (`round(w·ch ÷ h) × ch` when `w·ch ≥ h·cw`, else
  `cw × round(h·cw ÷ w)`), cropped to `cw × ch` at `(⌊(sw − cw) ÷ 2⌋, ⌊(sh − ch) ÷ 2⌋)`;
- scaling is the `image_fx` `resize` op with `filter: auto` (linear light)
  to the placed box — the same bytes that op produces;
- tracks: with `fit` / `fill` every column is `cw` wide and every row `ch`
  high; with `none` a column is as wide as its widest image and a row as
  high as its tallest;
- each image is centered in its cell at `(⌊(track_w − pw) ÷ 2⌋, ⌊(track_h − ph) ÷ 2⌋)`;
- the canvas is `2·padding + Σ column widths + gap·(columns − 1)` by
  `2·padding + Σ row heights + gap·(rows − 1)`;
- an input's alpha is composited over `background` BEFORE any scaling,
  per channel `⌊(c·a + bg·(255 − a) + 127) ÷ 255⌋` in sRGB integers · the
  artifact is RGB8 (PNG color type 2).

Caps (normative · `-005`) · each input ≤ 16384 per side · the SUM of the
inputs' pixels ≤ 2^26 · each `fit` / `fill` scaled box (the placed size
above, before the `fill` crop) ≤ 16384 per side and ≤ 2^26 pixels · the
canvas ≤ 16384 per side and ≤ 2^26 pixels — all judged from the PNG
headers and the args BEFORE any decompression or canvas allocation. An
input carries exactly one IHDR, its first chunk (PNG-3): the size the caps
judged is the size decoded, and a second IHDR is `-004`. The input count
(1..=64) is an argument, `-001`.

Output · `{ path, sha256, size_bytes, width, height, format, layout,
columns, rows, scale, inputs: [{ path, sha256, width, height }],
skipped_existing }` — artifact bytes NEVER ride outputs (the disk law).

**Authority.** The tool grant, `permits.fs.read` for EVERY input and
`permits.fs.write` for `out:`. A static checker judges each literal input
path (or whole bare `const:` reference) as a read and a literal `out:` as a
write, before anything runs: a missing read grant is a check-time
`NIKA-SEC-004`, never a surprise at run, and a read grant an input uses is
a use, never `declared-but-unused` advice. A templated list or element
defers to the run's boundary (`NIKA-SEC-004` there).

Determinism contract (normative) · no wall-clock · no randomness ·
integer/fixed-point pixel math · identical `(input bytes, args)` MUST
produce byte-identical artifacts across platforms and releases within a
stdlib major (the recipe carries the stable contract tag
`image_compose/v1`, never the engine build). An engine that cannot honor
this is not conformant (07-conformance §3). Labels and JPEG input are not
part of v1.

Throws · `NIKA-BUILTIN-IMAGE_COMPOSE-001` invalid arguments (unknown key ·
out-of-range value · `columns`/`rows` outside `grid` · `cell` with
`scale: none` · a grid smaller than the list · `out:` not `.png` ·
`validation_error`) · `-002` an input read failed (`tool_error` · the
message names the index) · `-003` unsupported input format (non-PNG ·
palette · exotic depth/interlace · `validation_error`) · `-004` decode
failed (structural corruption · CRC/Adler mismatch · `tool_error`) ·
`-005` a cap exceeded (`security_error` · never transient) · `-006`
artifact save failed (`tool_error`) — `image_fx`'s ladder, code for code.
Plus the boundary `NIKA-SEC-004` (paths outside `permits.fs`).

---

### `nika:tts_generate` · provider-backed speech synthesis (§Audio)

```yaml
narrate:
    invoke:
      tool: "nika:tts_generate"
      args:
        provider: local              # sovereign first — or system · openai · elevenlabs · mock
        text: "Bienvenue — the launch is live."
        voice: "alloy"
        output_dir: "./assets/audio"
```

Teaching shape · [`17-tts-self`](../examples/17-tts-self.nika)
(`provider: mock` · a real WAV · offline). Real speech with no key and no
network · `provider: system` (the host's own synthesizer, below).

The image family's contract, applied to audio: ONE audio file lands under
`output_dir:` (permit-gated per final path BEFORE I/O · atomic write ·
content-hash-named `{stem}-{provider}-{model}-{sha8}.{ext}` so identical
re-runs are idempotent), the output carries `{ provider, model, voice,
created_at, endpoint_host, audio: { path, filename, format, mime_type,
size_bytes, sha256, duration_ms }, cost_usd, warnings, manifest_path,
output_dir }` (plus `cost_basis` and `cost_as_of` beside a non-null
`cost_usd`, as `image_generate` · `catalog_rate_x_characters`: the
engine's dated catalog rate per character × the characters the request
sent, the text as trimmed · a provider's own exact report stays
`provider_reported`; plus `cost_unpriced` when the engine knows why
`cost_usd` is null · `system`) — **audio bytes NEVER ride outputs**, and a
sidecar provenance manifest (`manifest: true` default) echoes the resolved
request.

| Arg | Contract |
|---|---|
| `provider` | REQUIRED-or-inferred · closed: `local` (any OpenAI-speech-compatible self-hosted server — LocalAI · Kokoro-FastAPI · Speaches · openedai-speech · ONE wire: `POST {base}/v1/audio/speech` → raw bytes · base URL is ENGINE CONFIG `NIKA_TTS_LOCAL_URL`, default `http://localhost:8080`, never workflow data · never inferred) · `system` (the host's own speech synthesizer · keyless · offline · see below · never inferred) · `openai` · `elevenlabs` · `mock` (a REAL deterministic WAV · zero network/keys). Inference: `eleven*` → elevenlabs · `gpt-*`/`tts-*` → openai · `mock*` → mock. |
| `model` | per-provider default (reference engine 2026-07: `tts-1` local convention · `gpt-4o-mini-tts` · `eleven_multilingual_v2` · `mock-tts-1`) · `system` has no model choice: the model is the synthesizer's program name (`say` · `espeak-ng`), and any other `model:` is `-001` |
| `text` | REQUIRED · non-empty · ≤4096 chars (the strictest documented wire cap, held portably — fan longer scripts out with `for_each`) |
| `voice` | provider voice id · defaults: `alloy` (openai/local) · Rachel's public id (elevenlabs — ids are path components, so engines MUST restrict them to the id alphabet) · `sine` (mock) · `default` (system · the host's default voice) — a `system` voice is a host voice name (letters, digits, space and `( ) - _ . + '` · ≤ 64 chars · never a leading `-`); a name the host does not have fails the call, never falls back to the default voice |
| `format` | `mp3 \| wav \| auto` (default auto = provider-native) — the saved EXTENSION follows the sniffed bytes (magic authority), never the ask; a lossy fold is a `format_mismatch:` warning · `system` renders WAV |
| `speed` | 0.25–4.0 (openai/local native · `system`: words per minute = round(175 × speed), 175 being both synthesizers' default rate) — warned-dropped (`speed_unsupported:`) where the wire has no knob |
| `output_dir` · `filename_prefix` · `metadata` · `manifest` | as `image_generate` (stem falls back to `metadata.page_slug`, then `speech`) |
| `timeout_ms` | default 120000 · local 300000 (CPU synthesis) · max 600000 |

**Validation (engine MUST)** · the payload is sniffed (WAV `RIFF…WAVE` ·
MP3 `ID3`/frame-sync) — a non-audio payload is a hard `-007`; WAV
`duration_ms` is exact header math over the bytes actually PRESENT: a
`data` chunk declaring more than the payload carries (a streamed WAV's
`0xFFFFFFFF` placeholder size) counts only the bytes present, so a
streamed narration never reports a 24-hour duration. MP3 duration is
honestly `null` (never a guess). Empty payloads are `-004`.

**The `system` provider (keyless · offline · engine-run).** The engine
starts the host's own synthesizer ITSELF, as part of the builtin: macOS
`/usr/bin/say` (16-bit PCM WAV · 22050 Hz · written to an engine scratch
file, never a workflow path), Linux `espeak-ng --stdout` found in an
absolute directory of the engine's `PATH` (never a relative entry). The
text rides the program's standard input, never its argument vector, and is
spoken as TEXT, never synthesizer markup: both programs read `[[` as the
start of markup (`say`'s embedded commands such as `[[volm 0]]` ·
`espeak-ng`'s phonemes), so the engine breaks every `[[` with a space
before the text reaches them. The audio is capped at 64 MiB (`-003` ·
never transient · measured before it is read). It is never the
workflow's `exec:`: no `permits.exec` is required or consulted, the program is fixed by the engine (never
workflow data), and the audio still lands under `output_dir:` through
`permits.fs.write` like every provider. A static checker names the
program the call will start; the journal records the start as the
builtin's own effect (a `permit_checked` frame · `plane: builtin` · spec
17), never as an `exec:` decision. On any other operating system, or
when the program is absent, the call fails `-002` naming what is missing
— never a silent fallback to another provider. A non-zero exit is `-003`
(never transient · the synthesizer's message is carried). Cost · local
compute is unpriced, never free: `cost_usd` is `null`, the output names
the reason (`cost_unpriced: local_model`), and the run reports the task
unpriced on its journal frame the same way · never a priced `0`.

**Security (engine MUST)** · endpoints are engine-fixed constants (the
`local` base URL is engine config at the composition root — the same
sanction as `image_generate` and `infer:`) · keys are engine-configured
(`OPENAI_API_KEY` · `ELEVENLABS_API_KEY` · `NIKA_TTS_LOCAL_API_KEY` or
the `NIKA_`-prefixed forms), never workflow args, never logged · a
verbose local server reflecting the Bearer into an error body MUST be
scrubbed (the image family's rule).

Throws · `NIKA-BUILTIN-TTS_GENERATE-001` invalid arguments
(`validation_error`) · `-002` provider unavailable (a missing key · an
unwired plane · no system synthesizer on this host · `validation_error`) ·
`-003` request failed (`network_error` · `transient: true` for
5xx/408/429 + timeout/connection) · `-004` empty audio (`tool_error`) ·
`-005` content policy (`security_error` · never transient) · `-006` save
failed (`tool_error`) · `-007` payload validation (`tool_error`) — the
image twin's category ladder, code for code.

---

### `nika:video_compose` · native video assembly (§Media graduate #6)

```yaml
invoke:
  tool: "nika:video_compose"
  args:
    segments:                                # 1..=200 stills, in playback order
      - { image: out/frames/scene-1.png, audio: out/voice/scene-1.wav, pad_ms: 600 }
      - { image: out/frames/scene-2.png, audio: out/voice/scene-2.wav, pad_ms: 600 }
      - { image: out/frames/credits.png, duration_ms: 3000 }   # a still with no clip
    out: out/storyboard.mp4                  # the artifact · .mp4 (write-gated)
    size: "1280x720"                         # default · even sides
    fps: 25                                  # default
    scale: fit                               # fit (default) | fill | none
    audio_codec: aac                         # aac (default) | flac
```

Teaching shape · [`video-compose-storyboard`](../examples/video-compose-storyboard.nika)
(two photos and two mock narration clips · no `exec:` · no key).

The §Media graduate #6 — still images and their narration become ONE
video: an MP4 with H.264 video and AAC-LC or FLAC audio, cut from still to
still. Audio is the clock: each still holds for its clip (plus `pad_ms`)
or for its `duration_ms`, and the picture follows to the nearest frame.
The engine reads, checks and decodes every input itself, lays each still
out with `image_compose`'s geometry and builds the audio master, then
drives the ffmpeg installed on the host as its encoder, inside its own
confinement. The workflow declares no `exec:`, needs no `permits.exec` and
names no program: no argument or value names the encoder, so a later
native backend honours every workflow written against this contract. The
artifact lands on disk; outputs carry its receipt (path · sha256 ·
durations · frame and sample counts · the backend that encoded it), never
bytes.

| Arg | Contract |
|---|---|
| `segments` | REQUIRED · one or more items in playback order (more than 200 is the segment budget, `-005`) · each a CLOSED map with the keys `image` · `audio` · `duration_ms` · `pad_ms` (another key is `-001`) · a templated whole list, or an element that is itself a template, is resolved and judged at run |
| `segments[].image` | REQUIRED · a **media reference**: a path string, or an object carrying BOTH a string `path` and a string `sha256` of exactly 64 lowercase hex digits, the form every Nika producer writes — either key missing is `-001`, so a misspelt key can never drop the binding silently, and so is a digest of any other form (uppercase included); other keys are ignored, so a producer's output object passes as it is (`image_generate`'s `images[i]` · the outputs of `image_fx` and `image_compose`) · the bytes read MUST hash to `sha256` when one is given (`-004`) · **PNG v1** under the `image_fx` input law (depth 8 · gray, gray+alpha, RGB or RGBA · no Adam7 — anything else `-003`) · the read rides `permits.fs.read` |
| `segments[].audio` | OPTIONAL · a media reference to a **WAV** clip (`tts_generate`'s `audio` object passes as it is) · integer PCM of 8, 16 or 24 bits, or 32-bit float: format tag 1 (PCM) or 3 (IEEE float), or `WAVE_FORMAT_EXTENSIBLE` whose sub-format is one of those two (its container bits are the sample size · its channel mask is ignored) · 1 or 2 channels · 8000..=96000 Hz (another format `-003` with a hint · MP3: ask `tts_generate` for `format: wav` · AIFF is not part of v1) · the read rides `permits.fs.read` · never together with a top-level `audio` (`-001`) |
| `segments[].duration_ms` | 100..=600000 · REQUIRED for a segment without a clip · with a clip it MUST NOT be shorter than the clip (`-001` · never a silent cut), and the rest is silence |
| `segments[].pad_ms` | 0..=10000 · default 0 · silence after the clip · only for a segment with a clip and no `duration_ms` (`-001` otherwise) |
| `audio` | OPTIONAL · ONE soundtrack for the whole video · a media reference under the clip rules · every segment then needs `duration_ms` · a shorter track ends in silence, a longer one is cut at the end of the video with a `soundtrack_trimmed:` warning |
| `audio_codec` | `aac` (default · AAC-LC) · `flac` |
| `out` | REQUIRED · the artifact path · `.mp4` (else `-001`) · the write rides `permits.fs.write` |
| `size` | `WIDTHxHEIGHT` · both even · width 64..=3840 · height 64..=2160 · default `1280x720` |
| `fps` | integer 1..=60 · default 25 |
| `scale` | `fit` (default · the whole still, letterboxed on `background`) · `fill` (covers the frame, center-cropped) · `none` (native size, centered or center-cropped) — geometry below |
| `background` | `#rrggbb` or `[r, g, b]` · default `#000000` (the letterbox convention · `image_compose` defaults to white) · fills the letterbox and sits behind alpha |
| `quality` | `draft` · `standard` (default) · `high` — an ORDERED ladder each backend maps and records in `video.rate_control` and `audio.bitrate` (the ffmpeg backend: libx264 CRF 28 · 23 · 18 · AAC 48 · 64 · 96 kb/s per channel) |
| `transition` | `cut` (the only v1 value) · `fade` is RESERVED and refused loudly (`-001`) |
| `metadata` | a free object · at most 16 KiB (16384 bytes) as canonical JSON (`-001`): the JCS form [15](../spec/15-proof.md) hashes (RFC 8785 · sorted keys · no whitespace · UTF-8) · echoed into the outputs · the sha256 of that canonical JSON enters the recipe · it never reaches the encoder |
| `timeout_ms` | 1000..=3600000 · default 600000 · the call also ends at the task deadline, whichever comes first · waiting for an encode slot counts |

A static checker judges every literal value against this table before
anything runs (a literal list of more than 200 segments included); a
templated value is judged at run.

**Budgets (normative · `-005` · judged before any write).** At most 200
segments · a video of at most one hour (T ≤ 3600 × R, below) · each input
file within its byte cap, a still at most 512 MiB and a clip or soundtrack
at most 2 GiB, judged on the file's size before it is read and never read
past it · each still within the `image_fx` decoded-pixel budget (≤ 16384
per side · ≤ 2^26 pixels), judged from its header before any
decompression · each `fit` or `fill` scaled box (the placed size, before
the `fill` crop) at most 16384 per side and 2^26 pixels, `image_compose`'s
cap, judged from the header and `size` · the plan's intermediates,
`(segments + 1) × W × H × 3/2` bytes of 4:2:0 frames plus
`T × channels × 2` bytes of PCM (no PCM term for a silent video, which
stages no audio), at most 1 GiB (an engine's own scratch may hold more:
the reference engine stages RGB frames, about twice the figure) · any
argument handed to the encoder at most 64 KiB · the output at most
256 MiB, a tripped size limit included.

**Timing (normative · every backend).** R is the output sample rate and f
the frame rate; `round(a ÷ b)` is `⌊(a + ⌊b ÷ 2⌋) ÷ b⌋` (the
`image_compose` rounding), and all arithmetic is on integers ·

- segment k lasts D_k samples: `round(duration_ms × R ÷ 1000)` with
  `duration_ms`, else the clip's samples plus `round(pad_ms × R ÷ 1000)`;
  with a clip, `duration_ms` MUST give at least the clip's samples (`-001`);
  a segment without a clip is digital silence;
- segment k starts at S_k, the sum of the D_j before it, and the video
  lasts T samples, the sum of every D_k;
- the frame of sample position t is `F(t) = ⌊(2·t·f + R) ÷ (2R)⌋`:
  segment k shows the frames F(S_k) up to, not including, F(S_k + D_k), and
  the video has F(T) frames; a segment that would show no frame is `-001`.
  At every boundary picture and sound differ by at most 1/(2f) seconds, and
  the error never accumulates;
- keyframes · let G = 10 × f frames: every segment starts with an IDR
  frame, and the gap between two consecutive keyframes never exceeds G
  frames (a seek inside a long still costs at most ten seconds of frames).
  A backend MAY add keyframes; the ffmpeg backend places them exactly at
  each segment start s and at s + m·G inside the segment (m ≥ 1).

**Preparation (normative · every backend).** The engine prepares the
encoder's input itself, so every backend receives the same plan ·

- **Sample rate R** · the clips' common rate (the soundtrack's rate ·
  48000 for a video with no audio at all, its silent clock). Clips at
  different rates MAY be refused `-003` (the reference engine does in this
  version: « clips must share one sample rate »); an engine that accepts
  them uses R = 48000, a clip of n samples at rate r lasting
  `round(n × R ÷ r)` samples at R. With `audio_codec: aac`, R is an MPEG-4
  sampling rate (7350 · 8000 · 11025 · 12000 · 16000 · 22050 · 24000 ·
  32000 · 44100 · 48000 · 64000 · 88200 · 96000): a common rate outside that
  table MAY be refused `-003` (« use flac, or resample upstream »), and an
  engine that accepts it uses R = 48000 as above. `flac` keeps any rate.
- **Channels** · stereo when any clip is stereo (a mono clip plays on both
  channels), else mono.
- **The master** · 16-bit little-endian PCM at R: wider integer samples are
  rounded, never dithered · float samples are clamped · 8-bit samples are
  widened. A WAV's `data` size is the smaller of the declared size and the
  bytes present (a streamed WAV's `0xFFFFFFFF` placeholder counts only the
  bytes present). A malformed WAV is `-004`, never a crash.
- **Soundtrack** · the top-level `audio` plays from the first frame;
  shorter than T it ends in silence, longer it is cut at T.
- **Stills** · each image is decoded by the engine, its alpha composited
  over `background` BEFORE any scaling, per channel
  `⌊(c·a + bg·(255 − a) + 127) ÷ 255⌋`, then placed in a W × H frame
  (`size`): `fit` and `fill` are `image_compose`'s geometry with ONE cell of
  W × H (its placed sizes, its rounding, its centering, its cover crop, its
  `image_fx` resize with `filter: auto` in linear light) · `none` scales
  nothing: per axis, an image no larger than the frame is placed at
  `⌊(W − w) ÷ 2⌋` with `background` around it, a larger one is cropped from
  `⌊(w − W) ÷ 2⌋`.
- **Colour** · the video is Y′CbCr 4:2:0 with BT.709 primaries, transfer
  and matrix in limited range, tagged so in the bitstream and the container
  (`video.color: bt709/limited`). The conversion from the composited RGB
  frame is the backend's in this version: pixel values are not normative.

**The raw-only invariant (normative).** The encoder never parses a
workflow byte. The engine reads each declared input once through its judged
filesystem, checks its `sha256` binding and decodes it, then writes the
encoder's only inputs itself: headerless raw samples (the prepared frames ·
the 16-bit PCM master) in a private scratch directory it created. For the
ffmpeg backend that is four rules, each an engine MUST ·

1. every input operand is a scratch file the engine wrote, behind a forced
   raw demuxer (`-f rawvideo` with its pixel format, size and rate ·
   `-f s16le` with its rate and channel count): no container parser, codec
   parser or content probe ever reads workflow bytes;
2. `-protocol_whitelist file` comes before EVERY operand, each input and
   the output: the option binds the next operand only, so one occurrence
   fences one operand;
3. the engine alone builds the argument vector and starts it without a
   shell; every element is built only from engine constants, integers of
   the plan, scratch paths and the recipe (hashes, integers and closed
   words) — no workflow string reaches it: not `out:`, not `metadata`, not
   a file name, not a rendered template;
4. ffmpeg writes its output into the scratch only; the engine reads it
   back, verifies it, hashes it and saves it (below): ffmpeg never writes
   `out:`, and nothing is moved from the scratch to `out:`.

ffmpeg also reads no terminal input (`-nostdin`) and runs with a cleared
environment, with the scratch as its working directory. An engine that
cannot uphold these rules MUST NOT start the program. The vector is part of
the receipt: the ffmpeg backend records the one it started in
`backend.details.argv` (below), the program first, with the scratch
directory written `<scratch>` and the recipe written `<recipe>`
(`recipe_sha256` is its digest), so a reviewer, a trace verifier or a
conformance runner can check the forced demuxers, the whitelist before
each operand and the absence of any workflow string from the outputs and
the journal alone.

**The order law and the trifecta.** The order law (`NIKA-SEC-015` ·
[10](../spec/10-authority.md)) refuses an `exec:` task downstream of a
net-effecting task, and it still does when the program is ffmpeg: an
`exec:` hands its program workflow-built arguments and workflow files, so
fetched content can reach a parser, a protocol or an option.
`nika:video_compose` downstream of a fetch is not refused, and the
raw-only invariant is why: fetched content can change the picture and the
sound — pixel and sample values, a duration — never what ffmpeg parses,
opens or reaches. The lethal-trifecta law (`NIKA-SEC-009`) counts the call
as an `fs` write egress (its artifact · its media inputs are reads), like
`image_compose`: untrusted content that reaches its arguments needs the
human gate that law asks for. It is never an ingress source, as an `exec:`
is: an `exec:` outputs whatever its program prints, while this call adds
nothing to its outputs but engine-computed facts about the file and the
encoder. What the outputs echo of the arguments (`metadata` · the paths
read) keeps the taint it came with: untrusted content that enters the call
through them leaves it in the output, and an egress task that reads that
output needs the gate as if it read the content itself.

**Authority.** The tool grant · `permits.fs.read` for every media input
(each `segments[].image`, each `segments[].audio` and the top-level
`audio` · a path string or an object's `path`) · `permits.fs.write` for
`out:` — the `image_compose` and `vision:` pattern. No `permits.exec` is
required or consulted, and none is implied. A static checker judges each
literal media path (or whole bare `const:` reference) as a read and a
literal `out:` as a write, before anything runs: a missing grant is a
check-time `NIKA-SEC-004` (`NIKA-AUTH-006` under an absent block, whatever
form the paths take: every call reads a still and writes `out:`), never a
surprise at run, and a read grant a media input uses is a use, never
`declared-but-unused` advice. A templated list, element or path defers to
the run's boundary (`NIKA-SEC-004` there, each resolved path judged before
it is opened), except under a boundary that admits no read or no write at
all: no rendered value fits an empty grant, so the call is refused at check
whatever form its paths take. The inferred boundary (`nika check
--infer-permits`, and the block a `NIKA-AUTH-006` carries) lists each
literal workspace media path in `fs.read` and a literal `out:` in
`fs.write`, so it re-checks clean. A computed path cannot be printed: for
a computed list, element, path or `out:` the inference says its `fs` face
is partial, names the task and asks for the resolved paths (the
storyboard's case: the directories its producers write), and the block
re-checks clean once they are granted. A media path or an `out:` that
escapes the workspace is named for review, never printed as a grant (the
`vision:` rule). A
static checker names the program the call starts, and where it is looked
for, without probing the host: a host-dependent finding would break
check ≡ run.

**The host program (normative for the ffmpeg backend).** The engine
resolves ffmpeg from its own configuration, never from the workflow:
`NIKA_FFMPEG` (an absolute path · engine configuration read at the
composition root · a project file cannot set it), else the first trusted
candidate among `/opt/homebrew/bin/ffmpeg`, `/usr/local/bin/ffmpeg` and
`/usr/bin/ffmpeg`. `PATH` is never consulted: it depends on how the engine
was started, and a project can steer it. A candidate is trusted when its
canonical path is a regular executable file and no directory above the
candidate or above its canonical path is writable by others. A set
`NIKA_FFMPEG` that is not trusted is refused naming the variable; it never
falls through to the list. The floor, which the engine's capability probe
reads (`-version` · the encoder list), is FFmpeg 6.0 (libavformat and
libavcodec majors ≥ 60), with the `libx264` encoder and the requested
audio encoder (ffmpeg's own `aac` · `flac`). No trusted candidate, or
anything missing, is `-010`, naming each path tried, what is missing and
the fix (install ffmpeg · set `NIKA_FFMPEG`), never a fallback to another
program or codec. These rules are a floor, not a complete list: an engine
MAY refuse more (an owner or group rule · a program under the project
root), and a program that passes only the floor is not one a workflow can
count on.

The resolution and the probe MAY be cached per process; an engine that
caches them resolves once more when the program is gone at start (an
upgrade moved it) before failing `-010`. One rule is never cached: on every
call, the resolved program is refused when the workflow's
`permits.fs.write` admits its candidate path or its canonical path. The
call fails `-010` and never falls through to another candidate, so a
workflow never starts a program file its own grant could have written. The
libraries the program loads are outside that rule in this version: an
engine MAY refuse a program whose libraries a write grant admits.

**Confinement (normative).** ffmpeg runs inside the engine's own OS
confinement, the jail that confines `exec:` tasks: it receives no workflow
path, may write only engine scratch, and has network egress denied as far
as the host's jail denies it. An engine MUST NOT describe the call as
having no network access (in a check line, a review, an output or its
documentation) unless its confinement on that host denies every network,
loopback included: where outbound loopback stays reachable, or confinement
is waived, the whitelist inside ffmpeg is the only fence for it.
Confinement the operator waived (the reference engine's
`NIKA_SANDBOX=off`) runs ffmpeg unconfined, still under the raw-only
invariant, and the outputs say `backend.confinement: "waived"`. No
confinement that can start and no waiver is `-011`, and so is a profile
that cannot be built or a denial the jail reports during the call.

**Refusing early (normative · forward-compatible).** The trigger is
static: a workflow with at least one `invoke:` task whose `tool` is
`nika:video_compose`, whether or not a `when:` later skips it. When an
engine can tell before such a run's first effect that the host cannot
encode — no trusted program, one below the floor or without an encoder the
calls need, no confinement under a policy that requires one — it SHOULD
refuse the launch (no task runs, nothing is spent) rather than fail the
call after paid image or voice calls. An engine that learns it only at the
call fails there (`-010` · `-011`). A grant alone never triggers a launch
refusal: a run whose workflow reaches the builtin only through a grant
(`permits.tools`, a glob such as `nika:*` included, or an `agent:`
whitelist) launches, and an agent's call, if one comes, fails at the call.
Two outcomes are therefore not contracts a workflow can rely on:
recovering from `-010` or `-011` (an engine that refuses at launch never
reaches the recovery), and completing on a host that cannot encode because
the `nika:video_compose` task was skipped (an engine that refuses at launch
refuses that run).

**The scratch (normative).** Each call claims a fresh private directory by
exclusive creation (never adopting an existing path or a planted link),
mode 0700, and creates its files 0600: no other local user reads the
decoded stills, the audio master or the video while the encode runs. It is
engine state, never a workflow path, removed when the call settles
(success · failure · deadline · Stop). An engine also sweeps the scratch a
crashed engine left behind, and only that: a scratch is stale when its
owner is provably gone (the owner holds a lock on it for the whole call
and that lock is free, or it records its process and that process no
longer runs) or when it is older than the longest call, 3,600,000 ms,
plus ten minutes. A sweep never removes the scratch of a live call, another
engine process's under the same user included. A full disk is `-006`.

**Deadlines and Stop.** An engine MAY run one encode at a time per process
(the reference engine does); waiting for it counts against `timeout_ms` and
the task deadline. When the deadline passes, waiting or encoding, the
program is killed, drained and reaped, nothing is written, and the call
fails `-009`. A user's Stop ends the call through the run's cancellation —
the program killed, the scratch removed, nothing written — never through
`-009`.

**Verified before saved (normative).** An exit status of 0 proves nothing:
ffmpeg's size limit is soft and can leave a valid, shorter file with status
0. Before anything reaches `out:`, the engine reads the file back and checks
it against the plan without decoding it · `ftyp` first and `moov` before
`mdat` (the video starts playing before it is fully downloaded) · the
movie timescale (`mvhd`) is R, so a duration counts in samples · one
H.264 video track of the planned size whose sample count is the planned
frame count, every segment start a sync sample and no gap between sync
samples above G · one audio track (none for a silent video) of the planned
codec, rate and channel count, its media timescale (`mdhd`) R, whose
presented length is exactly T samples: with an edit list, the list is one
normal edit (media rate 1 · a media time other than −1) whose segment
duration, in the movie timescale, is T (any other edit list is a
mismatch); without one, the track's sample durations sum to T · at most
256 MiB.
A short file that reached the size cap is `-005`, any other mismatch
`-007`, and nothing is written. Then the bytes are hashed, compared with the
bytes already at `out:` when a read grant allows it (identical bytes are an
idempotent skip · `skipped_existing` · the `image_fx` law), and saved
atomically under `permits.fs.write`.

Output · `{ path, sha256, size_bytes, mime_type, container, duration_ms,
width, height, fps, frames, keyframe_interval_frames, video: { codec,
profile, pixel_format, color, quality, rate_control: { mode, value } },
audio: { codec, sample_rate, channels, samples, bitrate }, segments:
[{ index, image, image_sha256, audio, audio_sha256, start_ms, duration_ms,
first_frame, frames }], backend: { kind, version, build_id, encoder,
encoder_build, threads, confinement, details }, determinism, contract,
recipe_sha256, skipped_existing, warnings, metadata }` — artifact bytes
NEVER ride outputs (the disk law) ·

- `mime_type` is `video/mp4` · `container` `mp4` · `video.codec` `h264` ·
  `video.profile` `high` · `video.pixel_format` `yuv420p` · `video.color`
  `bt709/limited` · `video.quality` the `quality` asked · `frames` is F(T)
  and `keyframe_interval_frames` G · `duration_ms` is
  `round(T × 1000 ÷ R)`, and a segment's `start_ms` and `duration_ms` the
  same rounding of S_k and D_k · `first_frame` is F(S_k) and `frames` the
  segment's frame count · `image` and `audio` are the paths read and
  `image_sha256`, `audio_sha256` the digests of the bytes read (`audio` and
  `audio_sha256` are `null` for a segment without a clip);
- `audio` is `null` for a silent video · `audio.codec` is the `audio_codec`
  asked (`aac` · `flac`) · `audio.sample_rate` is R and `audio.channels`
  the planned count · `audio.samples` is T and `audio.bitrate` the
  encoder's target in bits per second (`null` for `flac`);
- `video.rate_control` is `{ mode, value }`: `mode` the closed set `crf` ·
  `q` · `qp`, `value` an integer (the ffmpeg backend: `crf` 28 · 23 · 18) ·
  `backend.kind` names the backend (`ffmpeg` for this one) ·
  `backend.confinement` names the jail that confined the program as the
  run prologue's `sandbox` field names it ([17](../spec/17-trace.md) ·
  `seatbelt` · `landlock` in this version, and a later jail by its own
  name), or is `waived`;
- every key outside `backend.details` exists for every backend ·
  `backend.details` is an open object the backend owns (for ffmpeg · the
  program path · the library versions · `argv_version` · `argv`, the
  vector the raw-only invariant records) · `backend.build_id` is an opaque
  build fingerprint (for ffmpeg · the sha256 of its `-version` output) ·
  `backend.encoder_build` is the encoder's own build string as the file
  records it (x264 writes one in the first IDR frame), read back after the
  encode and `null` when the encoder records none;
- `contract` is `video_compose/v1` · there is no `cost_usd` and no
  `cost_unpriced` (below).

**Determinism (normative).** `determinism` is `none` in this version: no
byte-identity claim, even for two runs on one host. The closed vocabulary
is `none` · `same_host` (the same build, host, thread count, inputs and
arguments give the same bytes) · `cross_platform` (the same bytes
everywhere); a later backend may raise the value without changing a
workflow. The recipe records what made the file, as canonical JSON (the
JCS form of `metadata` above) with no timestamp, no absolute path, no host
name and no workflow string, built only from hashes, integers and closed
words that are all known before the encode · the contract tag · the
normalized arguments (`size` as its two integers · `fps` · `scale` ·
`background` as its three integers · `quality` · `audio_codec` ·
`transition`) · the plan (R · channels · T · F(T) · G · the segment start
frames) · per segment the still's and the clip's sha256 with its
`duration_ms` or `pad_ms` · the soundtrack's sha256 · the sha256 of
`metadata`, never the object · the backend (`kind` · `build_id` · `encoder`
· `threads` · the rate control · for ffmpeg `argv_version`). That is what
lets it ride the encoder's own argument vector (raw-only rule 3) and so
the file (the `nika` key of the MP4's metadata), while `recipe_sha256`
rides the outputs. The program's version strings and `encoder_build` stay
in the outputs: `build_id` already fingerprints the `-version` output, and
the encoder's build string exists only once the file does. A re-render is a
consistency check, not a tamper proof. `nika:video_compose` is therefore
outside the byte-identity bar of [07](../spec/07-conformance.md): its bar is
structural (the verified file above).

Cost · nothing to meter: the output carries no `cost_usd` and no
`cost_unpriced`, like `image_fx`, `image_compose` and `chart`. The call
leaves the run's spend qualifier untouched, is never a priced `0` and is
never called free.

Events · the engine's progress stream for interfaces and SDKs (event
names, not journal kinds · [17](../spec/17-trace.md)'s vocabulary is
unchanged) · `video_compose.started` (segment count · size · fps · `out` ·
program · version · encoder · milliseconds waited for the encode slot) ·
`video_compose.segment` (once per segment while preparing · index ·
frames · elapsed milliseconds) · `video_compose.encoding` (program ·
encoder · threads · confinement) · `video_compose.completed` (path ·
sha256 prefix · size · duration · preparation and encode milliseconds) ·
`video_compose.error` (code · message). The journal records each start of
the program, its capability probe included, as the builtin's own effect:
one `permit_checked` frame with `plane: builtin`, the program's canonical
path as `gate`, `decision: allow`, and a `why` naming the builtin, the
program's version, the encoder and the confinement — never an `exec:`
decision.

Throws · `NIKA-BUILTIN-VIDEO_COMPOSE-001` invalid arguments (shape ·
closed sets · bounds · `fade` · the extension · a soundtrack with clips · a
duration shorter than its clip · a segment that would show no frame · a
media-reference object missing `path` or `sha256`, or whose `sha256` is not
64 lowercase hex digits · `metadata` over 16 KiB · `validation_error`) ·
`-002` an input could not be read (the message names the segment ·
`tool_error`) · `-003` unsupported input format, with a
hint (JPEG or WebP: produce PNG upstream · MP3: ask `tts_generate` for
`format: wav` · AIFF · palette PNG · Adam7 · more than 2 channels · clips
at different rates, or an `aac` rate outside the MPEG-4 table, where the
engine refuses them · `validation_error`) · `-004` decoding failed (a
corrupt PNG or WAV) or an input whose bytes do not match its `sha256`
(`tool_error`) · `-005` a budget exceeded (an input file above its byte
cap · pixels · a scaled box · duration · segments · the plan's
intermediates · an encoder argument above 64 KiB · the output size, a
tripped size limit included · `security_error` · never transient)
· `-006` saving failed (the scratch could not be created or written · the
atomic save failed · `tool_error`) · `-007` output validation failed: the
file read back does not match the plan, and nothing is written
(`tool_error`) · `-008` the encoder failed: it exited non-zero or was
killed by a signal other than the engine's (the message carries the status
and at most 400 characters of its error output · `tool_error` · not
transient) · `-009` the deadline passed, waiting for the encode slot or
encoding: the program, if started, was killed, drained and reaped, and
nothing was written (`tool_error` · not transient) · `-010` no usable
encoder on this host: ffmpeg absent, `NIKA_FFMPEG` invalid, no trusted
candidate, a program the workflow's `permits.fs.write` admits, a program
whose libraries cannot load inside the jail (the loader's message is
classified before any confinement denial), a
version below the floor, no H.264 encoder, the requested audio encoder
missing, a muxer, filter or option the engine's capability probe finds
missing (one the probe does not cover fails at the encode, `-008`), or an
operating system other than macOS and Linux (the message names what is
missing and the fix · never a fallback · `validation_error`) · `-011`
confinement failed: none could start and none was waived, a profile that
cannot be built, or a denial the jail reported during the call
(`security_error` · never transient) — `image_fx`'s ladder for `-001` to
`-006`, the generators' `-007`, then the encoder's own `-008` to `-011`.
Plus the boundary `NIKA-SEC-004` (paths outside `permits.fs`).

> **Reference engine (nonnormative).** The engine includes, links and
> distributes no codec: it starts the user's own ffmpeg, whose build
> carries its own licence. The official container image ships no ffmpeg; a
> derived image adds it. On stock Ubuntu 24.04 the AppArmor restriction on
> unprivileged user namespaces stops the Linux jail, as an unprivileged
> container does: the call is refused `-011` until the operator changes the
> host (an AppArmor profile for the jail, or the user-namespace setting) or
> waives confinement. An ffmpeg linked against libraries outside the system
> roots (a Nix or Conda build) cannot load inside the jail (`-010`): use a
> static build through `NIKA_FFMPEG`. VideoToolbox is not used in this
> version.

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
| `nika:json_to_csv` | `nika:convert to: csv` (NOT jq — the embedded jaq has no `@csv` filter) |
| `nika:json_merge` (recursive `*`) | `.[0] * .[1]` on a `[base, overlay]` input (jaq `obj_merge` · source-verified 2026-05-27) |

Also cut · `nika:task_status` (use `${{ tasks.X.status }}`) · `nika:orchestrate`
(use `for_each` for bounded fan-out · a sub-workflow call is the `invoke:
workflow:` tagged union per `14-composition.md` · never a builtin) ·
`nika:locale_lookup` (niche i18n → stdlib-extended).

---

## Media builtins · the REST stays DEFERRED (stdlib v0.x)

`image_generate` graduated 2026-07-05 · `tts_generate` same arc ·
`image_fx` graduated 2026-07-09 (the image *editing* row · §Media #3) ·
`chart` graduated 2026-07-09 (above · §Media #4) · `image_compose`
graduated 2026-10-26 (§Media #5 · the composition a workflow otherwise
borrowed from `exec`) · `video_compose` graduated 2026-10-28 (§Media #6 ·
the assembly a workflow otherwise borrowed from `exec ffmpeg`). The
remaining media class
(pdf_extract · svg_render · phash · provenance ·
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

🦋 *<!-- canon:builtins -->31<!-- /canon --> builtins canonical · jq = the data language · 5-layer Rams symmetry (fetch+extract · jq · convert · wait · inspect) · assets land on disk, never inline · clear forever.*
