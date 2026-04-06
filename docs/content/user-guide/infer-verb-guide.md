# The infer: Verb -- Complete Guide

The `infer:` verb is the heart of Nika workflows. It sends prompts to LLM providers and captures their responses. This guide covers every feature: basic prompting, system prompts, temperature control, structured output, multimodal/vision input, extended thinking, guardrails, and provider-specific options.

## Basic Usage

The simplest `infer:` task needs just a prompt:

```yaml
schema: nika/workflow@0.12
workflow: basic-infer
provider: anthropic

tasks:
  - id: greet
    infer:
      prompt: "Say hello in three different languages."
```

This sends the prompt to the default provider (Anthropic/Claude) and captures the response as the task output.

### String Shorthand

For the simplest cases, you can inline the prompt:

```yaml
  - id: greet
    infer: "Say hello in three different languages."
```

This is equivalent to `infer: { prompt: "..." }`.

## All infer: Fields

Here is the complete field reference:

```yaml
- id: my_task
  infer:
    # Core
    prompt: "Your prompt here"               # Required (unless content: present)
    system: "You are a helpful assistant"    # System prompt override
    temperature: 0.7                          # 0.0 (deterministic) to 2.0 (creative)
    max_tokens: 1000                          # Maximum output tokens

    # Output format
    response_format: json                     # text | json | markdown

    # Claude-specific
    extended_thinking: true                   # Enable chain-of-thought
    thinking_budget: 10000                    # Tokens for thinking

    # Vision/Multimodal
    content:                                  # Multimodal content blocks
      - type: image
        source: "hash-or-path"
        detail: high
      - type: text
        text: "Describe this image"

    # Guardrails
    guardrails:                               # Output validation
      - type: length
        min_words: 50
        max_words: 200
```

## Prompt Engineering

### Using Templates

Inject data from other tasks or context files into prompts:

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"

  - id: analyze
    depends_on: [fetch_data]
    with:
      data: $fetch_data | trim
    infer:
      prompt: |
        Analyze the following data and identify trends:

        {{with.data}}

        Provide your analysis in 3 sections:
        1. Key findings
        2. Trends
        3. Recommendations
```

### System Prompts

Set the LLM's persona and instructions with `system:`:

```yaml
  - id: code_review
    infer:
      system: |
        You are a senior software engineer conducting code reviews.
        You focus on: correctness, performance, readability, and security.
        Be direct and specific. Reference line numbers when possible.
      prompt: "Review this code:\n{{with.code}}"
      temperature: 0.2
```

System prompts are set once and persist for the duration of the inference call. They are ideal for establishing tone, expertise, and constraints.

### Temperature Control

Temperature controls randomness in the LLM output:

| Temperature | Effect | Use Case |
|-------------|--------|----------|
| 0.0 | Deterministic, most likely tokens | Classification, extraction, factual Q&A |
| 0.3 | Low variability | Analysis, code generation, summaries |
| 0.7 | Balanced creativity | General writing, brainstorming |
| 1.0 | High variability | Creative writing, poetry |
| 1.5-2.0 | Very random | Experimental, artistic |

```yaml
  # Deterministic classification
  - id: classify
    infer:
      prompt: "Classify as positive, negative, or neutral: {{with.text}}"
      temperature: 0.0

  # Creative story
  - id: story
    infer:
      prompt: "Write a short story about a robot learning to paint."
      temperature: 1.0
      max_tokens: 2000
```

### Max Tokens

Limit the output length to control costs and response time:

```yaml
  - id: one_liner
    infer:
      prompt: "Explain quantum computing."
      max_tokens: 50   # Forces brevity
```

If the LLM needs more tokens than allowed, it will be cut off mid-response. Set this based on your expected output length.

## Structured Output

The structured output system ensures LLM responses conform to a JSON Schema. This is critical for workflows that need to parse, validate, or further process LLM output.

### Inline Schema

```yaml
  - id: extract_contacts
    infer:
      prompt: "Extract all contact information from: {{with.email}}"
    structured:
      schema:
        type: object
        properties:
          contacts:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                email:
                  type: string
                  format: email
                phone:
                  type: string
              required: [name]
        required: [contacts]
      max_retries: 3
      enable_repair: true
```

### File Schema Reference

Keep schemas in separate files for reuse:

```yaml
  - id: classify
    infer:
      prompt: "Classify this document."
    structured: ./schemas/classification.json
```

### Shorthand vs Full Configuration

```yaml
# Shorthand (just the schema path, uses defaults)
structured: ./schemas/output.json

# Full configuration
structured:
  schema: ./schemas/output.json
  max_retries: 3                  # Retry with feedback on validation failure
  enable_repair: true             # Use LLM to fix complex violations
  repair_model: claude-sonnet-4-6 # Model for repair (default: same as task)
  enable_tool_injection: true     # Inject schema as a tool for provider-side enforcement
  enable_retry: true              # Enable retry with validation errors
```

### How It Works (4 Layers)

Nika's structured output engine uses multiple layers for near-perfect compliance:

1. **Layer 0: Tool Injection** -- Sends the schema as a synthetic tool parameter, leveraging the provider's built-in function calling for schema enforcement
2. **Layer 1: Extraction** -- Parses the response and extracts JSON from the output
3. **Layer 2: Retry** -- Re-prompts with specific validation error messages
4. **Layer 3: Repair** -- Uses an LLM to intelligently fix complex schema violations

### Using Structured Output with Transforms

The structured output produces JSON that can be accessed via JSONPath in downstream tasks:

```yaml
  - id: extract
    infer:
      prompt: "Extract product info from: {{with.page}}"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          price: { type: number }
          tags: { type: array, items: { type: string } }
        required: [name, price]

  - id: use_result
    depends_on: [extract]
    with:
      product_name: $extract.name
      product_price: $extract.price
      first_tag: $extract.tags[0]
    exec: "echo 'Product: {{with.product_name}} (${{with.product_price}})'"
```

## Vision and Multimodal Input

Since v0.34.0, Nika supports sending images to vision-capable LLMs using the `content:` field.

### Image from CAS (Content-Addressable Store)

After importing an image with `nika:import`, reference it by its CAS hash:

```yaml
tasks:
  - id: import_photo
    invoke:
      tool: "nika:import"
      params:
        path: "./photo.jpg"

  - id: describe
    depends_on: [import_photo]
    with:
      photo: $import_photo
    infer:
      content:
        - type: image
          source: "{{with.photo.media[0].hash}}"
          detail: high
        - type: text
          text: "Describe this image in detail. What objects, people, and activities are visible?"
```

### Image from URL

```yaml
  - id: analyze_image
    infer:
      content:
        - type: image_url
          url: "https://example.com/photo.jpg"
          detail: auto
        - type: text
          text: "What is happening in this image?"
```

### Combining Prompt and Content

When both `prompt:` and `content:` are present, the prompt is prepended as the first text part:

```yaml
  - id: vision_with_context
    infer:
      prompt: "You are analyzing product images for an e-commerce catalog."
      content:
        - type: image
          source: "{{with.product_image.media[0].hash}}"
          detail: high
        - type: text
          text: "Generate a product description and list visible features."
```

### Detail Levels

| Level | Description | Token Cost |
|-------|-------------|------------|
| `auto` | Let the provider decide | Varies |
| `low` | Low-resolution analysis | Lower |
| `high` | High-resolution, detailed analysis | Higher |

### Provider Vision Support

| Provider | Vision Support |
|----------|---------------|
| Anthropic (Claude) | Yes |
| OpenAI (GPT-4o) | Yes |
| Mistral | Yes |
| Groq | Yes |
| Gemini | Yes |
| xAI (Grok) | Yes |
| DeepSeek | No |
| Native (GGUF) | No (use VisionHf instead) |

For native vision, use HuggingFace models with ISQ quantization:

```bash
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
```

## Extended Thinking (Claude)

Claude models support extended thinking -- a chain-of-thought reasoning mode that allocates a separate token budget for the model to "think" before responding:

```yaml
  - id: complex_reasoning
    provider: anthropic
    infer:
      prompt: |
        A farmer has a 100-acre field. He plants corn on 60% and wheat on the rest.
        Corn yields 150 bushels/acre at $4/bushel. Wheat yields 40 bushels/acre at $8/bushel.
        Calculate total revenue and recommend the optimal split.
      extended_thinking: true
      thinking_budget: 10000
```

The thinking output is captured but not included in the main response. Use this for math, logic puzzles, complex analysis, and multi-step reasoning.

## Response Format

Control the output format hint sent to the provider:

```yaml
  - id: json_output
    infer:
      prompt: "List 5 programming languages with their year of creation."
      response_format: json
```

| Format | Effect |
|--------|--------|
| `text` | Default free-form text |
| `json` | Hint to produce valid JSON |
| `markdown` | Hint to produce Markdown |

Note: `response_format: json` is a soft hint. For guaranteed JSON compliance, use `structured:` instead.

## Guardrails

Validate LLM output before accepting it:

### Length Guardrail

```yaml
  - id: summary
    infer:
      prompt: "Summarize this article."
      guardrails:
        - type: length
          min_words: 50
          max_words: 200
```

### Schema Guardrail

```yaml
  - id: structured_check
    infer:
      prompt: "Generate user profile data."
      guardrails:
        - type: schema
          json_schema:
            type: object
            properties:
              name: { type: string }
              age: { type: integer }
            required: [name, age]
```

### Regex Guardrail

```yaml
  - id: format_check
    infer:
      prompt: "Generate a UUID."
      guardrails:
        - type: regex
          pattern: "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
          message: "Output must be a valid UUID"
```

### LLM Judge Guardrail

Use a second LLM call to validate the output:

```yaml
  - id: quality_check
    infer:
      prompt: "Write a product review."
      guardrails:
        - type: llm
          judge_prompt: |
            Evaluate if this product review is professional, unbiased, and factual.
            Respond with PASS or FAIL followed by your reasoning.
          pass_pattern: "^PASS"
          on_failure: retry
```

### Guardrail Escalation

Each guardrail can specify what happens on failure:

| Action | Effect |
|--------|--------|
| `retry` | Ask the LLM to fix the output (default) |
| `escalate` | Escalate to human or supervisor |
| `fail` | Fail the task immediately |

## Provider and Model Overrides

Override the default provider or model for specific tasks:

```yaml
schema: nika/workflow@0.12
provider: anthropic                     # Default for all tasks
model: claude-sonnet-4-6             # Default model

tasks:
  - id: fast_task
    provider: groq                      # Override provider
    model: llama-3.3-70b-versatile      # Override model
    infer:
      prompt: "Quick classification task"

  - id: smart_task
    model: claude-opus-4               # Same provider, different model
    infer:
      prompt: "Complex reasoning task"
```

You can also override from the CLI:

```bash
nika run workflow.nika.yaml --provider openai --model gpt-4o
```

## Cost Management

### Limiting Token Output

```yaml
  - id: concise
    infer:
      prompt: "Explain recursion."
      max_tokens: 100              # Limit output length
```

### Using Cheaper Models for Simple Tasks

```yaml
  # Fast/cheap for classification
  - id: classify
    provider: groq
    infer:
      prompt: "Is this spam? Yes or No. {{with.email}}"
      temperature: 0.0
      max_tokens: 5

  # Premium for complex analysis
  - id: deep_analysis
    provider: anthropic
    model: claude-opus-4
    infer:
      prompt: "Provide detailed security analysis of: {{with.code}}"
```

### Mock Provider for Testing

Test workflow structure without making API calls:

```bash
nika run workflow.nika.yaml --provider mock
```

The mock provider returns predictable test responses without incurring any cost.

## Common Patterns

### Summarization Pipeline

```yaml
  - id: summarize
    infer:
      system: "Summarize text concisely. Use bullet points."
      prompt: "{{with.content}}"
      temperature: 0.3
      max_tokens: 300
```

### Classification

```yaml
  - id: classify
    infer:
      prompt: |
        Classify the following text into one of these categories:
        - technology
        - science
        - politics
        - entertainment
        - sports

        Text: {{with.text}}

        Category:
      temperature: 0.0
      max_tokens: 10
```

### Data Extraction

```yaml
  - id: extract
    infer:
      prompt: "Extract all dates, names, and monetary amounts from: {{with.document}}"
    structured:
      schema:
        type: object
        properties:
          dates: { type: array, items: { type: string } }
          names: { type: array, items: { type: string } }
          amounts: { type: array, items: { type: number } }
        required: [dates, names, amounts]
```

### Translation

```yaml
  - id: translate
    infer:
      system: "You are a professional translator. Translate accurately preserving tone and meaning."
      prompt: "Translate to French:\n\n{{with.text}}"
      temperature: 0.2
```

### Code Generation

```yaml
  - id: generate_code
    infer:
      system: |
        You are an expert programmer. Write clean, well-documented code.
        Include error handling. Follow the language's idiomatic style.
      prompt: |
        Write a Python function that:
        {{with.requirements}}
      temperature: 0.3
      max_tokens: 2000
```

### Multi-Step Analysis Pipeline

Combine multiple infer tasks for progressive refinement:

```yaml
schema: nika/workflow@0.12
workflow: progressive-analysis
provider: anthropic

tasks:
  - id: raw_analysis
    infer:
      prompt: "Analyze the competitive landscape for: {{with.topic}}"
      temperature: 0.5
      max_tokens: 1500

  - id: critique
    depends_on: [raw_analysis]
    with:
      analysis: $raw_analysis
    infer:
      system: "You are a critical reviewer. Find gaps, biases, and unsupported claims."
      prompt: "Critique this analysis:\n\n{{with.analysis}}"
      temperature: 0.2

  - id: final_version
    depends_on: [raw_analysis, critique]
    with:
      draft: $raw_analysis
      feedback: $critique
    infer:
      prompt: |
        Revise this analysis based on the critique:

        Original:
        {{with.draft}}

        Critique:
        {{with.feedback}}

        Produce an improved, final version.
      temperature: 0.3
```

### Comparing Multiple Providers

Run the same prompt through different providers and compare quality:

```yaml
schema: nika/workflow@0.12
workflow: provider-shootout

tasks:
  - id: prompt
    exec: "echo 'Explain the CAP theorem in distributed systems in exactly 3 sentences.'"

  - id: claude_response
    depends_on: [prompt]
    with: { p: $prompt | trim }
    provider: anthropic
    model: claude-sonnet-4-20250514
    infer: { prompt: "{{with.p}}", temperature: 0.0 }

  - id: gpt_response
    depends_on: [prompt]
    with: { p: $prompt | trim }
    provider: openai
    model: gpt-4o
    infer: { prompt: "{{with.p}}", temperature: 0.0 }

  - id: judge
    depends_on: [claude_response, gpt_response]
    with:
      claude: $claude_response | trim
      gpt: $gpt_response | trim
    provider: anthropic
    model: claude-opus-4-20250514
    infer:
      prompt: |
        Compare these two responses for accuracy, clarity, and completeness.
        Do not reveal which model generated which response.

        Response A: {{with.claude}}
        Response B: {{with.gpt}}

        Which is better and why?
      temperature: 0.0
```

## Troubleshooting infer

### Empty Prompt Error

```
[NIKA-004] Resolved prompt is empty
```

Your template resolved to an empty string. Check that your `with:` bindings have valid values:

```yaml
# Verify the upstream task has output
- id: check
  depends_on: [source]
  with:
    data: $source | default("no data") | trim
  exec: "echo 'Data length: {{with.data | length}}'"
```

### Token Limit Exceeded

If the LLM response is cut off, increase `max_tokens`:

```yaml
infer:
  prompt: "..."
  max_tokens: 4000   # Increase from default
```

### Inconsistent JSON Output

Use `structured:` instead of `response_format: json` for guaranteed schema compliance:

```yaml
# Unreliable
infer:
  prompt: "Return JSON with name and age"
  response_format: json

# Reliable
infer:
  prompt: "Extract name and age"
structured:
  schema:
    type: object
    properties:
      name: { type: string }
      age: { type: integer }
    required: [name, age]
```
