# Nika Test Suite

Comprehensive test workflows covering all Nika v0.19.3 features.

## Summary

- **103 total workflows** (100 test workflows + 3 partials)
- All workflows validated against `nika/workflow@0.12` schema
- Uses `provider: mock` for offline validation

## Categories

### VERBS (25 workflows)

Tests all 5 Nika verbs:

| Directory | Workflows | Features Tested |
|-----------|-----------|-----------------|
| `infer/` | 5 | Shorthand, model override, temperature, system prompt, extended thinking |
| `exec/` | 5 | Shorthand, shell mode, timeout, working directory, complex commands |
| `fetch/` | 5 | GET/POST/PUT/PATCH/DELETE, headers, body, timeout |
| `invoke/` | 5 | Tool calls, parameters, builtin tools, resource reading, chained calls |
| `agent/` | 5 | Basic agent, tools, MCP, stop conditions, nested agents |

### BINDINGS (15 workflows)

Tests data flow and binding patterns:

| File | Feature |
|------|---------|
| `01-with-basic` | Basic `with:` binding from task output |
| `02-with-nested` | Nested object access in bindings |
| `03-with-jsonpath` | JSONPath expressions |
| `04-context-loading` | Context file loading at workflow start |
| `05-inputs` | Workflow inputs binding |
| `06-for-each-basic` | Basic `for_each` iteration |
| `07-for-each-concurrent` | Concurrent `for_each` with concurrency control |
| `08-for-each-binding` | `for_each` with binding reference to array |
| `09-template-resolution` | Multiple `{{with.x}}` template resolution |
| `10-lazy-bindings` | Lazy binding resolution |
| `11-cross-task` | Cross-task data flow in complex DAG |
| `12-multi-source` | Bindings from multiple source types |
| `13-default-values` | Binding with default values |
| `14-validation` | Output validation with schema |
| `15-edge-cases` | Special characters, empty strings, nulls |

### FLOW/DAG (15 workflows)

Tests DAG construction and execution patterns:

| File | Pattern |
|------|---------|
| `01-sequential` | A -> B -> C |
| `02-parallel` | A, B, C (concurrent) |
| `03-diamond` | A -> (B, C) -> D |
| `04-complex-dag` | Multiple paths with joins |
| `05-task-level-flow` | `flow:` property on tasks |
| `06-workflow-level-flows` | `flows:` section |
| `07-mixed-flows` | Both task and workflow level |
| `08-conditional-flow` | Conditional execution |
| `09-error-handling` | Error propagation |
| `10-retry-logic` | Retry with validation |
| `11-timeout` | Task timeout handling |
| `12-cancellation` | `fail_fast` cancellation |
| `13-dependencies` | Complex dependency resolution |
| `14-fan-out` | One to many |
| `15-fan-in` | Many to one |

### INCLUDE/PKG (18 workflows)

Tests DAG fusion and include patterns (15 tests + 3 partials):

| File | Feature |
|------|---------|
| `01-basic-include` | Single include |
| `02-nested-include` | Multiple includes with prefixes |
| `03-dag-fusion` | Flow connections to included tasks |
| `04-partial-tasks` | Reference prefixed task IDs |
| `05-lib-import` | Library-style imports |
| `06-pkg-structure` | `pkg:` URI syntax |
| `07-context-merge` | Context merging with includes |
| `08-skill-include` | Skills propagation |
| `09-multi-file` | Multiple file includes |
| `10-circular-detection` | Non-circular validation |
| `11-relative-paths` | Relative path resolution |
| `12-absolute-paths` | Path handling |
| `13-remote-include` | Remote include syntax |
| `14-version-pin` | Version pinning |
| `15-override` | Task override patterns |

### OUTPUT/ARTIFACTS (10 workflows)

Tests output handling and artifacts:

| File | Feature |
|------|---------|
| `01-json-output` | JSON format with validation |
| `02-text-output` | Plain text format |
| `03-file-write` | Output save to file |
| `04-artifact-basic` | Basic artifact configuration |
| `05-artifact-conditional` | Conditional artifact saving |
| `06-artifact-multiple` | Multiple artifacts per task |
| `07-log-config` | Logging configuration |
| `08-structured-output` | Complex JSON schema validation |
| `09-streaming` | Streaming output handling |
| `10-manifest` | Artifact manifest generation |

### MCP (10 workflows)

Tests MCP server integration:

| File | Feature |
|------|---------|
| `01-tool-call-basic` | Basic MCP tool call |
| `02-resource-read` | MCP resource reading |
| `03-multi-server` | Multiple MCP servers |
| `04-novanet-integration` | NovaNet MCP pattern |
| `05-params-templating` | Template resolution in params |
| `06-error-handling` | MCP error handling |
| `07-timeout` | MCP timeout handling |
| `08-retry` | MCP retry behavior |
| `09-batch-calls` | Batch MCP calls |
| `10-agent-with-mcp` | Agent with MCP access |

### ADVANCED (10 workflows)

Tests advanced patterns and features:

| File | Feature |
|------|---------|
| `01-multi-provider` | Multiple providers in workflow |
| `02-provider-fallback` | Provider fallback behavior |
| `03-extended-thinking` | Extended thinking feature |
| `04-skill-merging` | Skills through includes |
| `05-workflow-composition` | Complex composition |
| `06-saga-pattern` | Saga with compensation |
| `07-graceful-shutdown` | Graceful shutdown handling |
| `08-observability` | Logging and events |
| `09-security` | Security features |
| `10-performance` | Performance patterns |

## Running Tests

### Validate All Workflows

```bash
# Using Python + jsonschema
cd tools/nika
python3 -c "
import yaml, json
from pathlib import Path
from jsonschema import validate

schema = json.load(open('schemas/nika-workflow.schema.json'))
for f in Path('examples/test-suite').rglob('*.nika.yaml'):
    validate(yaml.safe_load(open(f)), schema)
    print(f'OK: {f}')
"
```

### Run Individual Test

```bash
# Dry-run validation
nika check examples/test-suite/infer/01-basic.nika.yaml

# Execute (requires API keys for non-mock provider)
nika run examples/test-suite/exec/01-basic.nika.yaml
```

## Schema Version

All workflows use `nika/workflow@0.12` schema.

## Provider

All workflows use `provider: mock` for offline validation. Change to `claude`, `openai`, etc. for live testing.
