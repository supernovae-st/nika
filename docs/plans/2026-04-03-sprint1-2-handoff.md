# Sprint 1+2 Handoff — Pipe Transforms & Builtin Tools

> **Date**: 2026-04-03 | **Version**: v0.63.0 | **Branch**: main (pushed)
> **Previous session context**: `~/dev/supernovae/test-jungo/docs/plans/2026-04-03-nika-builtins-killer-features.md`

---

## What Was Done

### Sprint 1 — 10 New Pipe Transforms (nika-core)

**File**: `tools/nika-core/src/binding/transform.rs` (+878 lines)

| Transform | Syntax | Purpose | Tests |
|-----------|--------|---------|-------|
| `pluck(field)` | `\| pluck("name")` | Extract field from array of objects | 5 unit + 1 E2E |
| `where(field, val)` | `\| where("status", "active")` | Filter array by field equality | 4 unit + 1 E2E |
| `pick(f1, f2)` | `\| pick("name", "age")` | Keep only specified fields | 4 unit + 1 E2E |
| `omit(f1, f2)` | `\| omit("password")` | Remove specified fields | 3 unit + 1 E2E |
| `sort_by(field)` | `\| sort_by("age")` | Sort array of objects by field | 4 unit + 1 E2E |
| `group_by(field)` | `\| group_by("locale")` | Group array into object by field | 4 unit + 1 E2E |
| `merge` | `\| merge` | Deep merge array of objects (RFC 7396) | 5 unit + 1 E2E |
| `regex(pattern)` | `\| regex("\\d+")` | Extract first regex match | 5 unit |
| `base64_encode` | `\| base64_encode` | Encode string to base64 | 3 unit + 1 E2E |
| `base64_decode` | `\| base64_decode` | Decode base64 to string | 3 unit + 1 E2E |

**Also added by parallel session** (commit `96b2aeef5`): `starts_with`, `ends_with`, `contains`, `content_hash`, `unique_urls`.

**Total transform count**: ~44 transforms (was 29).

### Sprint 2 — 6 New Builtin Tools (nika-engine)

| Tool | File | Replaces | Tests |
|------|------|----------|-------|
| `nika:json_verify` | `builtin/json_verify.rs` (310 lines) | `verify-translation.py` | 7 unit + 2 E2E |
| `nika:yaml_validate` | `builtin/yaml_validate.rs` (200 lines) | `validate-all-locales.py` | 4 unit + 1 E2E |
| `nika:locale_lookup` | `builtin/locale_lookup.rs` (200 lines) | `nllb-terms.py` | 4 unit + 1 E2E |
| `nika:aggregate` | `builtin/aggregate.rs` (250 lines) | LLM calls for trivial math | 8 unit + 1 E2E |
| `nika:json_flatten` | `builtin/json_transform.rs` (340 lines) | Python scripts | 5 unit + 1 E2E |
| `nika:json_unflatten` | same file | Python scripts | 5 unit + 1 E2E |

### Code Review Fixes (3 commits)

| Issue | Fix |
|-------|-----|
| LSP missing 10 transforms | Added all 10 to completion, test 29→39 |
| Router comment "5" vs 6 | Fixed + updated module docs (7 categories) |
| `aggregate` count semantics | count = total array length (not just numeric) |
| `json_unflatten` collision | Scalar→object on key conflict (was silent drop) |
| 13 new edge case tests | where(bool/numeric), pluck nested, group_by numeric/missing, pick order, stable sort, merge single, regex first-only, base64 empty, aggregate mixed, unflatten collision, flatten arrays |

---

## What Still Needs Doing

### Priority 1 — Verification & Deep Testing

1. **Real provider workflow tests** — Create `.nika.yaml` workflows that use the new transforms AND builtins with REAL LLM providers (anthropic, openai, xai). The mock tests validate transform logic but not the full pipeline with actual LLM output → transform chain.

   Example workflow to create:
   ```yaml
   schema: "nika/workflow@0.12"
   workflow: test-real-providers
   provider: anthropic
   model: claude-sonnet-4-20250514
   tasks:
     - id: generate_data
       infer: "List 5 programming languages with name, year, and paradigm as JSON array"
       structured:
         schema:
           type: object
           properties:
             languages:
               type: array
               items:
                 type: object
                 properties:
                   name: { type: string }
                   year: { type: number }
                   paradigm: { type: string }
                 required: [name, year, paradigm]
     - id: test_transforms
       depends_on: [generate_data]
       with:
         names: $generate_data.languages | pluck("name") | join(", ")
         old: $generate_data.languages | where("paradigm", "functional") | length
         sorted: $generate_data.languages | sort_by("year") | first | pick("name", "year")
       exec:
         command: "echo 'names: {{with.names}}, functional_count: {{with.old}}, oldest: {{with.sorted}}'"
         shell: true
   ```

   Test with: `nika run test-real-providers.nika.yaml`
   Also test with `--provider openai` and `--provider xai` overrides.

2. **Artifact output test** — Create a workflow that uses transforms to prepare data, then writes artifacts:
   ```yaml
   artifact:
     path: "report.json"
     format: json
   ```

3. **for_each + transforms combo** — Test transforms inside for_each loops:
   ```yaml
   for_each: $data | pluck("locale")
   ```

4. **Translation pipeline E2E** — The whole point: create a mini version of the nk-jungo translation workflow using the new builtins instead of Python:
   ```yaml
   tasks:
     - id: source
       exec: 'cat locales/en-US/ui.json'
     - id: translate
       depends_on: [source]
       infer: "Translate this to French: {{with.source}}"
     - id: verify
       depends_on: [source, translate]
       invoke:
         tool: nika:json_verify
         params:
           source: "{{with.source_data}}"
           translation: "{{with.translated}}"
   ```

### Priority 2 — Documentation Updates

5. **Update `~/.claude/rules/nika.md`** — The transform list says "31 available" but there are now ~44. Update the table to include:
   - Data category: `pluck`, `where`, `pick`, `omit`, `sort_by`, `group_by`, `merge`, `regex`
   - Encoding category: `base64_encode`, `base64_decode`
   - Also: `starts_with`, `ends_with`, `contains`, `content_hash`, `unique_urls` (from parallel session)

6. **Update `~/.claude/rules/nika-bugs-and-patterns.md`** — Remove these from "Transforms That DO NOT Exist":
   - `pluck(field)` — IMPLEMENTED
   - `base64_encode` / `base64_decode` — IMPLEMENTED
   - `map(expr)` — covered by `nika:map` builtin tool
   - `zip` — was already `nika:zip` tool
   - `enumerate` — still missing (low priority)
   - `pad(N, char)` — still missing (low priority)

7. **CHANGELOG.md** — Add entry for Sprint 1+2 features.

### Priority 3 — Known Issues to Investigate

8. **`regex()` compiles on every call** — In a `for_each` over 1000 items, the regex pattern is compiled 1000 times. Could pre-compile at parse time into `TransformOp::Regex(Arc<Regex>)`. Low priority unless hot loops are measured.

9. **`flatten_keys` duplication** — Same function exists in `json_verify.rs` and `json_transform.rs` with different signatures. Could extract to shared util.

10. **Parallel session additions** — Commits from parallel agents added:
    - `nika:map`, `nika:filter`, `nika:group_by` (builtin tools)
    - `nika:chunk`, `nika:token_count` (RAG tools)
    - `starts_with()`, `ends_with()`, `contains()`, `content_hash()`, `unique_urls()` (transforms)
    - HTTP cache, robots.txt, cookie infrastructure
    - Verify these are all properly tested and documented.

---

## Key Files Modified

### Core Transform Engine
```
tools/nika-core/src/binding/transform.rs     # Enum + parse + apply + Display + tests
tools/nika-core/Cargo.toml                   # Added base64 dep
```

### Builtin Tools
```
tools/nika-engine/src/runtime/builtin/json_verify.rs      # NEW — translation validator
tools/nika-engine/src/runtime/builtin/yaml_validate.rs    # NEW — batch YAML checker
tools/nika-engine/src/runtime/builtin/locale_lookup.rs    # NEW — BCP-47 mapping
tools/nika-engine/src/runtime/builtin/aggregate.rs        # NEW — array statistics
tools/nika-engine/src/runtime/builtin/json_transform.rs   # NEW — flatten/unflatten
tools/nika-engine/src/runtime/builtin/mod.rs              # Module declarations + exports
tools/nika-engine/src/runtime/builtin/router.rs           # Tool registration + tests
```

### E2E Tests
```
tools/nika-engine/src/runtime/tests_e2e_workflow.rs       # 14 new E2E tests
tests/workflows/test-transforms-mock.nika.yaml            # Workflow test (14 tasks)
tests/workflows/test-builtins-mock.nika.yaml              # Workflow test (12 tasks)
```

### LSP
```
tools/nika-lsp-core/src/handlers/completion.rs            # 39 transforms in autocomplete
```

---

## Commands to Verify

```bash
# Unit tests — transforms
cargo test -p nika-core --lib -- binding::transform
# Expected: 251 passed

# Unit tests — builtins
cargo test -p nika-engine --lib -- "aggregate"
cargo test -p nika-engine --lib -- "json_transform"
cargo test -p nika-engine --lib -- "json_verify"
cargo test -p nika-engine --lib -- "locale_lookup"
cargo test -p nika-engine --lib -- "yaml_validate"

# E2E workflow tests
cargo test -p nika-engine --lib -- "e2e_transform\|e2e_builtin"

# LSP completion
cargo test -p nika-lsp-core --lib -- completion
# Expected: 43 passed (includes 39-transform assertion)

# Full workspace (exclude nika-py — pyo3 linker issue)
cargo test --workspace --lib --exclude nika-py
# Expected: ~9,700 passed, 0 failed

# Live workflow tests (requires installed binary)
nika run tests/workflows/test-transforms-mock.nika.yaml --no-live
# Expected: 14/14 tasks passed

nika run tests/workflows/test-builtins-mock.nika.yaml --no-live
# Expected: 12/12 tasks passed

# Real provider test (requires API keys)
nika run test-real-providers.nika.yaml
nika run test-real-providers.nika.yaml --provider openai
nika run test-real-providers.nika.yaml --provider xai
```

---

## Git State

```
Branch: main (up to date with origin)
Last commit: 1495f1fcf fix(engine): aggregate count returns total items + unflatten handles collisions
Uncommitted: AGENTS.md, vscode extensions, Cargo.lock, some serve files (from parallel sessions)
```

### Session Commits (chronological)
```
ac12d6508 feat(core): add 10 data pipe transforms
627c191d6 feat(engine): add nika:json_verify builtin tool
85442705a feat(engine): add nika:yaml_validate builtin tool
b9bcaacd2 feat(engine): add nika:aggregate builtin tool
4255a93e3 feat(engine): register 6 Sprint 2 builtin tools in router
f7ad6f8b2 test(engine): add 14 E2E tests for Sprint 1+2 features
e6b1f96ba fix(test): correct workflow test files for output:json format
ad319419d fix(engine): update router tool count tests for new data tools
b5fc6dd5f fix(lsp): add 10 new transforms to completion list
8a2a8a1f9 fix(core): update module docs + router comment for new transforms
1495f1fcf fix(engine): aggregate count returns total items + unflatten handles collisions
```

---

## Architecture Notes

### Transform Pattern (nika-core)
Each transform = 4 touch points in `transform.rs`:
1. `TransformOp` enum variant (line ~27-91)
2. `apply()` match arm (line ~252-921)
3. `parse_single_op()` match arm (line ~815-980)
4. `Display` impl (line ~1074-1160)

### Builtin Tool Pattern (nika-engine)
Each tool = 1 file with:
1. Struct implementing `BuiltinTool` trait
2. `name()`, `description()`, `parameters_schema()`, `call()` methods
3. Params struct with `#[derive(Deserialize)]`
4. Registration in `router.rs` → `tools.insert("name", Arc::new(Tool))`
5. Module declaration + re-export in `mod.rs`

### Important: `output: { format: json }` for exec tasks
Exec tasks that produce JSON need `output: { format: json }` to auto-parse. Without it, downstream `$task | pluck(...)` sees a string, not an array. This is by design but catches people in workflow files.
