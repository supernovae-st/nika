# Nika Complex Workflows - Complete Index

A comprehensive suite of 10 production-ready workflow tests demonstrating realistic feature combinations in Nika v0.12.

## Quick Navigation

- **Start Here**: [WORKFLOW_SUMMARY.txt](WORKFLOW_SUMMARY.txt) - Executive summary
- **User Guide**: [TEST_WORKFLOWS_README.md](TEST_WORKFLOWS_README.md) - Complete documentation
- **Architecture**: [COMPLEX_WORKFLOWS_DESIGN_NOTES.md](COMPLEX_WORKFLOWS_DESIGN_NOTES.md) - Design decisions
- **Validation**: `./validate-test-workflows.sh` - Automated testing script

## The 10 Workflows

All files are in the current directory (`/Users/thibaut/dev/supernovae/nika/`):

### 1. Research Pipeline
**File**: `test-workflow-1-research-pipeline.nika.yaml` (303 lines)

Multi-URL research: fetch 3 URLs → extract articles → for_each summarize → synthesize report.

**Features**: fetch, extract:article, for_each, depends_on, with bindings

**Pattern**: Fan-out/fan-in (parallel processing)

**Key Concepts**:
- Multiple parallel fetch requests
- Array iteration with for_each
- Structured JSON output validation
- Data threading with $task_id bindings

**Run**:
```bash
nika run test-workflow-1-research-pipeline.nika.yaml --provider mock
```

---

### 2. Data Extraction
**File**: `test-workflow-2-data-extraction.nika.yaml` (268 lines)

API endpoint generation → fetch with JSONPath → aggregate results.

**Features**: infer structured, for_each, fetch, extract:jsonpath, null-safe transforms

**Pattern**: Sequential with parallel processing

**Key Concepts**:
- Structured output generation from LLM
- JSONPath extraction from JSON APIs
- Array compacting and uniqueness
- Type coercion in transforms

**Run**:
```bash
nika run test-workflow-2-data-extraction.nika.yaml --input item_count=5
```

---

### 3. Agent Tools
**File**: `test-workflow-3-agent-tools.nika.yaml` (201 lines)

Multi-turn agent using 3 builtin tools (nika:dag_info, nika:task_status, nika:records).

**Features**: agent, builtin tools, completion:explicit, max_turns, tool_choice:auto

**Pattern**: Sequential chain

**Key Concepts**:
- Agent verb (autonomous loop)
- Builtin nika:* tools (always available)
- Explicit completion mode
- Multi-turn tool invocation

**Run**:
```bash
nika run test-workflow-3-agent-tools.nika.yaml --input workflow_file="./example.nika.yaml"
```

---

### 4. Retry & Fallback
**File**: `test-workflow-4-retry-fallback.nika.yaml` (297 lines)

Robust extraction with exponential backoff and LLM repair.

**Features**: retry (exponential backoff), structured repair, repair_model selection, provider fallback

**Pattern**: Sequential with recovery

**Key Concepts**:
- Retry logic with backoff (1s → 2s → 4s)
- Structured output validation and LLM repair
- Cheaper model for validation retries
- Provider fallback (anthropic → groq)

**Run**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-4-retry-fallback.nika.yaml
```

---

### 5. Context Budget
**File**: `test-workflow-5-context-budget.nika.yaml` (321 lines)

Large document processing with token budgeting across 6 stages.

**Features**: context_budget, document compression, token limiting, staged processing

**Pattern**: Staged pipeline (6 sequential tasks)

**Key Concepts**:
- Context budget enforcement per-task
- Document compression strategies
- Token counting and estimation
- Multi-stage summarization

**Run**:
```bash
nika run test-workflow-5-context-budget.nika.yaml --input max_context_tokens=2000
```

---

### 6. Exec + Fetch
**File**: `test-workflow-6-exec-fetch.nika.yaml` (289 lines)

Execute shell → list files → fetch metadata from each → aggregate with transforms.

**Features**: exec, for_each, fetch, extract:metadata, pipe transforms

**Pattern**: Command → parallel → aggregate

**Key Concepts**:
- Exec verb with shell piping
- JSON parsing from command output
- Metadata extraction (OG tags)
- Complex array transforms

**Run**:
```bash
nika run test-workflow-6-exec-fetch.nika.yaml --input file_directory="./docs"
```

---

### 7. Multi-Model
**File**: `test-workflow-7-multi-model.nika.yaml` (361 lines)

Cost-optimized pipeline: fast haiku draft → quality sonnet → final haiku format.

**Features**: per-task model override, cost optimization, sequential chain

**Pattern**: Sequential with model selection

**Key Concepts**:
- Per-task model override (different models per task)
- Cost optimization (haiku 10x cheaper)
- Model selection strategy
- Temperature tuning by task type

**Cost Savings**: 40-50% vs all-sonnet

**Run**:
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-7-multi-model.nika.yaml --trace
```

---

### 8. Guardrails
**File**: `test-workflow-8-guardrails.nika.yaml` (351 lines)

Content generation with 4 types of guardrails: length, schema, regex, LLM judge.

**Features**: guardrails (4 types), on_failure:retry, schema validation, regex patterns

**Pattern**: Sequential with quality gates

**Key Concepts**:
- Length guardrail (min/max words)
- Schema validation (JSON structure)
- Regex pattern matching
- LLM judge for quality
- Automatic retry on failure

**Run**:
```bash
nika run test-workflow-8-guardrails.nika.yaml --verbose
```

---

### 9. Artifacts
**File**: `test-workflow-9-artifacts.nika.yaml` (352 lines)

Multi-format output: markdown → JSON → YAML with artifact persistence.

**Features**: artifacts, multiple formats, mode:overwrite/unique, artifact manifest

**Pattern**: Multi-output with persistence

**Key Concepts**:
- Multiple output formats per workflow
- Mode selection (overwrite vs unique)
- Source binding (save upstream task output)
- Artifact index generation (artifacts.json)

**Run**:
```bash
nika run test-workflow-9-artifacts.nika.yaml && ls -lh ./reports/
```

---

### 10. Orchestration
**File**: `test-workflow-10-orchestration.nika.yaml` (327 lines)

Goal-driven orchestration with adaptive task routing based on confidence.

**Features**: orchestrate, goal-driven routing, confidence scoring, max_rounds

**Pattern**: Conditional routing with feedback

**Key Concepts**:
- Orchestration goal definition
- Dynamic routing based on conditions
- Confidence scoring (0.0-1.0)
- Max rounds with graceful stop
- Feedback loops

**Run**:
```bash
nika run test-workflow-10-orchestration.nika.yaml --input target_confidence=0.90
```

---

## Documentation Files

### 1. TEST_WORKFLOWS_README.md (725 lines)
**Comprehensive user guide covering**:
- Quick start instructions
- Detailed workflow descriptions (with data flow diagrams)
- Feature matrix
- Testing guide (syntax, dry-run, live execution, visualization)
- CI/CD integration examples
- Data flow patterns (fan-out/fan-in, sequential, diamond)
- Error handling strategies
- Performance tips
- Troubleshooting guide
- Command reference

**Read this for**: Implementation guidance, how to run workflows, CI/CD setup

### 2. COMPLEX_WORKFLOWS_DESIGN_NOTES.md (450+ lines)
**Architecture and design documentation covering**:
- Design philosophy (feature orthogonality, data flow patterns, feature combinations)
- Feature coverage analysis (verbs, extract modes, advanced features)
- Production readiness checklist
- Execution environments (mock vs real providers)
- Error scenarios tested
- Cost optimization strategies
- Testing coverage matrix
- Known limitations and design decisions
- Future extensions
- File structure

**Read this for**: Understanding design decisions, testing strategy, production readiness

### 3. docs/plans/sessions/complex-workflows-multi-feature-tests.md
**High-level overview** with:
- Test design rationale
- Workflow matrix
- Execution instructions
- Design specifications

**Read this for**: Quick overview before diving deeper

### 4. WORKFLOW_SUMMARY.txt (This file)
**Executive summary** with:
- Deliverables checklist
- Workflow matrix overview
- Feature coverage summary
- Testing phases
- Data flow patterns
- Error handling strategies
- Cost optimization
- Production readiness checklist
- Execution commands
- Quick next steps

**Read this for**: Quick reference, command cheatsheet

---

## Validation & Testing

### validate-test-workflows.sh (415 lines)
Automated test suite with 6 validation phases:

1. **Syntax Validation**: nika check on all workflows
2. **Strict Validation**: MCP checks, dependency validation
3. **Feature Analysis**: Feature matrix mapping
4. **Dependency Analysis**: Task ordering verification
5. **Dry-Run Tests**: Mock provider execution
6. **DAG Visualization**: Graph generation

**Run**:
```bash
chmod +x validate-test-workflows.sh
./validate-test-workflows.sh
```

**Output**: Comprehensive test report with statistics

---

## Quick Commands

### Validation (30 seconds)
```bash
./validate-test-workflows.sh                          # Full suite
nika check test-workflow-1-research-pipeline.nika.yaml # Single workflow
```

### Dry-Run (1-2 minutes, no API calls)
```bash
nika run test-workflow-1-research-pipeline.nika.yaml --provider mock
for f in test-workflow-*.nika.yaml; do nika run "$f" --provider mock || exit 1; done
```

### Live Execution (requires API key)
```bash
ANTHROPIC_API_KEY=xxx nika run test-workflow-1-research-pipeline.nika.yaml
ANTHROPIC_API_KEY=xxx GROQ_API_KEY=yyy nika run test-workflow-4-retry-fallback.nika.yaml
```

### Visualization
```bash
nika workflow graph test-workflow-1-research-pipeline.nika.yaml
nika workflow graph test-workflow-*.nika.yaml        # All workflows
```

### CI/CD Integration
```bash
for f in test-workflow-*.nika.yaml; do nika check "$f" || exit 1; done
for f in test-workflow-*.nika.yaml; do nika run "$f" --provider mock || exit 1; done
```

---

## File Locations

All files in: `/Users/thibaut/dev/supernovae/nika/`

```
nika/
├── test-workflow-1-research-pipeline.nika.yaml      (303 lines)
├── test-workflow-2-data-extraction.nika.yaml        (268 lines)
├── test-workflow-3-agent-tools.nika.yaml            (201 lines)
├── test-workflow-4-retry-fallback.nika.yaml         (297 lines)
├── test-workflow-5-context-budget.nika.yaml         (321 lines)
├── test-workflow-6-exec-fetch.nika.yaml             (289 lines)
├── test-workflow-7-multi-model.nika.yaml            (361 lines)
├── test-workflow-8-guardrails.nika.yaml             (351 lines)
├── test-workflow-9-artifacts.nika.yaml              (352 lines)
├── test-workflow-10-orchestration.nika.yaml         (327 lines)
├── TEST_WORKFLOWS_README.md                         (725 lines)
├── COMPLEX_WORKFLOWS_DESIGN_NOTES.md                (450+ lines)
├── COMPLEX_WORKFLOWS_INDEX.md                       (this file)
├── WORKFLOW_SUMMARY.txt                             (comprehensive summary)
├── validate-test-workflows.sh                       (415 lines, executable)
└── docs/plans/sessions/
    └── complex-workflows-multi-feature-tests.md     (overview)
```

**Total**: 2,966 lines workflows + 1,500+ lines documentation = 4,500+ production code

---

## Feature Coverage Matrix

| Feature | Count | Workflows |
|---------|-------|-----------|
| infer | 8 | 1,2,4,5,7,8,9,10 |
| fetch | 4 | 1,2,4,6 |
| exec | 1 | 6 |
| agent | 2 | 3,10 |
| invoke | 1 | 3 |
| for_each | 3 | 1,2,6 |
| structured | 7 | 2,3,4,6,8,9,10 |
| retry | 1 | 4 |
| context_budget | 1 | 5 |
| artifacts | 1 | 9 |
| guardrails | 1 | 8 |
| orchestrate | 1 | 10 |
| extract (4 modes) | 4 | 1,2,6 |

**Total unique features**: 13 core features + variants
**Total feature uses**: 30+ across 10 workflows

---

## Data Flow Patterns

### Sequential Chain (3 workflows)
Task1 → Task2 → Task3 → Task4

Examples: Workflows 3, 5, 7

### Fan-Out / Fan-In (2 workflows)
Task1 → [parallel branch] → Task2 (aggregate)

Examples: Workflows 1, 2, 6

### Conditional Routing (2 workflows)
Task1 → [if condition A] → Task2a OR Task2b

Examples: Workflows 4, 10

### Tree Structure (3 workflows)
Task1 → [multiple dependencies] → Task2

Examples: Workflows 6, 8, 9

---

## Getting Started (5 Minutes)

1. **Read the summary** (2 min):
   ```bash
   cat WORKFLOW_SUMMARY.txt
   ```

2. **Run validation** (2 min):
   ```bash
   ./validate-test-workflows.sh
   ```

3. **Execute one workflow** (1 min):
   ```bash
   nika run test-workflow-1-research-pipeline.nika.yaml --provider mock
   ```

4. **Next**:
   - Read TEST_WORKFLOWS_README.md for detailed usage
   - Check COMPLEX_WORKFLOWS_DESIGN_NOTES.md for architecture
   - Run more workflows with real providers (requires API keys)

---

## References

- Nika Schema: `/Users/thibaut/.claude/rules/nika.md`
- Project Instructions: `/Users/thibaut/dev/supernovae/nika/CLAUDE.md`
- Architecture Rules: `/Users/thibaut/dev/supernovae/dx/.claude/rules/architecture.md`

---

## Support

All workflows are self-contained and documented. Each .nika.yaml file includes:
- Description of what the workflow does
- Features being tested
- Expected data flow
- Run instructions (with and without real providers)
- Input examples

For questions about specific features, see:
- Individual workflow comments
- TEST_WORKFLOWS_README.md (comprehensive guide)
- COMPLEX_WORKFLOWS_DESIGN_NOTES.md (architecture)

---

**Last Updated**: 2026-03-30
**Schema**: nika/workflow@0.12
**Provider**: mock (default), plus real providers documented

