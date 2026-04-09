# nika-builtin

Extracted builtin tools for the Nika workflow engine. L2 crate, ~10k LOC, ~264 tests.

Created in Constellation Phase 12 (Sessions 6-9). Contains 37/63 builtin tools.
Session 9 added 3 introspection tools (task_status, records, orchestrate) via
RecordQuery trait in nika-kernel.

## Architecture

### Sealed Trait Pattern

Tools implement `nika_kernel::BuiltinTool` (returns `BuiltinError`), NOT the engine's
`BuiltinTool` trait (returns `NikaError`). The engine bridges them via `KernelToolAdapter<T>`:

```
nika-kernel::BuiltinTool (BuiltinError)
        |
  KernelToolAdapter<T>  (in nika-engine, converts errors)
        |
nika-engine::BuiltinTool (NikaError)
```

The kernel trait is sealed via `#[doc(hidden)] pub mod __sealed` to prevent external
implementations. All tools must live in nika-builtin or nika-engine.

### BuiltinError (8 variants)

Defined in `nika-kernel`. Maps to NikaError codes in the engine adapter:

| Variant | NikaError Code |
|---------|---------------|
| InvalidArgs | NIKA-212 (BuiltinInvalidParams) |
| Io | NIKA-210 (BuiltinToolError) |
| Parse | NIKA-210 (BuiltinToolError) |
| Timeout | NIKA-210 (BuiltinToolError) |
| Schema | NIKA-210 (BuiltinToolError) |
| Denied | NIKA-380 (CapabilityDenied) |
| AssertionFailed | NIKA-213 (AssertionFailed) |
| Other | NIKA-210 (BuiltinToolError) |

### 4 Kernel Traits (consumed by tools)

In `nika-kernel`, used by migrated tools:

- `RunExecutor` — for nika:run (nested workflow execution)
- `HitlPrompt` — for nika:prompt (human-in-the-loop)
- `MediaContext` — for media tools (CAS access)
- `RecordQuery` — for introspection tools (record access via RecordView DTO)

## Tools (37)

### Core (7)
| Tool | File | Purpose |
|------|------|---------|
| `nika:sleep` | `sleep.rs` | Pause execution for N seconds |
| `nika:log` | `log.rs` | Structured logging (debug/info/warn/error) |
| `nika:emit` | `emit.rs` | Emit custom events to EventLog |
| `nika:assert` | `assert.rs` | Runtime assertions with structured conditions |
| `nika:complete` | `complete.rs` | Signal agent completion |
| `nika:prompt` | `prompt.rs` | Human-in-the-loop prompting (via HitlPrompt trait) |
| `nika:run` | `run_tool.rs` | Nested workflow execution (via RunExecutor trait) |

### File (5)
| Tool | File | Purpose |
|------|------|---------|
| `nika:read` | `file/read.rs` | Read file contents |
| `nika:write` | `file/write.rs` | Write file contents |
| `nika:edit` | `file/edit.rs` | Edit file (search/replace) |
| `nika:glob` | `file/glob.rs` | Glob pattern file matching |
| `nika:grep` | `file/grep.rs` | Search file contents |

### Data (13)
| Tool | File | Purpose |
|------|------|---------|
| `nika:jq` | `data/jq.rs` | Full jq expressions via jaq-core |
| `nika:map` | `data/transform.rs` | Transform array elements |
| `nika:filter` | `data/transform.rs` | Filter array by condition |
| `nika:group_by` | `data/transform.rs` | Group array into object by field |
| `nika:chunk` | `data/transform.rs` | Split array into fixed-size chunks |
| `nika:token_count` | `data/text.rs` | Count tokens (tiktoken) |
| `nika:enrich` | `data/text.rs` | Enrich data with computed fields |
| `nika:zip` | `data/merge.rs` | Zip two arrays into pairs |
| `nika:set_diff` | `data/merge.rs` | Set difference between arrays |
| `nika:json_merge` | `data/merge.rs` | Deep merge JSON objects |
| `nika:json_diff` | `data/json_diff.rs` | Diff two JSON values (RFC 6902) |
| `nika:tree_data` | `data/aggregate.rs` | Build tree from flat data |
| `nika:inject` | `data/io.rs` | Inject computed values into objects |

### Data Sprint 2 (6)
| Tool | File | Purpose |
|------|------|---------|
| `nika:json_verify` | `json_verify.rs` | Verify translated JSON structure against source |
| `nika:yaml_validate` | `yaml_validate.rs` | Validate YAML structure |
| `nika:locale_lookup` | `locale_lookup.rs` | i18n locale resolution |
| `nika:aggregate` | `aggregate.rs` | Statistical aggregation (sum, avg, min, max) |
| `nika:json_flatten` | `json_transform.rs` | Flatten nested JSON to dot-notation |
| `nika:json_unflatten` | `json_transform.rs` | Unflatten dot-notation to nested JSON |

### Introspection (6)
| Tool | File | Purpose |
|------|------|---------|
| `nika:cost` | `cost.rs` | Query token cost for current run |
| `nika:dag_info` | `introspect_dag.rs` | DAG structure and task metadata |
| `nika:threads` | `introspect_threads.rs` | Active thread/task information |
| `nika:task_status` | `introspect_task_status.rs` | Task status + metrics (via RecordQuery) |
| `nika:records` | `introspect_records.rs` | Query compressed records (via RecordQuery) |
| `nika:orchestrate` | `introspect_orchestrate.rs` | Aggregate workflow stats (via RecordQuery) |

## Testing

```bash
cargo test -p nika-builtin --lib        # ~250+ tests, safe (no keychain)
```

All tools have unit tests validating:
- Input schema validation (missing/invalid params -> BuiltinError::InvalidArgs)
- Happy path with programmatic output validation
- Edge cases (empty arrays, null values, invalid types)
- Security: path traversal rejection in io.rs, input sanitization

## Dependencies

- `nika-kernel` (L0.5) — BuiltinTool trait, BuiltinError
- `nika-core` (L0) — Types only (Value, RunStats, DagInfo)
- `serde_json` — JSON manipulation
- `jaq-core` + `jaq-std` — jq implementation (for JqTool)
- `tiktoken-rs` — Token counting (for TokenCountTool)
- `serde_json` — JSON manipulation (JsonVerifyTool uses structural comparison, not JSON Schema)

- `nika-event` (L1) — EventLog for introspection tools (cost, dag_info, threads)
- `rustc-hash` — FxHashMap/FxHashSet for fast lookups

Does NOT depend on: nika-engine, nika-media, nika-mcp.
