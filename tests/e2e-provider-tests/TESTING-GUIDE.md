# E2E Provider Testing Guide

Complete walkthrough for running, understanding, and extending Nika provider E2E tests.

## Quick Start

### Option 1: Run Mock Tests (Instant, No API Keys)
```bash
cd <project-root>
nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```

Expected output: All tasks pass in <10 seconds, deterministic results.

### Option 2: Run All Cloud Providers (Requires API Keys)
```bash
# Set environment variables
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GEMINI_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export MISTRAL_API_KEY="..."
export DEEPSEEK_API_KEY="sk-..."
export XAI_API_KEY="..."

# Run tests sequentially
for test in tests/e2e-provider-tests/{01,02,03,04,05,06,07}.nika.yaml; do
  echo "Testing: $test"
  nika run "$test"
  echo "---"
done
```

### Option 3: Validate Without Running
```bash
# Check syntax and DAG validity
nika check tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml

# Dry-run (parse, validate, don't execute)
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml --dry-run
```

## Test Anatomy

Each workflow follows this structure:

### 1. Header Configuration
```yaml
schema: nika/workflow@0.12
workflow: test-name
description: "What this tests"

provider: anthropic              # Default provider
model: claude-sonnet-4-20250514      # Default model

inputs:                          # Configurable inputs
  param1: "value1"
  param2: "value2"
```

### 2. Task Sequence
```yaml
tasks:
  # Phase 1: Setup/baseline
  - id: baseline_inference
    infer: { prompt: "..." }

  # Phase 2: Provider-specific feature
  - id: extended_thinking
    infer:
      prompt: "..."
      extended_thinking: true
      thinking_budget: 8000

  # Phase 3: Error handling
  - id: error_test
    infer: { prompt: "..." }
    retry:
      max_attempts: 1

  # Phase 4: Verification/summary
  - id: verify
    depends_on: [baseline_inference, extended_thinking, error_test]
    with:
      result1: $baseline_inference
      result2: $extended_thinking
    exec: { command: "echo {{with.result1}}" }
```

### 3. Verification Pattern
```yaml
- id: final_summary
  depends_on: [all_previous_tasks]
  with:
    # Bind results from previous tasks
    result: $task_name | transform1 | transform2
  exec:
    command: |
      echo "Task completed successfully"
      echo "Result: {{with.result}}"
    shell: true
```

## Key Features Tested

### 1. Anthropic (Extended Thinking)
**File:** `01-anthropic-extended-thinking.nika.yaml`

Tests:
- Extended thinking with configurable thinking budget
- Cache read tokens (second identical prompt uses less)
- Structured output with tool injection
- Thinking token consumption

Unique to Anthropic:
```yaml
infer:
  extended_thinking: true
  thinking_budget: 8000      # Tokens for Claude to think before responding
```

### 2. OpenAI (Native JSON)
**File:** `02-openai-json-response.nika.yaml`

Tests:
- Native JSON response format (OpenAI-specific)
- Nested schema validation
- Multiple response format modes
- Cost comparison (mini vs full models)

Unique to OpenAI:
```yaml
infer:
  response_format: json      # Native OpenAI feature for guaranteed JSON
```

### 3. Gemini (Stop Sequences)
**File:** `03-gemini-stop-sequences.nika.yaml`

Tests:
- Single stop sequence control
- Multiple stop sequences
- Stop sequence precedence
- Long-form generation with natural stops

Unique to Gemini:
```yaml
infer:
  stop_sequences: ["STOP", "END", "---"]  # Generation stops at first match
```

### 4. Groq (Ultra-Fast)
**File:** `04-groq-ultra-fast-inference.nika.yaml`

Tests:
- Latency measurement (expect <500ms)
- Token counting accuracy
- Consistency across identical queries
- Llama 3.3 specific capabilities

Special focus:
- Measures actual response time
- Verifies token counts are accurate
- Tests response consistency (determinism)

### 5. Mistral (Standard + Features)
**File:** `05-mistral-standard-inference.nika.yaml`

Tests:
- Basic inference
- Code generation
- Temperature control (0.0 = deterministic)
- Retry and repair mechanisms
- Cost-efficiency analysis

Standard pattern:
```yaml
infer:
  temperature: 0.3      # Lower = more deterministic
  max_tokens: 500
```

### 6. DeepSeek (Reasoning + Error)
**File:** `06-deepseek-reasoning.nika.yaml`

Tests:
- Math/logic problem solving
- Multiple approach comparison
- Reasoning-focused tasks
- **CRITICAL: Vision rejection** (should fail gracefully)

Vision rejection test:
```yaml
- id: deepseek_vision_attempt
  infer: { prompt: "..." }
  # Expected: VisionNotSupported error (correct behavior)
```

### 7. xAI Grok (Personality)
**File:** `07-xai-grok-inference.nika.yaml`

Tests:
- Personality and humor in responses
- Multi-turn conversation capability
- Reasoning quality
- Speed comparison with other models

Special property: Grok has recognizable personality/wit in responses.

### 8. Native Local GGUF (Zero Cost)
**File:** `08-native-local-gguf.nika.yaml`

Tests:
- Local GGUF model inference (no API)
- **Text-only verification** (vision unavailable)
- **Vision rejection** (should error)
- Privacy/cost benefits
- Offline capability

Special requirements:
```bash
export NIKA_NATIVE_MODEL_PATH="/path/to/model.gguf"
nika run tests/e2e-provider-tests/08-native-local-gguf.nika.yaml
```

### 9. Mock Provider (Deterministic)
**File:** `09-mock-provider-deterministic.nika.yaml`

Tests:
- Deterministic identical responses
- All features simulated (vision, extended thinking, etc.)
- Zero cost, instant execution
- Perfect for CI/CD testing

Perfect for:
- Development without API keys
- Testing workflow syntax
- CI/CD pipelines
- Performance baseline

## Template Language Features

Each test demonstrates Nika's template syntax:

### Basic Binding
```yaml
with:
  simple: $previous_task
```
Result: Full output of previous task

### Field Access
```yaml
with:
  field: $previous_task.metadata.name
```
Result: Nested field from previous task

### Transforms
```yaml
with:
  # String transforms
  upper: $data | upper
  trimmed: $data | trim

  # Array transforms
  first: $list | first
  count: $list | length
  unique_items: $list | unique

  # Combined
  cleaned: $data | trim | lower | first(50)
```

Available transforms: 31 total
- String: upper, lower, trim, trim_start, trim_end, length, to_string
- Array: first, last, flatten, reverse, sort, unique, compact, keys, values
- Numeric: round, abs, ceil, floor, to_number
- Type: to_bool, to_json, parse_json, type_of
- Parametric: join(","), split(","), default("fallback")
- Utility: shell (escape for shell), type_of

### Fallback Operator
```yaml
with:
  value: $data.nested.field ?? "default_value"
```
Result: Uses default if path doesn't exist

### Conditional/Complex
```yaml
with:
  # JSON conversion
  as_json: $previous_task | to_json

  # First N characters
  preview: $previous_task | first(100)

  # Multiple transforms
  result: $data | trim | upper | join(",")
```

## Structured Output (Schema Validation)

All tests demonstrate schema validation:

```yaml
infer:
  prompt: "Return structured data"
  response_format: json

structured:
  schema:
    type: object
    properties:
      field1: { type: string }
      field2: { type: number, minimum: 0, maximum: 100 }
      items: {
        type: array,
        items: { type: string },
        minItems: 1
      }
    required: [field1]

  enable_repair: true    # LLM auto-repairs invalid JSON
  max_retries: 2         # Attempts before giving up
```

Features:
- Schema validation (NIKA-300 error if fails)
- Automatic repair (LLM fixes malformed JSON)
- Retry with feedback
- Type checking and constraints

## Error Handling Patterns

### Retry Mechanism
```yaml
- id: task_with_retry
  infer: { prompt: "..." }
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0  # Exponential: 1s, 2s, 4s
```

### Vision Rejection (DeepSeek)
```yaml
- id: deepseek_vision_test
  infer: { prompt: "..." }
  retry:
    max_attempts: 1  # Fail fast for expected error
  # Catch VisionNotSupported error gracefully
```

### Timeout Handling
```yaml
- id: task_with_timeout
  infer: { prompt: "..." }
  # Default timeout per provider
  # Anthropic: generous for thinking
  # Groq: ultra-fast expected
```

## DAG Patterns

### Sequential Chain
```yaml
tasks:
  - id: step1
    infer: { prompt: "..." }

  - id: step2
    depends_on: [step1]
    with: { data: $step1 }
    infer: { prompt: "Process: {{with.data}}" }
```

### Parallel (simulated)
```yaml
tasks:
  - id: parallel_1
    infer: { prompt: "Query 1" }

  - id: parallel_2
    infer: { prompt: "Query 2" }

  - id: parallel_3
    infer: { prompt: "Query 3" }

  - id: merge
    depends_on: [parallel_1, parallel_2, parallel_3]
    with:
      r1: $parallel_1
      r2: $parallel_2
      r3: $parallel_3
    infer: { prompt: "Merge: {{with.r1}} {{with.r2}} {{with.r3}}" }
```

### For_Each Loop
```yaml
- id: generate_list
  infer: { prompt: "Return array JSON" }
  output: { format: json }

- id: process_each
  depends_on: [generate_list]
  for_each:
    items: $generate_list
    as: item
    concurrency: 3
    fail_fast: false
  infer: { prompt: "Process: {{with.item}}" }
```

## Metrics and Analysis

Each test collects:

1. **Response Quality**
   - Schema validation success/failure
   - Repair attempts needed
   - Final output correctness

2. **Performance**
   - Total execution time
   - Per-task latency
   - Token efficiency (input/output ratio)

3. **Behavior**
   - Determinism (identical inputs = identical outputs?)
   - Consistency (responses stable across runs?)
   - Edge cases (how does it handle errors?)

4. **Cost**
   - Estimated tokens used
   - Estimated cost
   - Cost per task

## Custom Modifications

### Change Provider
```yaml
provider: openai      # Instead of anthropic
model: gpt-4o         # Instead of claude-sonnet-4
```

### Adjust Inputs
```bash
# Command line override
nika run workflow.nika.yaml -i problem="Your problem here"

# Or edit workflow inputs:
inputs:
  problem: "New problem"
```

### Add New Tasks
```yaml
- id: custom_task
  depends_on: [previous_task]
  with:
    data: $previous_task
  infer:
    prompt: |
      Custom prompt using {{with.data}}
    max_tokens: 500
  description: "Custom test task"
```

### Change Schema
```yaml
structured:
  schema:
    type: object
    properties:
      your_field: { type: string }
    required: [your_field]
```

## Debugging

### Enable Verbose Logging
```bash
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml --verbose
```

### Check Task Execution
```bash
# View each task's output
nika run workflow.nika.yaml --show-all-output
```

### View Intermediate Results
```yaml
- id: debug_task
  depends_on: [previous]
  with:
    data: $previous
  exec:
    command: |
      echo "Debug output:"
      echo "{{with.data}}"
    shell: true
```

### Validate Workflow Only
```bash
nika check workflow.nika.yaml --strict
```

## Performance Baselines

Expected performance per provider (single inference call):

| Provider | Model | Latency | Cost | Tokens |
|----------|-------|---------|------|--------|
| Anthropic | claude-sonnet-4 | 800-1200ms | ~$0.01 | 100-200 |
| OpenAI | gpt-4.1-mini | 500-1000ms | ~$0.001 | 50-100 |
| Gemini | gemini-2.5-flash | 300-700ms | ~$0.001 | 40-80 |
| Groq | llama-3.3-70b | 100-300ms | ~$0.001 | 50-100 |
| Mistral | mistral-small | 300-600ms | ~$0.0001 | 30-60 |
| DeepSeek | deepseek-chat | 400-800ms | ~$0.001 | 40-80 |
| xAI | grok-3 | 600-1000ms | ~$0.01 | 80-150 |
| Native | TinyLlama-1.1B | <50ms | $0 | 20-40 |
| Mock | mock | ~1ms | $0 | ~50 |

## Continuous Integration

### GitHub Actions Example
```yaml
name: E2E Provider Tests

on: [push, pull_request]

jobs:
  test-mock:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Nika
        run: |
          curl -fsSL https://installer.supernovae.studio/nika.sh | bash

      - name: Test Mock Provider
        run: |
          cd nika
          nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml

  test-cloud:
    runs-on: ubuntu-latest
    if: secrets.ANTHROPIC_API_KEY != ''
    steps:
      - uses: actions/checkout@v3
      - name: Install Nika
        run: curl -fsSL https://installer.supernovae.studio/nika.sh | bash

      - name: Test Anthropic
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          cd nika
          nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml
```

## Troubleshooting

### "Provider not available"
```
Error: Provider anthropic not found or API key missing
```
Solution:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
nika provider list  # Verify key is detected
```

### "Model not found"
```
Error: Model claude-sonnet-4-20250514 not recognized
```
Solution:
```bash
nika model list  # See available models
# Update workflow to use available model
```

### "Vision not supported"
```
Error: VisionNotSupported (DeepSeek)
```
Expected behavior - this is correct! Tests validate error handling.

### "Timeout exceeded"
```
Error: Request timeout after 30s
```
Solutions:
- Check network connectivity
- Increase timeout: `timeout: 60`
- Try a different provider
- Use mock provider for testing

### "Structured output failed"
```
Error: NIKA-300 Structured output validation failed
```
Debug steps:
1. Check schema is valid JSON Schema
2. Verify `enable_repair: true` is set
3. Increase `max_retries`
4. Check model's JSON capability

## Next Steps

1. Run mock test first: `09-mock-provider-deterministic.nika.yaml`
2. Try native test if GGUF available: `08-native-local-gguf.nika.yaml`
3. Test one cloud provider with API key
4. Compare outputs and performance metrics
5. Create custom workflows based on patterns

## References

- [Nika CLI](../../CLAUDE.md)
- [Schema Reference](../../CLAUDE.md#workflow-syntax)
- [Error Codes](../../CLAUDE.md#error-codes)
- [Provider Catalog](../../tools/nika-core/src/catalogs/providers.rs)
- [Cost Tables](../../tools/nika-core/src/catalogs/cost.rs)
