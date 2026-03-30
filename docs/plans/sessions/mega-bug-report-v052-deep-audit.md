# Mega Bug Report — Deep Audit Post v0.52.0

> Generated: 2026-03-30 | Base: v0.52.0 + 10 commits | 8,914 tests | 0 clippy
> Tested with: OpenAI (gpt-4.1-mini), xAI (grok-3-mini), Gemini (gemini-2.5-flash)
> Not tested: Anthropic (billing), Mistral/Groq/DeepSeek (no API keys)

---

## SESSION SUMMARY: What This Session Did

| # | Commit | Impact |
|---|--------|--------|
| 1 | `chore(deps): remove 6 dead workspace deps` | nutype, static_assertions, strum, derive_more, tracing-error, console |
| 2 | `ci: cargo-deny/machete hard fail` | 11 crate deps removed + deny.toml fix + `\|\| true` removed |
| 3 | `fix(streaming): log try_send (9 sites)` | stream_send! macro replaces silent `let _ =` |
| 4 | `refactor(engine): ProviderName migration (33 files)` | Option\<String\> → Option\<ProviderName\> |
| 5 | `style: rustfmt pass` | 7 files |
| 6 | `docs(secrets): fix boot comment` | env vars + daemon IPC, not keychain |
| 7 | `chore: Dockerfile 0.40.2→0.52.0` | version bump |
| 8 | `test(e2e): 12 adversarial tests` | data flow traps, structured stress, concurrency |
| 9 | `refactor(secrets): provider list — env/daemon/keychain` | daemon IPC before keychain |
| 10 | `test(daemon): 4 auto-start tests` | NIKA_NO_DAEMON, env fallback, clear errors |

---

## REAL PROVIDER EXECUTION RESULTS

### Validation (nika check)

| Category | Count | Result |
|----------|-------|--------|
| Non-error examples | 450 | **450/450 pass** |
| Error examples (should fail) | 52 | 33 caught at check, 19 are runtime-only |

### Execution (nika run — real APIs)

| Category | Provider | Result | Details |
|----------|----------|--------|---------|
| DAG patterns (exec) | N/A | **15/15** | All topology patterns work |
| Use-cases | OpenAI (gpt-4.1-mini) | **28/41** | 13 failures analyzed below |
| Use-cases | xAI (grok-3-mini) | **8/8** | Perfect on tested subset |
| Structured gates | OpenAI | **5/6** | 1 missing temp file |
| Infer gates | OpenAI | **4/5** | max_tokens:10 < provider min 16 |
| Infer gates | xAI | **5/5** | Perfect |
| Infer gates | Gemini | **1/5** | 429 rate limit (free tier) |
| Combo gates | OpenAI | **9/10** | 1 missing context file |
| Complex gates | OpenAI | **3/5** | 2 missing files/tools |

---

## BUG-1 [HIGH]: Structured Output L3/L4 Layers Disabled on First Attempt

### Symptom
```
⬡ L3: retry_with_feedback ✗ (retry 1: no infer callback - Layer 3 disabled)
⬡ L3: retry_with_feedback ✗ (retry 2: no infer callback - Layer 3 disabled)
⬡ L4: llm_repair ✗ (no infer callback - Layer 4 disabled)
```

### Root Cause
`StructuredOutputEngine` is created WITHOUT `infer_fn` callback in 3 out of 4 code paths:

| Path | File | Line | infer_fn | When |
|------|------|------|----------|------|
| Layer 0a (native response_format) | executor/infer.rs | 533 | **NONE** | Provider supports response_format |
| Layer 0b (tool injection) | executor/infer.rs | 712 | **NONE** | Tool injection fallback |
| Main path (L1-L3) | executor/infer.rs | 1016 | **SET** | Standard validation |
| Runner fallback | runner.rs | 1145 | **NONE** | Defensive path |

### Impact
- L3 (retry with feedback) and L4 (LLM repair) are disabled when validation happens after L0a/L0b
- The system recovers by retrying the full pipeline (L0→L2 again), but this costs extra LLM calls
- **7 use-case workflows showed flaky structured output** (all passed on 2nd run)

### Affected workflows
content-pipeline, etl-pipeline, meeting-notes, multi-step-analysis, onboarding-checklist, parallel-research, quiz-generator, seo-keywords

### Fix
In executor/infer.rs at lines 533 and 712, pass the `infer_callback` to the StructuredOutputEngine:
```rust
// Current (Layer 0a safety net, line 533):
let mut engine = StructuredOutputEngine::new(spec, Arc::new(self.event_log.clone()));

// Fix:
let mut engine = StructuredOutputEngine::new(spec, Arc::new(self.event_log.clone()))
    .with_infer_callback(infer_callback.clone());
```

Same fix at line 712 (Layer 0b) and runner.rs line 1145. The `infer_callback` needs to be created earlier in the function and shared across all paths.

### Reproduction
```bash
# Run multiple times — sometimes L0 fails and L3/L4 can't recover:
nika run examples/use-cases/quiz-generator.nika.yaml --provider openai --model gpt-4.1-mini --no-live 2>&1 | grep "L3:"
```

---

## BUG-2 [MEDIUM]: `default()` Transform Does Not Trigger on Empty Strings

### Symptom
```yaml
# Empty string from exec:
- id: empty
  exec: "echo ''"
- id: use
  with: { val: $empty }
  exec:
    command: "echo '{{with.val | default(\"FALLBACK\")}}'"
    shell: true
# Output: ""  (empty, NOT "FALLBACK")
```

### Expected
`default("FALLBACK")` should trigger when value is empty string `""`.

### Actual
`default()` only triggers on JSON null, not on empty string. This is inconsistent with common expectations.

### Root Cause
The `default` transform implementation checks for `Value::Null` but not for `Value::String("")`.

### Fix
In the transform implementation (likely `nika-engine/src/binding/transforms.rs`), add empty string check:
```rust
// Current:
Value::Null => Ok(Value::String(fallback.to_string())),

// Fix:
Value::Null | Value::String(s) if s.is_empty() => Ok(Value::String(fallback.to_string())),
```

### Reproduction
```bash
cat > /tmp/test.nika.yaml << 'EOF'
schema: nika/workflow@0.12
workflow: test
provider: mock
tasks:
  - id: empty
    exec: "echo ''"
  - id: use
    depends_on: [empty]
    with: { val: $empty }
    exec:
      command: "echo 'result={{with.val | default(\"X\")}}'"
      shell: true
EOF
nika run /tmp/test.nika.yaml --no-live 2>&1 | grep "result="
# Expected: result=X
# Actual:   result=
```

---

## BUG-3 [MEDIUM]: NIKA-060 JSON Extraction Fails on Markdown Code Fences

### Symptom
```
error NIKA-060: Invalid JSON output: Failed to extract JSON from output.
First 200 chars: ```json
```

### Root Cause
When `output: { format: json }` is used (NOT `structured:`), the LLM wraps JSON in markdown code fences:
```
\`\`\`json
{"key": "value"}
\`\`\`
```
The JSON extractor doesn't strip these fences before parsing.

### Affected Workflows
- examples/use-cases/changelog-generator.nika.yaml
- examples/use-cases/data-etl.nika.yaml

### Impact
Workflows using `output: { format: json }` fail when the LLM wraps output in markdown fences. The `structured:` path handles this correctly (L2 extract_validate strips fences), but the `output: json` path doesn't.

### Fix
In the JSON output validation path, strip markdown code fences before parsing:
```rust
fn strip_json_fences(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with("```json") || s.starts_with("```") {
        let s = s.trim_start_matches("```json").trim_start_matches("```");
        s.trim_end_matches("```").trim()
    } else {
        s
    }
}
```

### Reproduction
```bash
nika run examples/use-cases/changelog-generator.nika.yaml --provider openai --model gpt-4.1-mini --no-live 2>&1 | grep "NIKA-060"
```

---

## BUG-4 [LOW]: YAML Anchors (&/\*) Not Supported

### Symptom
```
× [NIKA-001] Failed to parse workflow: [NIKA-160] YAML syntax error:
  Unexpected definition of anchor
```

### Root Cause
The YAML parser (`marked-yaml`) doesn't support YAML anchor/alias syntax (`&name` / `*name`). This is valid YAML 1.2 syntax for avoiding repetition.

### Impact
Users can't DRY-up repeated schema definitions:
```yaml
# This FAILS:
structured:
  schema: &my_schema
    type: object
    properties: { name: { type: string } }

# Later:
structured:
  schema: *my_schema  # References the anchor — NOT SUPPORTED
```

### Workaround
Copy the schema to each task manually.

### Fix
Either switch to a YAML parser that supports anchors, or document this as a known limitation. `serde_yaml` or `saphyr` support anchors.

---

## BUG-5 [LOW]: OpenAI Rejects max_tokens < 16

### Symptom
```
Invalid 'max_output_tokens': integer below minimum value.
Expected a value >= 16, but got 10 instead.
```

### Root Cause
OpenAI's API requires `max_output_tokens >= 16`. Nika doesn't validate this before sending.

### Impact
Example file `infer-full-form.nika.yaml` sets `max_tokens: 10` which fails on OpenAI but works on xAI.

### Fix
Add provider-specific validation in the executor:
```rust
if provider_name == "openai" && max_tokens.map_or(false, |t| t < 16) {
    max_tokens = Some(16); // clamp to minimum
}
```
Or: validate at parse time and warn.

---

## BUG-6 [LOW]: NIKA-053 False Positive on Multi-Line Echo with Quotes

### Symptom
```
NIKA-053: Blocked dangerous command: 'echo 'SOURCE 1 (docs/mcp-config.md):'
```

### Root Cause
Multi-line `exec: command: |` with nested single quotes and parentheses triggers the security blocklist. The command is just an `echo` but the pattern matcher sees suspicious content.

### Affected
examples/use-cases/knowledge-base.nika.yaml

### Fix
The security blocklist should not flag `echo` commands regardless of content, or the pattern matching should be more context-aware.

---

## BUG-7 [INFO]: `${VAR:-default}` Shell Syntax Not Resolved in URLs

### Symptom
```
[NIKA-004] fetch: URL must use http:// or https:// scheme,
got: ${URL:-https://httpbin.org/html}
```

### Root Cause
Shell-style `${VAR:-default}` syntax is not resolved by Nika. Workflows should use `inputs:` + `{{inputs.url}}` instead.

### Affected
- examples/use-cases/competitive-analysis.nika.yaml
- examples/use-cases/content-audit.nika.yaml

### Fix
These are YAML authoring bugs. Fix the example files to use `inputs:` syntax instead of shell `${}`.

---

## BUG-8 [INFO]: Agent Completion Shows LowConfidence(0.0)

### Symptom
```
⊗ done 1 turns · LowConfidence(0.0)
```
Even when the agent successfully called `nika:complete` with mode `explicit`.

### Impact
Cosmetic — doesn't affect functionality. The agent still completes and produces output.

### Root Cause
The confidence calculation doesn't account for `nika:complete` tool calls properly.

---

## FEATURES VERIFIED WORKING

| Feature | Provider(s) | Status |
|---------|------------|--------|
| Basic infer | OpenAI, xAI, Gemini | ✅ |
| Structured output (L0 response_format) | OpenAI, xAI | ✅ |
| Structured output (L2 extract_validate) | OpenAI, xAI | ✅ |
| Structured output (L3 retry) | OpenAI | ✅ (when callback wired) |
| for_each with concurrency | OpenAI, xAI | ✅ |
| for_each with $task.path binding | OpenAI | ✅ |
| for_each + structured per iteration | OpenAI | ✅ |
| depends_on ordering | All | ✅ |
| with: bindings + path access ($task.field.sub) | OpenAI | ✅ |
| ?? fallback operator | Mock | ✅ |
| Pipe transforms (upper, trim, join, length, first, to_json) | All | ✅ |
| fetch: GET, POST, json body | N/A | ✅ |
| fetch: extract markdown | N/A | ✅ |
| fetch: extract metadata | N/A | ✅ |
| fetch: extract jsonpath | N/A | ✅ |
| fetch: extract links | N/A | ✅ |
| fetch: response full (status, headers, body) | N/A | ✅ |
| exec: shell commands | N/A | ✅ |
| exec: template resolution in commands | N/A | ✅ |
| agent: with builtin tools | OpenAI | ✅ |
| agent: max_turns limit | OpenAI | ✅ |
| agent: guardrails (length, regex) | OpenAI | ✅ |
| agent: completion mode natural | OpenAI | ✅ |
| retry: with backoff | OpenAI | ✅ |
| context: file loading | N/A | ✅ (with correct paths) |
| Unicode/accent support in prompts + bindings | OpenAI | ✅ |
| additionalProperties: false | OpenAI, xAI | ✅ |
| Multi-provider in same workflow | OpenAI + xAI | ✅ |
| DAG: diamond, tree, fan-out, parallel | All | ✅ |
| 15 DAG topology patterns | All | ✅ |

---

## PRIORITY FIX ORDER

```
1. [HIGH]   BUG-1: Wire infer_callback to L0a/L0b StructuredOutputEngine
            → Fixes flaky structured output, reduces LLM calls
            → 3 code sites: executor/infer.rs (2) + runner.rs (1)

2. [MEDIUM] BUG-3: Strip markdown code fences in output:json extractor
            → Fixes NIKA-060 failures in 2 use-cases
            → 1 code site

3. [MEDIUM] BUG-2: default() transform on empty strings
            → Semantic fix — empty string should trigger default
            → 1 code site in transforms.rs

4. [LOW]    BUG-5: Clamp max_tokens to provider minimums
            → Prevents confusing provider errors
            → Provider-specific validation

5. [LOW]    BUG-4: Document YAML anchor limitation
            → Or switch YAML parser

6. [LOW]    BUG-6: Refine NIKA-053 false positive for echo commands
            → Security filter too aggressive

7. [INFO]   BUG-7: Fix 2 example YAML files (${} → inputs:)

8. [INFO]   BUG-8: Agent confidence calculation (cosmetic)
```

---

## KNOWN STATE

```
Branch: main
Tests: 8,914 (8,874 lib + 40 E2E integration)
Clippy: 0 warnings
Providers working: openai ✓, xai ✓, gemini ✓ (rate limited)
Providers broken: anthropic (billing), mistral/groq/deepseek (no keys)
Cargo deny: PASSES (was || true, now hard fail)
Cargo machete: PASSES (was || true, now hard fail)
ProviderName: FULLY MIGRATED (33 files, Option<String> → Option<ProviderName>)
Streaming: 9 sites now log at debug (was silent let _ =)
Daemon: provider list shows env/daemon/keychain source
Docker: VERSION updated 0.40.2 → 0.52.0
```

---

## CONTEXT WINDOW HANDOFF

```bash
claude --model opus -p "$(cat docs/plans/sessions/mega-bug-report-v052-deep-audit.md)"
```
