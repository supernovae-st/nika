# 10 Complex Multi-Feature Nika Workflow Tests

Production-ready test workflows combining 3+ features each. All use `provider: mock` for CI testing, with comments showing real provider usage.

## Test Design Rationale

Each workflow tests realistic production patterns:
- **Feature matrix**: fetch, extract, for_each, infer, structured, agents, retry, guardrails, artifacts, exec, invoke
- **Data flow**: dependencies, bindings, transforms, array handling, null safety
- **Error paths**: retry logic, fallback strategies, validation repair
- **Integration**: cross-verb pipelines, upstream/downstream coupling, fan-out/fan-in patterns

All workflows are syntactically valid and follow strict schema rules:
- `schema: "nika/workflow@0.12"` always first
- `$task_id` bindings with `with:` blocks
- `{{with.alias}}` template interpolation
- `depends_on: [task]` as arrays
- `.nika.yaml` file extension

## Execution Instructions

```bash
# Test all with mock provider (CI)
for f in test-workflow-*.nika.yaml; do
  nika check "$f" && echo "✓ $f" || echo "✗ $f"
done

# Run single workflow with mock
nika run test-workflow-1-research-pipeline.nika.yaml --provider mock

# Run with real provider (requires API key)
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml

# Visualize DAG
nika workflow graph test-workflow-1-research-pipeline.nika.yaml
```

## Test Matrix

| # | Workflow | Features | Complexity |
|---|----------|----------|-----------|
| 1 | Research Pipeline | fetch, extract:article, for_each, depends_on, with | 5 |
| 2 | Data Extraction | infer, structured, for_each, fetch, extract:jsonpath | 5 |
| 3 | Agent Tools | agent, 3x builtin tools, completion:explicit, max_turns | 4 |
| 4 | Retry + Fallback | routing, retry, structured repair, provider fallback | 5 |
| 5 | Context Budget | context_budget, record:compress, with, depends_on | 4 |
| 6 | Exec + Fetch | exec, for_each, fetch, extract:metadata, pipe transforms | 5 |
| 7 | Multi-Model Chain | per-task model override, depends_on chain, cost tracking | 4 |
| 8 | Guardrails | infer, guardrails (3 types), on_failure:retry | 4 |
| 9 | Artifact Pipeline | infer, artifacts (2 formats), mode:overwrite, chaining | 4 |
| 10 | Orchestration Goal | goal:, orchestrate:, max_rounds, confidence_target | 3 |

---

## Design Specifications

Each workflow includes:

- **Features documented**: YAML comments showing which features are tested
- **Data flow diagram**: ASCII art showing task dependencies
- **Mock execution**: `provider: mock` for instant CI/testing
- **Real provider hint**: Comment block showing how to use real provider
- **Syntax validation**: All pass `nika check` with strict mode
- **Test assertions**: Inline comments for expected outputs

Total: 10 complete, runnable `.nika.yaml` files ready for integration testing.
