# E2E Provider Tests for Nika

Comprehensive workflow tests exercising each LLM provider's unique capabilities and edge cases.

## Overview

9 complete `.nika.yaml` workflows, each testing provider-specific features, error handling, and integration patterns.

| Provider | Workflow | Model(s) | Key Features Tested |
|----------|----------|---------|-------------------|
| **Anthropic** | `01-anthropic-extended-thinking.nika.yaml` | claude-sonnet-4-20250514 | Extended thinking, cache read tokens, structured output with tool injection |
| **OpenAI** | `02-openai-json-response.nika.yaml` | gpt-4.1-mini | Native JSON response format, schema validation, nested objects |
| **Gemini** | `03-gemini-stop-sequences.nika.yaml` | gemini-2.5-flash | Stop sequences (single + multiple), structured output, long-form generation |
| **Groq** | `04-groq-ultra-fast-inference.nika.yaml` | llama-3.3-70b-versatile | Ultra-low latency, token counting accuracy, parallel simulation |
| **Mistral** | `05-mistral-standard-inference.nika.yaml` | mistral-small-latest | Standard inference, code generation, temperature control, retry/repair |
| **DeepSeek** | `06-deepseek-reasoning.nika.yaml` | deepseek-chat | Reasoning tokens, problem-solving approaches, **vision rejection (error handling)** |
| **xAI** | `07-xai-grok-inference.nika.yaml` | grok-3 | Personality/humor, reasoning, multi-turn capability, speed |
| **Native** | `08-native-local-gguf.nika.yaml` | TinyLlama-1.1B | Local GGUF inference (no API), text-only, **vision unavailable (error)**, zero cost |
| **Mock** | `09-mock-provider-deterministic.nika.yaml` | mock | Deterministic responses, all features work, zero cost, perfect for testing |

## Running the Tests

### Run All E2E Tests (with API keys)

```bash
cd <project-root>

# Test individual providers
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml
nika run tests/e2e-provider-tests/02-openai-json-response.nika.yaml
nika run tests/e2e-provider-tests/03-gemini-stop-sequences.nika.yaml
nika run tests/e2e-provider-tests/04-groq-ultra-fast-inference.nika.yaml
nika run tests/e2e-provider-tests/05-mistral-standard-inference.nika.yaml
nika run tests/e2e-provider-tests/06-deepseek-reasoning.nika.yaml
nika run tests/e2e-provider-tests/07-xai-grok-inference.nika.yaml
```

### Run Tests Without API Calls (Mock & Native Only)

```bash
# Mock provider - instant, deterministic, no API
nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml

# Native - requires GGUF model path
export NIKA_NATIVE_MODEL_PATH=/path/to/model.gguf
nika run tests/e2e-provider-tests/08-native-local-gguf.nika.yaml
```

### Validate Workflows Without Running

```bash
# Check all workflows for syntax/DAG errors
for file in tests/e2e-provider-tests/*.nika.yaml; do
  nika check "$file" || echo "ERROR in $file"
done
```

### Dry-Run Mode (validate without execution)

```bash
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml --dry-run
```

## Test Architecture

### Each Workflow Tests

1. **Basic Inference** - Core provider functionality
2. **Provider-Specific Feature** - Extended thinking, stop sequences, JSON format, etc.
3. **Structured Output** - Schema validation with repair
4. **Retry/Resilience** - Error handling and recovery
5. **Edge Cases** - Error paths (vision on text-only, etc.)
6. **Performance Baseline** - Speed/latency measurement
7. **Integration Pattern** - Multi-task workflows (for_each, DAG dependencies)
8. **Summary Report** - Verification of all capabilities

### Key Test Patterns

#### Template Binding & Transforms
```yaml
with:
  data: $previous_task
  clean: $previous_task | trim | upper
  first_100: $previous_task | first(100)
```

#### Structured Output with Schema Validation
```yaml
structured:
  schema:
    type: object
    properties:
      field1: { type: string }
      field2: { type: number, minimum: 0 }
    required: [field1]
  enable_repair: true
  max_retries: 2
```

#### Error Handling (DeepSeek Vision)
```yaml
- id: vision_test
  infer: { prompt: "..." }
  retry:
    max_attempts: 1
  # Expected: VisionNotSupported error, caught gracefully
```

#### For_Each Loop (parallel-like execution)
```yaml
- id: iterate
  for_each:
    items: $previous_task
    as: item
    concurrency: 3
  infer: { prompt: "Process: {{with.item}}" }
```

## Provider Capabilities Matrix

| Feature | Anthropic | OpenAI | Gemini | Groq | Mistral | DeepSeek | xAI | Native | Mock |
|---------|-----------|--------|--------|------|---------|----------|-----|--------|------|
| Basic Inference | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Extended Thinking | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ (sim) |
| Cache Tokens | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| JSON Response | ✓ | ✓ (native) | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ (sim) |
| Structured Output | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ (sim) |
| Stop Sequences | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ (sim) |
| Vision | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ (sim) |
| Retry/Repair | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Zero Cost | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ | ✓ |
| No API Key | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ (optional) | ✓ |

## Cost Estimates (for testing)

Approximate cost for running each E2E test once (1-2 minutes per test):

| Provider | Model | Est. Cost | Notes |
|----------|-------|-----------|-------|
| Anthropic | claude-sonnet-4-20250514 | ~$0.05 | Cache tokens may reduce cost |
| OpenAI | gpt-4.1-mini | ~$0.02 | Cheapest per test |
| Gemini | gemini-2.5-flash | ~$0.01 | Flash model is fast, cheap |
| Groq | llama-3.3-70b-versatile | ~$0.01 | Ultra-fast, low cost |
| Mistral | mistral-small-latest | ~$0.001 | Cheapest option |
| DeepSeek | deepseek-chat | ~$0.01 | Competitive pricing |
| xAI | grok-3 | ~$0.05 | Mid-range cost |
| Native | TinyLlama-1.1B | $0.00 | Local, free |
| Mock | mock | $0.00 | Instant, free |

**Total for all 9 tests**: ~$0.16 (if all API keys available)

## Error Code Coverage

Tests validate handling of these NIKA error codes:

| Code | Scenario | Test |
|------|----------|------|
| NIKA-035 | Custom endpoint not found | Custom endpoints (advanced) |
| NIKA-036 | Cannot connect to endpoint | Network resilience |
| NIKA-300 | Structured output validation failed | All structured tests |
| NIKA-301 | Schema mismatch (after repair failed) | Repair mechanism tests |
| Vision error | DeepSeek vision unsupported | 06-deepseek-reasoning |
| Text-only error | Native GGUF vision unsupported | 08-native-local-gguf |

## Environment Variables Required

```bash
# Cloud providers (only needed for their respective tests)
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GEMINI_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export MISTRAL_API_KEY="..."
export DEEPSEEK_API_KEY="sk-..."
export XAI_API_KEY="..."

# Local/mock providers (optional, not required)
export NIKA_NATIVE_MODEL_PATH="/path/to/model.gguf"  # Optional for native test
# Mock provider needs no environment variables
```

## CI/CD Integration

### Test in GitHub Actions (mock only - no API keys)

```yaml
- name: Run Mock Provider E2E Tests
  run: |
    cd nika
    nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```

### Test with API Keys (local or private CI)

```bash
# Create a test matrix
for provider in anthropic openai gemini groq mistral deepseek xai; do
  nika run "tests/e2e-provider-tests/*${provider}*.nika.yaml" || echo "FAILED: $provider"
done
```

## Troubleshooting

### API Key Not Found
```bash
# Verify environment variable is set
echo $ANTHROPIC_API_KEY

# Set it temporarily for testing
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Model Not Available
```bash
# Check available models
nika model list

# Some providers have model variants:
# Anthropic: claude-opus-4, claude-sonnet-4, claude-haiku-4-5
# OpenAI: gpt-4o, gpt-4.1, gpt-4.1-mini, o1, o3-mini
# Gemini: gemini-2.5-flash, gemini-2.5-pro, gemini-2.0-flash
```

### Vision Not Supported
```bash
# Expected for:
# - DeepSeek (all models)
# - Native GGUF (unless VisionHf model)

# Error: VisionNotSupported
# This is normal and validates error handling
```

### Timeout on Groq/Anthropic Tests
```bash
# These tests measure latency - ensure network is stable
# Expected latency: <1s per inference
# If >10s, likely network issue

# Increase timeout if needed:
# timeout: 60  # seconds
```

## Future Extensions

Planned additional test coverage:

1. **Custom Endpoints** - Test vLLM/TGI/Ollama compatibility
2. **Streaming Responses** - Verify streaming output with for_each
3. **Multi-Modal** - Vision workflows with image inputs
4. **Vision Cost Analysis** - Token usage for vision tasks
5. **MCP Integration** - Invoke external tools alongside inference
6. **Agent Workflows** - Multi-turn agent loops
7. **Cost Tracking** - Detailed billing per provider
8. **Latency Comparison** - SLA verification across providers
9. **Fallback Chains** - Provider fallback when primary unavailable

## Notes

- All workflows use `.nika.yaml` extension (required)
- Each test is independent and can run in isolation
- Tests produce deterministic output for CI/CD integration
- Mock provider tests run instantly (no external calls)
- Native GGUF tests require model file download (~1-2 GB)
- Cost estimates are approximate and vary by region/pricing changes

## References

- [Nika Schema Reference](../../CLAUDE.md#5-verbs)
- [Provider Catalog](../../tools/nika-core/src/catalogs/providers.rs)
- [Cost Tables](../../tools/nika-core/src/catalogs/cost.rs)
- [Vision Support](../../CLAUDE.md#vision-support-since-v0340)
- [Error Codes](../../CLAUDE.md#error-codes)
