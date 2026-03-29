# E2E Provider Tests - Complete Index

## Overview
Complete end-to-end workflow tests for all 9 LLM providers supported by Nika.

**Total files**: 11 files (9 workflows + 2 guides + 1 index)
**Total lines**: ~2,400 lines of YAML + documentation
**Cost to run**: ~$0.16 for all cloud tests (or $0 with mock/native)
**Time to run**: 2-5 minutes per test, 15-30 minutes for all

## Files

### Test Workflows (9 files, 49.8 KB)

| File | Size | Provider | Model | Key Feature | Status |
|------|------|----------|-------|------------|--------|
| `01-anthropic-extended-thinking.nika.yaml` | 3.5 KB | Anthropic | claude-sonnet-4-20250514 | Extended thinking + cache tokens | ✓ Complete |
| `02-openai-json-response.nika.yaml` | 5.5 KB | OpenAI | gpt-4.1-mini | Native JSON response format | ✓ Complete |
| `03-gemini-stop-sequences.nika.yaml` | 5.3 KB | Gemini | gemini-2.5-flash | Stop sequences (single + multiple) | ✓ Complete |
| `04-groq-ultra-fast-inference.nika.yaml` | 6.2 KB | Groq | llama-3.3-70b-versatile | Ultra-low latency inference | ✓ Complete |
| `05-mistral-standard-inference.nika.yaml` | 6.5 KB | Mistral | mistral-small-latest | Standard + code generation | ✓ Complete |
| `06-deepseek-reasoning.nika.yaml` | 7.8 KB | DeepSeek | deepseek-chat | Reasoning + vision rejection | ✓ Complete |
| `07-xai-grok-inference.nika.yaml` | 7.0 KB | xAI | grok-3 | Personality + multi-turn | ✓ Complete |
| `08-native-local-gguf.nika.yaml` | 8.1 KB | Native | TinyLlama-1.1B | Local GGUF (zero cost, text-only) | ✓ Complete |
| `09-mock-provider-deterministic.nika.yaml` | 9.4 KB | Mock | mock | Deterministic, all features | ✓ Complete |

### Documentation (2 files, 24 KB)

| File | Size | Purpose |
|------|------|---------|
| `README.md` | 9.8 KB | Quick reference, capabilities matrix, environment setup |
| `TESTING-GUIDE.md` | 14 KB | Detailed walkthrough, patterns, troubleshooting |
| `INDEX.md` | This file | Master index and navigation |

## Quick Start

### Run Instantly (No API Keys)
```bash
nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```
Expected: All 15 tasks pass in <10 seconds

### Validate All Workflows
```bash
for f in tests/e2e-provider-tests/*.nika.yaml; do
  echo "Checking $f..."
  nika check "$f" || exit 1
done
```

### Run All Tests (With API Keys)
```bash
# Set environment variables
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
# ... etc for each provider

# Run tests
for test in tests/e2e-provider-tests/{01,02,03,04,05,06,07,08}.nika.yaml; do
  nika run "$test"
done
```

## Test Coverage Matrix

### Providers (9 total)
- [x] Anthropic (claude-sonnet-4-20250514)
- [x] OpenAI (gpt-4.1-mini)
- [x] Gemini (gemini-2.5-flash)
- [x] Groq (llama-3.3-70b-versatile)
- [x] Mistral (mistral-small-latest)
- [x] DeepSeek (deepseek-chat)
- [x] xAI (grok-3)
- [x] Native (TinyLlama-1.1B)
- [x] Mock (deterministic)

### Features Tested (30+ total)

#### By Provider
- **Anthropic**: Extended thinking, cache tokens, structured output, tool injection
- **OpenAI**: JSON response format, nested schemas, cost comparison
- **Gemini**: Stop sequences (1x, 3x, 5x), long-form generation
- **Groq**: Latency measurement, token accuracy, parallel simulation
- **Mistral**: Code generation, temperature control, retry/repair
- **DeepSeek**: Multi-approach solving, reasoning, vision rejection
- **xAI**: Personality, multi-turn, reasoning, speed
- **Native**: Text-only, vision rejection, zero cost, privacy
- **Mock**: Determinism, all features, CI/CD ready

#### By Feature Category
- Basic inference (9/9)
- Structured output + repair (9/9)
- Retry mechanisms (9/9)
- Error handling (9/9)
- Schema validation (9/9)
- Template transforms (9/9)
- For_each loops (partial)
- Vision support (tested + rejection cases)
- Cost analysis (partial)

## Architecture

### Test Pattern (per workflow)
1. **Baseline** - Basic inference
2. **Provider Feature** - Unique capability
3. **Validation** - Structured output
4. **Error Cases** - Edge cases (vision rejection, etc.)
5. **Summary** - Verification report

### DAG Pattern (per workflow)
```
Baseline → Feature Test → Structured Output
    ↓
  Retry/Error Cases
    ↓
  Verification
    ↓
  Summary Report
```

### Task Count per Workflow
- `01-anthropic`: 6 tasks + 1 summary = 7 total
- `02-openai`: 5 tasks + 1 summary = 6 total
- `03-gemini`: 8 tasks + 1 summary = 9 total
- `04-groq`: 8 tasks + 1 summary = 9 total
- `05-mistral`: 9 tasks + 1 summary = 10 total
- `06-deepseek`: 11 tasks + 1 summary = 12 total
- `07-xai`: 11 tasks + 1 summary = 12 total
- `08-native`: 11 tasks + 1 summary = 12 total
- `09-mock`: 15 tasks + 1 summary = 16 total
- **Total**: 89 tasks across all workflows

## Key Test Insights

### What Each Test Validates

#### 01: Anthropic Extended Thinking
```yaml
# Tests these capabilities:
extended_thinking: true        # ✓ 8000 token thinking budget
thinking_budget: 8000          # ✓ Configurable thinking tokens
cache tokens                   # ✓ Second identical prompt uses cache
structured output              # ✓ Schema validation
tool injection                 # ✓ Structured with tool definitions
```

#### 02: OpenAI JSON Response
```yaml
# Tests these capabilities:
response_format: json          # ✓ Native OpenAI feature
nested schemas                 # ✓ Complex object validation
integer constraints            # ✓ min/max on numbers
array validation              # ✓ minItems/maxItems
```

#### 03: Gemini Stop Sequences
```yaml
# Tests these capabilities:
stop_sequences: ["STOP"]       # ✓ Single stop sequence
stop_sequences: ["A", "B", "C"] # ✓ Multiple sequences
long-form generation          # ✓ Creative writing with stops
```

#### 04: Groq Ultra-Fast
```yaml
# Tests these capabilities:
latency measurement           # ✓ Expect <500ms
token counting               # ✓ Accuracy verification
determinism                 # ✓ Identical responses
consistency                 # ✓ Across multiple runs
```

#### 05: Mistral Standard
```yaml
# Tests these capabilities:
temperature: 0.0             # ✓ Deterministic responses
temperature: 0.8             # ✓ Creative responses
code generation             # ✓ Syntax highlighting ready
retry mechanisms            # ✓ max_attempts with backoff
```

#### 06: DeepSeek Reasoning
```yaml
# Tests these capabilities:
problem solving             # ✓ Math/logic
multiple approaches         # ✓ Compare strategies
vision rejection            # ✓ VisionNotSupported error (expected)
reasoner model              # ✓ deepseek-reasoner variant available
```

#### 07: xAI Grok
```yaml
# Tests these capabilities:
personality                 # ✓ Humor/sarcasm in responses
temperature: 0.8           # ✓ Creative mode
multi-turn                 # ✓ Follow-up questions
reasoning                  # ✓ Complex problems
```

#### 08: Native GGUF
```yaml
# Tests these capabilities:
no API key                 # ✓ NIKA_NATIVE_MODEL_PATH only
text-only                  # ✓ Confirmed in tests
vision rejection           # ✓ No vision support (text-only error)
zero cost                  # ✓ No per-token billing
privacy                    # ✓ Model runs locally
```

#### 09: Mock Provider
```yaml
# Tests these capabilities:
determinism                # ✓ Identical inputs = identical outputs
all features simulated    # ✓ Even vision/thinking
zero cost                 # ✓ No API calls
instant execution         # ✓ <1ms per call
CI/CD ready              # ✓ Reliable for automation
```

## Environment Setup

### Required (Cloud Providers)
```bash
export ANTHROPIC_API_KEY="sk-ant-..."       # For test 01
export OPENAI_API_KEY="sk-..."              # For test 02
export GEMINI_API_KEY="..."                 # For test 03
export GROQ_API_KEY="gsk_..."               # For test 04
export MISTRAL_API_KEY="..."                # For test 05
export DEEPSEEK_API_KEY="sk-..."            # For test 06
export XAI_API_KEY="..."                    # For test 07
```

### Optional (Local/Mock)
```bash
export NIKA_NATIVE_MODEL_PATH="/path/to/model.gguf"  # For test 08
# Mock provider (test 09) needs NO environment variables
```

## File Organization

```
tests/e2e-provider-tests/
├── 01-anthropic-extended-thinking.nika.yaml    (3.5 KB)
├── 02-openai-json-response.nika.yaml           (5.5 KB)
├── 03-gemini-stop-sequences.nika.yaml          (5.3 KB)
├── 04-groq-ultra-fast-inference.nika.yaml      (6.2 KB)
├── 05-mistral-standard-inference.nika.yaml     (6.5 KB)
├── 06-deepseek-reasoning.nika.yaml             (7.8 KB)
├── 07-xai-grok-inference.nika.yaml             (7.0 KB)
├── 08-native-local-gguf.nika.yaml              (8.1 KB)
├── 09-mock-provider-deterministic.nika.yaml    (9.4 KB)
├── README.md                                   (9.8 KB)  ← Start here
├── TESTING-GUIDE.md                            (14 KB)   ← Deep dive
└── INDEX.md                                    (This file)
```

## Quick Reference

### By Use Case

**I want to test without API keys**
→ Run `09-mock-provider-deterministic.nika.yaml`

**I have one API key (e.g., OpenAI)**
→ Run `02-openai-json-response.nika.yaml`

**I want the fastest provider**
→ Run `04-groq-ultra-fast-inference.nika.yaml`

**I want the cheapest option**
→ Run `05-mistral-standard-inference.nika.yaml` or `09-mock`

**I want privacy (no API calls)**
→ Run `08-native-local-gguf.nika.yaml` (requires GGUF)

**I want to develop features**
→ Run `09-mock-provider-deterministic.nika.yaml` + your custom workflow

**I want CI/CD testing**
→ Add `09-mock` to your CI pipeline (no secrets needed)

### By Feature

**Extended Thinking / Deep Reasoning**
→ `01-anthropic-extended-thinking.nika.yaml`

**Structured JSON Output**
→ `02-openai-json-response.nika.yaml` or `03-gemini-stop-sequences.nika.yaml`

**Ultra-Fast Inference**
→ `04-groq-ultra-fast-inference.nika.yaml`

**Code Generation**
→ `05-mistral-standard-inference.nika.yaml`

**Local/Offline**
→ `08-native-local-gguf.nika.yaml`

**Error Handling (Vision Rejection)**
→ `06-deepseek-reasoning.nika.yaml` (negative test)

## Performance Baselines

Expected metrics when running each test:

| Test | Execution Time | Total Tokens | Est. Cost |
|------|----------------|--------------|-----------|
| 01-Anthropic | 5-8 minutes | 2-3K | ~$0.05 |
| 02-OpenAI | 2-3 minutes | 1-2K | ~$0.02 |
| 03-Gemini | 2-4 minutes | 1-2K | ~$0.01 |
| 04-Groq | 1-2 minutes | 800-1200 | ~$0.01 |
| 05-Mistral | 2-3 minutes | 1-2K | ~$0.001 |
| 06-DeepSeek | 2-3 minutes | 1-2K | ~$0.01 |
| 07-xAI | 3-4 minutes | 2-3K | ~$0.05 |
| 08-Native | <10 seconds | N/A | $0.00 |
| 09-Mock | <10 seconds | N/A | $0.00 |

## Extending the Tests

To add a new provider test:

1. **Copy template workflow**
   ```bash
   cp tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml \
      tests/e2e-provider-tests/10-newprovider-feature.nika.yaml
   ```

2. **Update header**
   ```yaml
   provider: newprovider
   model: model-name
   workflow: newprovider-feature-test
   description: "Test description"
   ```

3. **Add provider-specific tests** (see guides)

4. **Validate**
   ```bash
   nika check tests/e2e-provider-tests/10-newprovider-feature.nika.yaml
   ```

5. **Update README.md** - Add to provider matrix

## Documentation Navigation

- **Quick Start**: Start in [README.md](README.md)
- **Deep Dive**: Read [TESTING-GUIDE.md](TESTING-GUIDE.md)
- **Test Details**: Read workflow YAML directly (well-commented)
- **Provider Info**: See comments in each workflow

## Validation Checklist

Before running tests:

- [ ] API keys set correctly: `echo $ANTHROPIC_API_KEY`
- [ ] Nika installed: `nika --version`
- [ ] Network available: `ping api.anthropic.com`
- [ ] Sufficient credits/quota on each provider account
- [ ] Read [README.md](README.md) for any provider-specific setup

Before committing changes:

- [ ] All workflows validate: `nika check *.nika.yaml`
- [ ] Mock test passes: `nika run 09-*.nika.yaml`
- [ ] Documentation updated
- [ ] No hardcoded API keys
- [ ] All comments are clear

## Support & Troubleshooting

See [TESTING-GUIDE.md - Troubleshooting](TESTING-GUIDE.md#troubleshooting) for:
- Provider not available
- Model not found
- Vision not supported
- Timeout issues
- Schema validation failures

## Links

- [Nika Documentation](../../CLAUDE.md)
- [Provider Catalog](../../tools/nika-core/src/catalogs/providers.rs)
- [Cost Tables](../../tools/nika-core/src/catalogs/cost.rs)
- [Schema Reference](../../CLAUDE.md#workflow-syntax)
- [Error Codes](../../CLAUDE.md#error-codes)

---

**Created**: 2026-03-30
**Version**: 1.0
**Status**: Complete and Ready for Testing
