# Research Report: Provider + Model Configuration Syntax Across AI Frameworks

**Date**: 2026-04-06
**Author**: Thibaut + Nika
**Purpose**: Inform Nika's provider/model config syntax decision

---

## Summary

The industry has converged on **three dominant patterns** for specifying LLM provider + model. The "single string with provider prefix" pattern (LiteLLM's `provider/model`) is winning as the universal glue layer, while YAML-first workflow tools tend toward **separate fields**. No tool that uses both `provider:` and `model:` fields has been criticized for it -- the real UX problem is when the provider field is **required** even though the model name makes it obvious.

---

## The Three Patterns

### Pattern A: Provider-Specific Classes (Code-First)
**Used by**: LangChain, LlamaIndex, Haystack, Vercel AI SDK

The provider is implicit in the class/function you import. Model is a string parameter.

```python
# LangChain
from langchain_openai import ChatOpenAI
llm = ChatOpenAI(model="gpt-4o")

from langchain_anthropic import ChatAnthropic
llm = ChatAnthropic(model="claude-3-5-sonnet-20240620")
```

```python
# LlamaIndex
from llama_index.llms.openai import OpenAI
Settings.llm = OpenAI(model="gpt-4")

from llama_index.llms.ollama import Ollama
Settings.llm = Ollama(model="llama3")
```

```typescript
// Vercel AI SDK
import { openai } from 'ai/openai';
import { anthropic } from 'ai/anthropic';

await generateText({ model: openai('gpt-4o'), prompt: '...' });
await generateText({ model: anthropic('claude-3-5-sonnet'), prompt: '...' });
```

```yaml
# Haystack (YAML serialization)
components:
  llm:
    type: haystack.components.generators.chat.openai_responses.OpenAIResponsesChatGenerator
    init_parameters:
      model: gpt-4o
```

**Verdict**: Clean for code, terrible for YAML config. The "type" path is the provider, the "model" param is the model. Not applicable to Nika's YAML-first approach.

---

### Pattern B: Single String with Provider Prefix (Unified)
**Used by**: LiteLLM, AISuite, CrewAI, Vercel AI Gateway

A single `model:` field encodes both provider and model with a separator.

```python
# LiteLLM — uses slash separator: "provider/model"
litellm.completion(model="openai/gpt-4o", messages=[...])
litellm.completion(model="anthropic/claude-3-5-sonnet-20240620", messages=[...])
litellm.completion(model="ollama/mistral", messages=[...])

# Special case: bare "gpt-4o" auto-detects OpenAI (OpenAI models only)
litellm.completion(model="gpt-4o", messages=[...])
```

```yaml
# LiteLLM Proxy YAML
model_list:
  - model_name: gpt-4-team1          # alias (what clients send)
    litellm_params:
      model: azure/chatgpt-v-2       # actual provider/model
      api_base: https://...
```

```python
# AISuite (Andrew Ng) — uses colon separator: "provider:model"
client.chat.completions.create(
    model="openai:gpt-4o",
    messages=[...]
)
client.chat.completions.create(
    model="anthropic:claude-3-5-sonnet-20240620",
    messages=[...]
)
```

```yaml
# CrewAI YAML — uses slash like LiteLLM
custom_llm:
  model: "anthropic/claude-3-5-sonnet-20240620"
  temperature: 0.2
```

```typescript
// Vercel AI Gateway — uses slash
await generateText({
  model: gateway('anthropic/claude-3-opus'),
  prompt: '...'
});
```

**Verdict**: Elegant for a single field. The separator varies (LiteLLM/CrewAI use `/`, AISuite uses `:`). Works great as a routing/gateway pattern. Problem: breaks down for custom endpoints (vLLM, Azure deployments) where provider != model-name prefix.

---

### Pattern C: Separate Provider + Model Fields (Config-First)
**Used by**: Dify, Prompt Flow, Flowise, Nika (current)

Two explicit fields. Provider selects the API/endpoint, model selects the specific model.

```yaml
# Dify.ai
- id: llm_node
  type: llm
  spec:
    model:
      provider: "openai"
      model: "gpt-4o-2024-05-13"

# Or for Anthropic:
    model:
      provider: "anthropic"
      model: "claude-3-5-sonnet-20240620"
```

```yaml
# Microsoft Prompt Flow
nodes:
- name: my_llm_node
  type: llm
  provider: AzureOpenAI
  connection: my_azure_openai_connection
  api: chat
  deployment_name: gpt-4-deployment

# Or for direct OpenAI:
  provider: OpenAI
  model: gpt-4
```

```json
// Flowise (node config JSON)
{
  "modelName": "claude-3-haiku",
  "temperature": 0.9,
  "maxTokensToSample": 2000
}
// Provider is the NODE TYPE you drag (ChatAnthropic, ChatOpenAI, etc.)
```

```yaml
# Nika (current)
provider: anthropic
model: claude-sonnet-4-20250514
```

**Verdict**: Most explicit, least ambiguous. Handles custom endpoints naturally. Slightly more verbose but zero confusion. This is what enterprise/config-heavy tools choose.

---

## Comparison Matrix

| Tool | Pattern | Provider Field | Model Field | Separator | Auto-Infer Provider? |
|------|---------|---------------|-------------|-----------|---------------------|
| **LiteLLM** | B | No (encoded in model) | `model: "provider/model"` | `/` | Yes (OpenAI models only) |
| **AISuite** | B | No (encoded in model) | `model: "provider:model"` | `:` | No (always required) |
| **CrewAI** | B | No (encoded in model) | `model: "provider/model"` | `/` | No |
| **LangChain** | A | Class import | `model: "name"` | N/A | N/A (class IS provider) |
| **LlamaIndex** | A | Class import | `model: "name"` | N/A | N/A (class IS provider) |
| **Vercel AI SDK** | A+B | Function import OR gateway string | `openai('model')` OR `"provider/model"` | `/` | No |
| **Haystack** | A | `type:` (class path) | `model: "name"` | N/A | N/A (type IS provider) |
| **Dify** | C | `provider: "name"` | `model: "name"` | N/A | No |
| **Prompt Flow** | C | `provider: OpenAI` | `model: gpt-4` | N/A | No |
| **Flowise** | C | Node type (visual) | `modelName: "name"` | N/A | No |
| **Nika** | C | `provider: anthropic` | `model: claude-sonnet-4` | N/A | No |

---

## Key Findings

### 1. Nobody does "auto-infer provider from bare model name" well

LiteLLM is the only tool that attempts it, and it only works for OpenAI models (`gpt-4o` auto-maps to OpenAI). For all other providers, you MUST prefix: `anthropic/claude-3-5-sonnet`. This is because:
- Model names are not globally unique (Llama models served by Groq, Together, Fireworks, Ollama...)
- Custom endpoints make inference impossible
- New models appear faster than any mapping table can update

### 2. The "/" separator is becoming a de facto standard

LiteLLM, CrewAI, Vercel AI Gateway all use `provider/model`. AISuite uses `provider:model` but is the minority. The slash pattern dominates.

### 3. Separate fields win for YAML workflow engines

Dify, Prompt Flow, and Flowise -- the three tools most similar to Nika (visual/config-first workflow engines) -- ALL use separate provider + model fields. This is not a coincidence: when config is YAML, separate fields are more readable, more greppable, and easier to override.

### 4. The hybrid approach exists and works

Vercel AI SDK supports BOTH patterns: explicit `openai('gpt-4o')` AND gateway string `"openai/gpt-4o"`. This suggests the patterns are not mutually exclusive.

### 5. Nobody has been criticized for having both fields

The complaint is never "why do I need to specify provider AND model?" -- it's "why can't I just say `model: gpt-4o` and have it work?". The friction is the *requirement* of the provider field when it's obvious, not its *existence*.

---

## Recommendations for Nika

### Option 1: Keep Current (Separate Fields) -- RECOMMENDED

```yaml
# Explicit, clear, works for everything
provider: anthropic
model: claude-sonnet-4-20250514
```

**Why**: Nika is a YAML workflow engine, not a Python library. Separate fields are the industry standard for config-first tools (Dify, Prompt Flow, Flowise). Zero ambiguity. Handles custom endpoints, vLLM, Azure deployments naturally.

**Enhancement**: Make `provider:` optional with auto-inference as syntactic sugar:
```yaml
# These would be equivalent:
model: claude-sonnet-4-20250514        # auto-infers provider: anthropic
model: gpt-4o                           # auto-infers provider: openai
provider: anthropic                     # explicit (override or for ambiguous models)
model: claude-sonnet-4-20250514
```

### Option 2: Support Slash Syntax as Sugar

```yaml
# Accept both:
model: anthropic/claude-sonnet-4-20250514   # slash syntax (LiteLLM-compatible)
model: claude-sonnet-4-20250514              # auto-infer
provider: anthropic                          # explicit override
model: claude-sonnet-4-20250514
```

**Why**: Familiarity for LiteLLM users (huge community). But adds parser complexity and the slash in YAML needs quoting awareness.

### Option 3: Single Field Only (Drop provider:)

```yaml
model: anthropic/claude-sonnet-4-20250514
```

**Why NOT**: Breaks custom endpoints (`provider: openai` + `base_url:` for vLLM). Forces encoding everything in one string. Less readable in YAML.

---

## Final Verdict

**Keep `provider:` + `model:` as separate fields.** This is what every YAML/config-first workflow tool does. Add auto-inference of provider from model name as optional sugar (make `provider:` optional when the model name is unambiguous). This gives Nika the best of both worlds: explicit when needed, concise when obvious.

The LiteLLM `provider/model` pattern is great for routing layers and code SDKs, but it's not the right pattern for a YAML workflow engine where readability and explicitness matter more than brevity.

---

## Sources

1. LiteLLM docs - https://docs.litellm.ai/docs/proxy/configs
2. LiteLLM providers - https://docs.litellm.ai/docs/providers
3. LangChain models - https://docs.langchain.com/oss/python/langchain/models
4. LangChain ModelSpec - https://reference.langchain.com/python/deepagents-cli/model_config/ModelSpec
5. LlamaIndex LLMs - https://developers.llamaindex.ai/python/framework/module_guides/models/llms/
6. LlamaIndex Settings - https://developers.llamaindex.ai/python/framework/module_guides/supporting_modules/settings/
7. Vercel AI SDK providers - https://ai-sdk.dev/docs/foundations/providers-and-models
8. Vercel AI Gateway - https://vercel.com/docs/ai-gateway/models-and-providers
9. CrewAI LLMs - https://docs.crewai.com/en/concepts/llms
10. Haystack pipeline components - https://docs.cloud.deepset.ai/docs/pipeline-components
11. Haystack serialization - https://haystack.deepset.ai/tutorials/29_serializing_pipelines
12. Dify model providers - https://docs.dify.ai/en/use-dify/workspace/model-providers
13. Dify model plugin dev - https://docs.dify.ai/en/develop-plugin/dev-guides-and-walkthroughs/creating-new-model-provider
14. Prompt Flow LLM tool - https://learn.microsoft.com/en-us/azure/machine-learning/prompt-flow/tools-reference/llm-tool
15. Prompt Flow YAML schema - https://microsoft.github.io/promptflow/reference/flow-yaml-schema-reference.html
16. Flowise LLM providers - https://www.mintlify.com/FlowiseAI/Flowise/integrations/llm-providers
17. AISuite GitHub - https://github.com/andrewyng/aisuite

## Methodology

- Tools used: Perplexity search (sonar model), 11 queries
- Sources analyzed: 17 primary documentation pages
- Frameworks covered: LiteLLM, LangChain, LlamaIndex, Vercel AI SDK, CrewAI, Haystack, Dify, Prompt Flow, Flowise, AISuite
- Confidence: **High** -- based on current official documentation, not blog posts

## Further Research Suggestions

- Deep-dive into how each tool handles **custom endpoints** (vLLM, Azure, self-hosted)
- Research how `base_url:` / `api_base:` interacts with provider/model in each tool
- Investigate the model alias/routing patterns (LiteLLM proxy model_name vs litellm_params.model)
- Look at how DSPy handles model specification (uses LiteLLM under the hood)
