# E2E Provider Tests - Quick Start (2 minutes)

Complete end-to-end testing suite for all 9 LLM providers in Nika. Everything you need to validate provider capabilities.

## What's Inside

9 complete `.nika.yaml` workflows testing:
- Anthropic (extended thinking + cache tokens)
- OpenAI (native JSON response format)
- Gemini (stop sequences)
- Groq (ultra-fast inference)
- Mistral (standard + code generation)
- DeepSeek (reasoning + vision rejection)
- xAI (personality + multi-turn)
- Native (local GGUF, zero cost)
- Mock (deterministic, CI-ready)

## 30-Second Test

Run this right now (no API keys needed):

```bash
cd <project-root>
nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```

Expected: ✓ 16 tasks pass in <10 seconds, zero API calls

## With API Keys (Pick One)

Have an API key? Test that provider:

```bash
# Fastest & cheapest (test takes 2-3 min, costs ~$0.001)
export MISTRAL_API_KEY="your-key"
nika run tests/e2e-provider-tests/05-mistral-standard-inference.nika.yaml

# Or Groq (ultra-fast, ~1min, ~$0.01)
export GROQ_API_KEY="your-key"
nika run tests/e2e-provider-tests/04-groq-ultra-fast-inference.nika.yaml

# Or Anthropic (extended thinking, ~5min, ~$0.05)
export ANTHROPIC_API_KEY="your-key"
nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml

# Or OpenAI (JSON format, ~2min, ~$0.02)
export OPENAI_API_KEY="your-key"
nika run tests/e2e-provider-tests/02-openai-json-response.nika.yaml
```

## All 9 Tests (If You Have All Keys)

```bash
# Set all environment variables
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GEMINI_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export MISTRAL_API_KEY="..."
export DEEPSEEK_API_KEY="sk-..."
export XAI_API_KEY="..."

# Run all tests (15-30 minutes total, ~$0.16 total cost)
for test in tests/e2e-provider-tests/{01,02,03,04,05,06,07,08,09}.nika.yaml; do
  echo "Running: $(basename $test)"
  nika run "$test" || echo "FAILED"
done
```

## What Each Test Does

| Test | Time | Cost | What It Tests |
|------|------|------|---------------|
| 01-Anthropic | 5-8m | $0.05 | Extended thinking, cache tokens, structured output |
| 02-OpenAI | 2-3m | $0.02 | Native JSON format, nested schemas |
| 03-Gemini | 2-4m | $0.01 | Stop sequences, generation control |
| 04-Groq | 1-2m | $0.01 | Ultra-fast latency, token accuracy |
| 05-Mistral | 2-3m | $0.001 | Code generation, temperature, retry logic |
| 06-DeepSeek | 2-3m | $0.01 | Math/logic reasoning, vision error handling |
| 07-xAI | 3-4m | $0.05 | Personality, multi-turn, reasoning |
| 08-Native | <1s | $0.00 | Local GGUF, text-only, privacy |
| 09-Mock | <1s | $0.00 | Deterministic, all features, CI-ready |

## Just Show Me (Copy-Paste Ready)

### Instant Test (Right Now, No Setup)
```bash
cd <project-root> && nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```

### With Mistral (Cheapest)
```bash
export MISTRAL_API_KEY="your-mistral-key-here"
cd <project-root> && nika run tests/e2e-provider-tests/05-mistral-standard-inference.nika.yaml
```

### With Groq (Fastest)
```bash
export GROQ_API_KEY="your-groq-key-here"
cd <project-root> && nika run tests/e2e-provider-tests/04-groq-ultra-fast-inference.nika.yaml
```

### With Anthropic (Most Features)
```bash
export ANTHROPIC_API_KEY="sk-ant-your-key-here"
cd <project-root> && nika run tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml
```

## What You'll See

Test output shows:
- ✓ All tasks passed
- Task execution times
- Token usage (input + output)
- Structured output validation
- Error handling verification
- Summary report

Example:
```
=== Mock Provider Deterministic E2E Test Summary ===

Provider: mock (zero-cost, deterministic, for testing)
API Key Required: NO
Cost per token: $0

Test Results:
  ✓ Basic inference working
  ✓ Extended thinking simulated
  ✓ Structured output validated
  ✓ Deterministic responses confirmed
  ...
```

## Validation Only (No Execution)

Check syntax without running:
```bash
nika check tests/e2e-provider-tests/01-anthropic-extended-thinking.nika.yaml
```

Or all at once:
```bash
for f in tests/e2e-provider-tests/*.nika.yaml; do
  echo "Checking $f..."
  nika check "$f" || exit 1
done
echo "All workflows valid!"
```

## Files Explained

```
tests/e2e-provider-tests/
├── 00-QUICKSTART.md                          ← You are here
├── README.md                                 ← Full reference
├── TESTING-GUIDE.md                         ← Deep dive
├── INDEX.md                                 ← Master index
│
├── 01-anthropic-extended-thinking.nika.yaml ← Extended thinking
├── 02-openai-json-response.nika.yaml        ← JSON format
├── 03-gemini-stop-sequences.nika.yaml       ← Stop sequences
├── 04-groq-ultra-fast-inference.nika.yaml   ← Speed test
├── 05-mistral-standard-inference.nika.yaml  ← Code generation
├── 06-deepseek-reasoning.nika.yaml          ← Math/logic
├── 07-xai-grok-inference.nika.yaml          ← Personality
├── 08-native-local-gguf.nika.yaml           ← Local (free)
└── 09-mock-provider-deterministic.nika.yaml ← CI-ready
```

Next step?
- **Start now**: Run mock test above
- **Learn more**: Read [README.md](README.md)
- **Deep dive**: Read [TESTING-GUIDE.md](TESTING-GUIDE.md)
- **All details**: See [INDEX.md](INDEX.md)

## FAQ

**Q: Do I need all API keys?**
A: No. Run just `09-mock-provider-deterministic.nika.yaml` (requires no keys). Or run any single provider test.

**Q: How much will this cost?**
A: Approximately $0.16 for all 9 tests with API keys. Or $0 with mock/native only.

**Q: Can I run this in CI/CD?**
A: Yes! Use `09-mock-provider-deterministic.nika.yaml` in your pipeline (no secrets needed).

**Q: Why does test 06 fail with vision?**
A: Expected! DeepSeek doesn't support vision. The test validates error handling works correctly.

**Q: Why is test 08 so fast?**
A: Native provider runs locally without API calls. It's instant and free.

**Q: Can I modify these tests?**
A: Yes! Edit any `.nika.yaml` file. Just validate with `nika check` before running.

**Q: Where do I find the results?**
A: Output displays in terminal. Tasks show success/failure. Summary at the end.

## Troubleshooting

**"Provider not available"**
→ Set environment variable: `export ANTHROPIC_API_KEY="sk-ant-..."`

**"Model not found"**
→ Run `nika model list` to see available models for that provider

**"Workflow validation error"**
→ Run `nika check workflow.nika.yaml` to see what's wrong

**"Timeout"**
→ Check network connection. Try mock test first (no network needed).

## Next Steps

1. Run the instant mock test (30 seconds)
2. Pick one provider with an API key and test it
3. Explore the workflows to understand patterns
4. Read [TESTING-GUIDE.md](TESTING-GUIDE.md) to learn testing patterns
5. Create your own workflow based on these examples

---

**Ready?** Run this now:
```bash
nika run tests/e2e-provider-tests/09-mock-provider-deterministic.nika.yaml
```

Then read [README.md](README.md) for next steps!
