# Complex Multi-Feature Nika Workflow Tests

10 production-ready test workflows combining 3+ features each.

## Quick Start

```bash
# Validate all workflows
for f in test-workflow-*.nika.yaml; do
  nika check "$f" && echo "✓ $f" || echo "✗ $f"
done

# Run single workflow with mock provider (instant, no API keys)
nika run test-workflow-1-research-pipeline.nika.yaml

# Run with real provider
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml

# Visualize task DAG
nika workflow graph test-workflow-1-research-pipeline.nika.yaml
```

## Workflows Overview

### 1. Research Pipeline (`test-workflow-1-research-pipeline.nika.yaml`)

**Features**: fetch, extract:article, for_each, depends_on, with bindings

Fetch 3 URLs → extract articles → for_each summarize → synthesize report.

```
fetch_urls (for_each, concurrency:3)
    ↓
summarize_all (for_each, structured output)
    ↓
synthesize_report (final synthesis)
```

**Test concepts**:
- Multiple parallel fetch requests
- Array handling with for_each
- Structured JSON output validation
- Data threading with $task_id bindings
- Pipe transforms (join arrays)

**Run with real data**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml \
  --input urls='["https://example.com/article1", "https://example.com/article2"]'
```

---

### 2. Data Extraction (`test-workflow-2-data-extraction.nika.yaml`)

**Features**: infer structured, for_each, fetch, extract:jsonpath, null-safe transforms

Generate API URLs → fetch with JSONPath → aggregate with structured validation.

```
generate_urls (infer + structured)
    ↓
fetch_all (for_each, JSONPath extract)
    ↓
aggregate_data (array transforms)
    ↓
validate_schema (strict schema validation)
```

**Test concepts**:
- Structured output generation (create data from LLM)
- JSONPath extraction from JSON APIs
- Array compacting and uniqueness
- Null-safe transforms (compact, unique, default)
- Type coercion (parse_json, to_json)

**Run with real data**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-2-data-extraction.nika.yaml \
  --input data_source="product-api" \
  --input item_count=10
```

---

### 3. Agent Tools (`test-workflow-3-agent-tools.nika.yaml`)

**Features**: agent, 3 builtin tools, completion:explicit, max_turns

Multi-turn agent using nika:dag_info, nika:task_status, nika:records.

```
load_workflow
    ↓
workflow_inspector (agent with tools)
    ↓
parse_inspection
```

**Test concepts**:
- Agent verb (autonomous loop)
- Builtin nika:* tools (always available)
- Explicit completion mode (agent calls nika:complete)
- Tool selection (tool_choice: auto)
- Max turns limiting
- Guardrails on agent output

**Run with real provider**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-3-agent-tools.nika.yaml \
  --input workflow_file="/path/to/workflow.nika.yaml" \
  --input inspection_depth="detailed"
```

---

### 4. Retry & Fallback (`test-workflow-4-retry-fallback.nika.yaml`)

**Features**: retry, structured repair, provider fallback, max_retries

Robust extraction with exponential backoff and LLM repair.

```
fetch_with_retry (retry: max_attempts:3, backoff:2.0)
    ↓
validate_and_repair (structured + repair_model)
    ↓
process_validated
    ↓
fallback_groq (provider fallback)
    ↓
final_report
```

**Test concepts**:
- Retry logic with exponential backoff
- Structured output validation
- LLM auto-repair on validation failure
- Repair model selection (cheaper model)
- Provider fallback (anthropic → groq)
- Null-safe fallback with default()

**Run with fallback**:
```bash
ANTHROPIC_API_KEY=xxx GROQ_API_KEY=yyy nika run test-workflow-4-retry-fallback.nika.yaml
```

---

### 5. Context Budget (`test-workflow-5-context-budget.nika.yaml`)

**Features**: context_budget, large document handling, token counting, staged processing

Process large documents within token limits.

```
fetch_document
    ↓
analyze_size (estimate tokens)
    ↓
compress_content (context_budget: 4000)
    ↓
stage1_summary (context_budget: 2000)
    ↓
stage2_synthesis (context_budget: 1500)
    ↓
extract_insights (structured)
    ↓
final_report (context_budget enforcement)
```

**Test concepts**:
- Context budget enforcement per-task
- Document compression strategies
- Token counting and estimation
- Multi-stage summarization
- Pipe transforms for content analysis

**Run with custom budget**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-5-context-budget.nika.yaml \
  --input max_context_tokens=2000 \
  --input summary_depth="detailed"
```

---

### 6. Exec + Fetch (`test-workflow-6-exec-fetch.nika.yaml`)

**Features**: exec, for_each, fetch, extract:metadata, pipe transforms

Execute shell → list files → fetch metadata from each → aggregate with transforms.

```
list_files (exec shell command)
    ↓
fetch_metadata (for_each, parallel)
    ↓
aggregate_metadata (transforms: compact, unique, join)
    ↓
generate_stats (structured output)
    ↓
final_summary (complex array transforms)
```

**Test concepts**:
- Exec verb with shell:true pipes
- JSON parsing from command output
- Metadata extraction (OG tags, JSON-LD)
- Complex array transforms (compact, unique, sort)
- Type coercion in transforms
- Array length and first/last access

**Run with real data**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-6-exec-fetch.nika.yaml \
  --input file_directory="./docs" \
  --input metadata_selector="title,description"
```

---

### 7. Multi-Model (`test-workflow-7-multi-model.nika.yaml`)

**Features**: per-task model override, cost optimization, sequential chain

Cost-optimized pipeline: fast haiku → quality sonnet → final haiku.

```
draft_outline (model: claude-haiku-4-5)
    ↓
refine_outline (model: claude-sonnet-4-20250514)
    ↓
generate_section1 (model: claude-sonnet)
    ↓
summarize_section (model: claude-haiku-4-5)
    ↓
final_validation (model: claude-sonnet)
    ↓
format_output (model: claude-haiku-4-5)
```

**Test concepts**:
- Per-task model override (different models per task)
- Cost optimization (haiku 10x cheaper than sonnet)
- Model selection strategy
- Temperature tuning per task type
- Token budget by model capability

**Cost breakdown**:
- 2x haiku: ~$0.02 total
- 3x sonnet: ~$0.09 total
- Estimated savings: 40-50% vs all-sonnet

**Run with cost tracking**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-7-multi-model.nika.yaml \
  --trace  # Shows model usage and costs
```

---

### 8. Guardrails (`test-workflow-8-guardrails.nika.yaml`)

**Features**: 4 guardrail types, on_failure:retry, schema validation, regex

Content generation with strict quality checks.

```
generate_article (guardrail: length)
    ↓
extract_summary (guardrail: schema)
    ↓
generate_outline (guardrail: regex)
    ↓
validate_outline (guardrails: length + regex)
    ↓
final_review (guardrail: llm judge)
```

**Guardrail types**:
1. **Length**: min_words, max_words
2. **Schema**: JSON structure validation
3. **Regex**: Pattern matching
4. **LLM**: Custom judge evaluation

**Test concepts**:
- Multiple guardrails on single task
- on_failure:retry (auto-retry with feedback)
- on_failure:escalate (fail loudly)
- Guardrail composition
- LLM judge for quality validation

**Run with verbose guardrails**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-8-guardrails.nika.yaml --verbose
```

---

### 9. Artifacts (`test-workflow-9-artifacts.nika.yaml`)

**Features**: artifacts, multiple formats, mode:overwrite, artifact manifest

Generate markdown → save → transform to JSON → save → index.

```
generate_report (artifact: .md, mode:unique)
    ↓
enhance_report
    ↓
extract_to_json (artifact: .json, mode:overwrite)
    ↓
create_metadata (artifact: .yaml)
    ↓
generate_index (artifact: source binding)
    ↓
final_summary
```

**Artifact configuration**:
- **Workflow-level**: default format, mode, directory
- **Task-level**: per-task overrides
- **Modes**: overwrite, unique, append, fail
- **Formats**: markdown, json, yaml, text, binary
- **Manifest**: auto-generate artifacts.json index

**Test concepts**:
- Multiple output formats per workflow
- Mode selection (overwrite vs unique)
- Source binding (save upstream task output)
- Artifact index generation
- File organization

**Run and inspect artifacts**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-9-artifacts.nika.yaml
ls -lh ./reports/
cat ./reports/artifacts.json
```

---

### 10. Orchestration (`test-workflow-10-orchestration.nika.yaml`)

**Features**: orchestrate, goal-driven routing, max_rounds, confidence targets

Goal-driven orchestration with adaptive task routing.

```
initial (assess)
    ↓
[confidence < 0.3] → researcher_agent
    ↓
[confidence 0.3-0.7] → analyst_agent
    ↓
[confidence 0.7-0.85] → writer_agent
    ↓
[confidence >= 0.85] → complete
```

**Test concepts**:
- Orchestration goal definition
- Dynamic routing based on conditions
- Confidence scoring (0.0-1.0)
- Max rounds with graceful stop
- Feedback loops (can route back)
- Terminal conditions

**Run with custom targets**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-10-orchestration.nika.yaml \
  --input target_confidence=0.90 \
  --input max_iterations=6
```

---

## Feature Matrix

| Feature | Workflow | Example |
|---------|----------|---------|
| `fetch` | 1, 2, 4, 6 | Get HTTP content |
| `extract` | 1, 2, 6, 7 | Article, JSON, metadata |
| `for_each` | 1, 2, 6 | Parallel processing |
| `infer` | 1, 2, 4, 5, 7, 8, 9, 10 | LLM generation |
| `structured` | 2, 3, 4, 6, 8, 9, 10 | JSON schema validation |
| `agent` | 3, 10 | Multi-turn autonomous |
| `exec` | 6 | Shell commands |
| `retry` | 4 | Exponential backoff |
| `context_budget` | 5 | Token limiting |
| `artifacts` | 9 | File persistence |
| `guardrails` | 8 | Quality constraints |
| `orchestrate` | 10 | Goal-driven routing |
| Model override | 7 | Per-task models |
| Transforms | 1, 2, 5, 6, 7 | Pipe transforms |
| with bindings | 1, 2, 4, 5, 6, 7, 8, 9, 10 | Data threading |
| depends_on | 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 | Task ordering |

---

## Testing Guide

### 1. Syntax Validation

```bash
# Validate all workflows
nika check test-workflow-*.nika.yaml

# Validate with strict MCP checks
nika check test-workflow-*.nika.yaml --strict

# Check specific workflow
nika check test-workflow-1-research-pipeline.nika.yaml --strict
```

### 2. Dry Run (No API Calls)

```bash
# Test workflow logic without calling APIs
nika run test-workflow-1-research-pipeline.nika.yaml --dry-run

# Dry run with mock provider
nika run test-workflow-1-research-pipeline.nika.yaml --provider mock
```

### 3. Live Execution

```bash
# Run with real provider (requires API key)
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml

# Run with output tracing
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml --trace

# Run without live display
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml --no-live
```

### 4. Visualization

```bash
# Visualize task DAG
nika workflow graph test-workflow-1-research-pipeline.nika.yaml

# Export as JSON
nika workflow graph test-workflow-1-research-pipeline.nika.yaml --format json
```

### 5. CI/CD Integration

```bash
# Batch validation for CI
nika check test-workflow-*.nika.yaml || exit 1

# Run all with mock provider (fast, no API keys)
for f in test-workflow-*.nika.yaml; do
  nika run "$f" --provider mock || exit 1
done

echo "All tests passed!"
```

---

## Data Flow Patterns

### Fan-Out / Fan-In

Multiple parallel branches that converge:

```yaml
- id: fan_out
  for_each: { items: "{{inputs.items}}", concurrency: 5 }
  fetch: "{{with.item}}"

- id: fan_in
  depends_on: [fan_out]
  with: { results: $fan_out }
  infer: "Aggregate: {{with.results | join(', ')}}"
```

### Sequential Chain

Tasks execute one after another:

```yaml
- id: step1
  infer: "Initial analysis"

- id: step2
  depends_on: [step1]
  with: { data: $step1 }
  infer: "Refine: {{with.data}}"

- id: step3
  depends_on: [step2]
  with: { refined: $step2 }
  infer: "Final: {{with.refined}}"
```

### Diamond Pattern

Two branches merge, then diverge:

```yaml
- id: start
  infer: "Setup"

- id: left_branch
  depends_on: [start]
  infer: "Process left"

- id: right_branch
  depends_on: [start]
  infer: "Process right"

- id: merge
  depends_on: [left_branch, right_branch]
  with:
    left: $left_branch
    right: $right_branch
  infer: "Combine results"
```

---

## Error Handling

### Retry with Backoff

```yaml
- id: flaky_task
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0
  fetch: "{{inputs.unstable_url}}"
```

### Fallback Values

```yaml
with:
  data: "$upstream_task | default('fallback value')"
infer: "Process: {{with.data}}"
```

### Structured Repair

```yaml
structured:
  schema: { type: object, ... }
  enable_repair: true
  max_retries: 2
  repair_model: claude-haiku-4-5
```

---

## Performance Tips

1. **Use smaller models first**: haiku for drafts, sonnet for quality
2. **Parallel processing**: set `concurrency:` in for_each
3. **Context budgets**: limit large documents with context_budget
4. **Structured output**: enable repair for robustness
5. **Minimal API calls**: use mock provider for testing

---

## Troubleshooting

### Workflow won't validate

```bash
nika check workflow.nika.yaml
# Shows: NIKA-010 (schema error), NIKA-020 (DAG cycle), etc.
# Fix: Check syntax against reference in CLAUDE.md
```

### Task fails with NIKA-071 (unknown alias)

```yaml
# Wrong:
infer: "Data: {{with.data}}"  # 'data' not declared in with:

# Right:
with:
  data: $upstream_task
infer: "Data: {{with.data}}"
```

### Array handling after for_each

```yaml
# Wrong:
with:
  results: $for_each_task
infer: "First: {{with.results.field}}"

# Right:
with:
  results: $for_each_task
infer: "First: {{with.results | first}}"
# Or: {{with.results[0].field}}
```

### Model not found

```bash
# List available models
nika model list

# Use correct model name
model: claude-sonnet-4-20250514  # Not claude-sonnet-4-6
```

---

## Next Steps

1. **Run all validation**: `nika check test-workflow-*.nika.yaml`
2. **Execute with mock**: `nika run test-workflow-1-research-pipeline.nika.yaml`
3. **Try with real provider**: `ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml`
4. **Explore DAGs**: `nika workflow graph test-workflow-*.nika.yaml`
5. **Integrate into CI**: Use test scripts above

---

## Reference

- Full syntax: `/Users/thibaut/.claude/rules/nika.md`
- Workflow format: `.nika.yaml` extension required
- Schema: Always `schema: "nika/workflow@0.12"`
- Error codes: NIKA-XXX format (see rules for details)

