# Nika E2E Test Suite — Quick Checklist

**Complete feature inventory: 54 testable scenarios** covering schema v0.12 + all documented features.

Created: 2026-03-29 | Full spec: `E2E_TEST_SUITE_COMPREHENSIVE.md`

---

## By Verb (51 core tests)

### Infer (9)
- [x] Short form default model
- [x] Full form + system prompt
- [x] Response format JSON (no validation)
- [x] Structured output with schema + repair
- [x] Extended thinking (Claude)
- [x] Multimodal/vision content
- [x] Guardrails (length, on infer)
- [x] Provider override at task
- [x] Template interpolation

### Exec (8)
- [x] Short form (no shell)
- [x] Full form + shell: true
- [x] Environment variables + $env.* binding
- [x] Working directory (cwd)
- [x] Timeout (seconds)
- [x] Error independence (DAG)
- [x] Template in command
- [x] Security blocklist (rm -rf, sudo)

### Fetch (11)
- [x] Simple GET
- [x] POST + JSON body + headers
- [x] POST + raw string body
- [x] Response full (JSON envelope)
- [x] Extract: markdown
- [x] Extract: article (Readability)
- [x] Extract: jsonpath
- [x] Extract: selector (CSS)
- [x] Extract: metadata (OG/Twitter/JSON-LD)
- [x] Response: binary (CAS storage)
- [x] Timeout + redirect settings

### Invoke (8)
- [x] Builtin short form (nika:tool)
- [x] Builtin full form + timeout
- [x] MCP double colon notation
- [x] MCP explicit mcp: field
- [x] Resource URI syntax
- [x] Timeout override
- [x] With binding from upstream
- [x] Retry at task level (backoff)

### Agent (10)
- [x] Basic with builtin tools
- [x] Completion mode: explicit (must call nika:complete)
- [x] Completion mode: natural (stops on no tools)
- [x] Guardrail: length (min/max words, retry)
- [x] Guardrail: schema (JSON validation, escalate)
- [x] Guardrail: regex pattern (string match, retry)
- [x] Guardrail: LLM judge (quality via LLM, retry)
- [x] Cost limits (max_cost_usd, duration_seconds)
- [x] Token budget + extended thinking
- [x] With binding from upstream

---

## Control Flow & Data (18 tests)

### Ordering & Dependency
- [x] depends_on (ordering only, no data)
- [x] for_each basic (array iteration)
- [x] for_each concurrency
- [x] for_each fail_fast (true/false)
- [x] for_each output is array (requires [0], first, length)
- [x] Diamond DAG merge
- [x] DAG cycle detection (should fail)
- [x] Dependency chain failure (NIKA-026)

### Data Binding & Transforms
- [x] with: simple $task ref
- [x] JSONPath field access ($.nested.field)
- [x] Fallback operator (??)
- [x] $env.VAR_NAME binding
- [x] Pipe transforms: string (upper, lower, trim, length)
- [x] Pipe transforms: array (first, last, reverse, unique, flatten, compact)
- [x] Pipe transforms: type (to_string, to_number, to_bool, to_json, type_of)
- [x] Pipe transforms: numeric (round, abs, ceil, floor)
- [x] Pipe transforms: chained (trim | upper | length)
- [x] Context files ({{context.filename}})

---

## Resilience (4 tests)

- [x] Retry + exponential backoff (max_attempts, delay_ms, backoff)
- [x] Retry on all verbs (exec, infer, fetch, invoke, agent)
- [x] Structured output repair (enable_repair, max_retries, repair_model)
- [x] Max turns graceful stop (not an error)

---

## Output & Artifacts (6 tests)

- [x] Artifact text format
- [x] Artifact JSON format
- [x] Artifact YAML format
- [x] Artifact binary format (media CAS)
- [x] Artifact mode: unique (numbered files) vs overwrite
- [x] Workflow-level artifact config + manifest.json

---

## Security & Error Handling (10 tests)

### Security
- [x] SSRF protection (private IP ranges blocked)
- [x] Exec command blocklist (rm -rf, sudo)
- [x] Template injection prevention (safe interpolation)
- [x] API keys env-only ($env.VAR, never hardcoded)
- [x] Directory traversal in artifacts (../ blocked)

### Errors
- [x] Empty output handling
- [x] Null value guards (??with default())
- [x] Unknown alias detection (NIKA-071)
- [x] Template resolution error (NIKA-041)
- [x] Structured validation (NIKA-300)

---

## Advanced Features (5 tests)

- [x] Provider fallback pattern (multi-provider resilience)
- [x] Skills injection (prompt augmentation)
- [x] Presets (from: agent_name)
- [x] Inputs with defaults ({{inputs.param}})
- [x] Log level control (workflow + task override)

---

## Error Codes Covered (10+)

| Code | Count | Tests |
|------|-------|-------|
| NIKA-010 | 2 | Schema validation |
| NIKA-020 | 1 | DAG cycle |
| NIKA-026 | 1 | Dependency chain |
| NIKA-041 | 1 | Template resolution |
| NIKA-045 | 1 | SSRF blocked |
| NIKA-053 | 2 | Blocked command |
| NIKA-071 | 1 | Unknown alias |
| NIKA-072 | 1 | Null value at path |
| NIKA-112 | 4 | Guardrail violation |
| NIKA-300 | 1 | Structured validation |

---

## Test Execution Tiers

### Tier 1: Smoke (5 tests, <1 min)
Minimal, no API calls needed.
```
1.1 infer_short
2.1 exec_basic
3.1 fetch_get
4.1 invoke_builtin
5.1 agent_basic
```

### Tier 2: Provider-Less (30 tests, ~5 min)
Run with `provider: mock` to skip API calls. Full syntax validation.
```
All verbs, data flow, control flow tests with mock provider.
```

### Tier 3: Full Integration (54 tests, ~30 min)
Real API keys required. Tests all features including vision, repair, structured output.
```
All 54 tests with real providers (anthropic, openai, etc).
```

### Tier 4: Failure Cases (15 tests, ~5 min)
Security, error codes, blocklist, SSRF, schema violations.
```
2.8, 3.10, 10.1-10.5, 12.1-12.5
```

---

## Feature Matrix

| Feature | Verb(s) | Status |
|---------|---------|--------|
| **5 Core Verbs** | infer, exec, fetch, invoke, agent | ✓ Complete |
| **Data Binding** | with, $ref, JSONPath, $env | ✓ Complete |
| **Transforms** | 31 pipe transforms | ✓ Complete |
| **Control Flow** | for_each, depends_on, concurrency | ✓ Complete |
| **Resilience** | retry, backoff, structured repair | ✓ Complete |
| **Output** | artifacts (5 formats), manifest | ✓ Complete |
| **Security** | SSRF, blocklist, injection, traversal | ✓ Complete |
| **Vision** | multimodal infer, CAS hashes | ✓ Complete |
| **Agent Guardrails** | 4 types (length, schema, regex, llm) | ✓ Complete |
| **Provider Fallback** | ?? operator, multi-provider | ✓ Complete |
| **Skills** | Prompt augmentation | ✓ Complete |
| **Presets** | Agent reuse (from:) | ✓ Complete |
| **Context Files** | Load and bind | ✓ Complete |
| **Error Codes** | 10+ NIKA-XXX codes | ✓ Complete |

---

## Implementation Notes

1. **Provider:** Use `mock` for quick tests (no API calls)
2. **API Keys:** `.nika/config.toml` or `$env.API_KEY` (never hardcoded)
3. **Media:** CAS hash binding from `nika:import` for vision tests
4. **Artifacts:** Ensure `./output` writable for artifact tests
5. **MCP:** Mock MCP or configure `mcp:` block for invoke tests
6. **Timeouts:** All in **seconds** (not milliseconds)
7. **Retry:** Task-level field (NOT inside verb block)
8. **for_each:** Output is **array** — requires `[0]`, `first`, `length` access

---

## Key Files

- **Full Spec:** `/docs/plans/E2E_TEST_SUITE_COMPREHENSIVE.md` (1188 lines)
- **This Doc:** `/docs/plans/E2E_TEST_CHECKLIST.md`
- **Examples:** `/examples/gates/` (535 workflows)
- **Specs:** `CLAUDE.md` (user docs), `/rules/nika.md` (schema)

---

**Status:** Ready for implementation
**Last Updated:** 2026-03-29
**Schema:** nika/workflow@0.12
