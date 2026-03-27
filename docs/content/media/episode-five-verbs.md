# Episode 2: Five Verbs to Rule Them All

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 02 |
| **Duration** | ~30 minutes |
| **Topics** | infer, exec, fetch, invoke, agent -- deep dive into each verb |
| **Guest Suggestions** | An LLM API expert, a web scraping specialist, an MCP protocol contributor |
| **Audience** | Developers who want to understand Nika's core primitives |
| **Prerequisites** | Episode 1 or basic Nika awareness |

---

## Cold Open (30 seconds)

[MUSIC: Rhythmic, building energy]

**Host:** Here is a question that will make any framework designer squirm: how many primitives does your system really need?

Too few and you are constantly working around limitations. Too many and you have a bloated API where nobody remembers which method to call. The sweet spot is small enough to memorize, large enough to express anything.

[PAUSE]

Nika chose five. Five verbs. Not four. Not six. Exactly five. And today, we are going to find out why -- and what each one can really do.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Welcome back to "Building Nika -- A Rust AI Engine from Scratch." Last episode, we covered the big picture -- what Nika is, why it exists, and how it fits into the AI workflow landscape. Today we are going microscopic.

Five verbs: infer, exec, fetch, invoke, agent. These are the atoms of Nika. Everything you build -- every AI pipeline, every data processing flow, every autonomous agent -- is composed from these five operations. And each one has surprising depth once you start digging.

Let us start with the verb that does the most heavy lifting.

---

## Segment 1: infer -- The LLM Generation Verb (10 minutes)

**Host:** `infer:` is Nika's primary verb for LLM generation. At its simplest, it is one line:

[CODE EXAMPLE]
```yaml
- id: hello
  infer: "Tell me a joke about Rust programming"
```

That is the shorthand form. Nika resolves your LLM provider from environment variables -- it checks ANTHROPIC_API_KEY, then OPENAI_API_KEY, then MISTRAL_API_KEY, and so on -- sends the prompt, and returns the response. But the full object form is where infer gets powerful:

[CODE EXAMPLE]
```yaml
- id: analyze
  infer:
    prompt: "Analyze this codebase for security issues"
    model: claude-sonnet-4-20250514
    provider: anthropic
    temperature: 0.3
    max_tokens: 4000
    system: "You are a senior security auditor. Be thorough and precise."
```

Now you have explicit control over the model, provider, temperature, token limit, and system prompt. But we are just getting started.

**Structured Output**

[EMPHASIS] This is one of Nika's most impressive features. When you add a `structured:` block, Nika guarantees your LLM response will conform to a JSON schema. Not "usually." Not "if the LLM cooperates." Guarantees.

[CODE EXAMPLE]
```yaml
- id: extract_products
  infer:
    prompt: "Extract all product information from this page"
  structured:
    schema:
      type: object
      properties:
        products:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              price: { type: number }
              currency: { type: string }
            required: [name, price]
      required: [products]
    max_retries: 3
    enable_repair: true
```

How does it guarantee this? Through a five-layer validation cascade:

Layer 0 -- DynamicSubmitTool. Nika injects a tool-use schema into the LLM request, asking the model to "submit" its response as a tool call with the exact schema. This works about 80-90% of the time.

Layer 1 -- Extract JSON. If Layer 0 fails, Nika extracts JSON from the raw text response. Maybe the model returned valid JSON wrapped in markdown code blocks. Layer 1 handles that.

Layer 2 -- Schema Validation + Repair Prompts. The extracted JSON is validated against the schema. If it fails, Nika generates a specific repair prompt: "Your response had these schema violations: [list]. Please fix them."

Layer 3 -- LLM Repair with Retry. The repair prompt goes back to the LLM. This is where `max_retries` kicks in.

Layer 4 -- Manual Schema Coercion. As a last resort, Nika attempts programmatic coercion -- converting strings to numbers, wrapping single values in arrays, adding missing required fields with null values.

[EMPHASIS] The result? Approximately 99.99% JSON compliance across all providers. Your downstream tasks can safely assume they are getting a typed object, not a string that might or might not parse.

**Multimodal Vision**

Since version 0.34, `infer:` supports sending images to vision-capable LLMs:

[CODE EXAMPLE]
```yaml
- id: describe_photo
  infer:
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe what you see in this image"
```

Notice the `content:` field replaces `prompt:` for multimodal requests. You can mix text and image parts. The image `source` can be a CAS hash (from Nika's content-addressable storage) -- the engine automatically resolves it to base64. Or you can use `image_url` for HTTPS URLs (with SSRF protection -- only HTTPS, never HTTP).

Vision works with Claude, OpenAI, Mistral, Groq, Gemini, and xAI for cloud providers. And for local inference, Nika supports HuggingFace vision models via mistral.rs with Integer-Scaled Quantization -- you can run Qwen2.5-VL 7B on a MacBook with about 5 GB of VRAM.

[PAUSE]

**Extended Thinking**

For Claude specifically, `infer:` supports extended thinking -- where the model spends extra tokens on internal reasoning before producing its answer:

[CODE EXAMPLE]
```yaml
- id: deep_analysis
  infer:
    prompt: "Design a microservices architecture for a trading platform"
    provider: anthropic
    model: claude-sonnet-4-20250514
    thinking_budget: 32768
```

The `thinking_budget` ranges from 1,024 to 65,536 tokens. The reasoning is captured in the task metadata but not passed to downstream tasks by default -- you get the distilled answer, not the scratch work.

---

## Segment 2: exec and fetch -- The World Outside the LLM (8 minutes)

**Host:** Not everything in an AI workflow needs an LLM. Sometimes you just need to run a shell command or make an HTTP request. That is where `exec:` and `fetch:` come in.

### exec: -- Shell Commands with Guardrails

[CODE EXAMPLE]
```yaml
- id: list_files
  exec: "ls -la ./data"

- id: run_query
  exec:
    command: "psql -c 'SELECT count(*) FROM users' --csv"
    shell: true
    timeout: 30
```

Like `infer:`, `exec:` has a shorthand string form and a full object form. The shorthand runs the command in shell-free mode using shlex parsing -- the command string is tokenized without invoking a shell interpreter. This is safer by default because shell metacharacters like `$()`, backticks, and pipes are treated as literal strings.

When you need shell features -- pipes, redirection, variable expansion -- you set `shell: true`. But Nika applies additional security checks in shell mode, blocking command substitution patterns like `$(` and backticks.

[EMPHASIS] And here is what makes Nika's exec different from just running a shell command: the blocklist.

Nika maintains a list of dangerous command patterns. Things like:

- `rm -rf /` (and variants)
- `| bash`, `| sh` (piping downloads to shell)
- `sudo`, `doas`, `pkexec` (privilege escalation)
- `eval ` (dynamic code execution)
- `mkfifo` (named pipes for reverse shells)
- `nc -e`, `ncat -e` (netcat reverse shells)
- `python -c "import socket` (Python reverse shells)
- `chmod 777` (dangerous permissions)
- `dd if=` (disk destruction)
- `base64 -d |` (encoded payload execution)
- `perl -e`, `ruby -e`, `node -e` (interpreter bypass)
- Fork bombs

[PAUSE]

But here is the really clever part: Unicode normalization. An attacker might try to bypass the blocklist by using fullwidth characters -- writing `sudo` using the fullwidth Unicode variants that look identical but are technically different code points. Nika applies NFKC normalization (Compatibility Decomposition plus Canonical Composition) before checking the blocklist. So fullwidth `sudo` normalizes to ASCII `sudo`. Math bold `sudo` normalizes to ASCII `sudo`. Zero-width spaces inserted between characters get stripped. The blocklist check operates on the normalized form.

This is the kind of security detail that most workflow engines do not even think about.

### fetch: -- HTTP Requests with Intelligence

Now let us talk about `fetch:`. On the surface, it is simple:

[CODE EXAMPLE]
```yaml
- id: get_data
  fetch:
    url: https://api.example.com/data
    method: GET
    headers:
      Authorization: "Bearer {{env.API_TOKEN}}"
```

But Nika's `fetch:` has nine extraction modes that turn raw HTTP responses into structured, useful data.

[EMPHASIS] Let me walk through all nine.

**1. `extract: markdown`** -- Takes any HTML page and converts it to clean Markdown. Uses the htmd library. Perfect for feeding web content to an LLM -- HTML tags are noise, Markdown is signal.

[CODE EXAMPLE]
```yaml
- id: read_article
  fetch:
    url: https://blog.example.com/post
    extract: markdown
```

**2. `extract: article`** -- Uses Readability (like the Firefox Reader View) to extract just the main article content, stripping navigation, ads, and sidebars. Powered by dom_smoothie.

**3. `extract: text`** -- Extracts visible text only. You can optionally add a `selector:` to target specific elements:

[CODE EXAMPLE]
```yaml
- id: get_prices
  fetch:
    url: https://shop.example.com/products
    extract: text
    selector: ".price"
```

**4. `extract: selector`** -- Returns raw HTML of matching CSS selector elements. For when you need the structure, not just the text.

**5. `extract: metadata`** -- Extracts OpenGraph tags, Twitter Cards, JSON-LD structured data, and SEO metadata. Returns everything as a JSON object. Perfect for content analysis workflows.

**6. `extract: links`** -- Rich link classification. Returns all links on a page, classified as internal/external, navigation/content/footer, with full URL resolution. Great for web crawling workflows.

**7. `extract: jsonpath`** -- Applies a JSONPath query to a JSON API response. The `selector:` field contains the JSONPath expression:

[CODE EXAMPLE]
```yaml
- id: get_names
  fetch:
    url: https://api.example.com/users
    extract: jsonpath
    selector: "$.data[*].name"
```

**8. `extract: feed`** -- Parses RSS, Atom, and JSON Feed formats via the feed-rs library. Returns structured feed data.

**9. `extract: llm_txt`** -- Discovers AI-era content using the emerging llm.txt convention. Checks `/.well-known/llm.txt` and `/llms.txt` for machine-readable content manifests.

[PAUSE]

**Host:** And `fetch:` has response modes too. `response: full` gives you a JSON object with status code, headers, body, and final URL (after redirects). `response: binary` stores the response body in the CAS (content-addressable storage) and returns a hash -- perfect for downloading images or files that will be processed by the media pipeline.

---

## Segment 3: invoke and agent -- Tools and Autonomy (8 minutes)

**Host:** The last two verbs are where Nika becomes truly powerful. `invoke:` connects Nika to the world of tools. `agent:` makes it autonomous.

### invoke: -- MCP Tool Calls

`invoke:` calls tools using the Model Context Protocol. These tools come from two sources.

**Source 1: 24 built-in tools.** These are tools that ship with Nika itself, organized in three tiers:

Tier 1 -- Always available. Five tools with zero or tiny dependencies:
- `nika:import` -- Import files into content-addressable storage
- `nika:dimensions` -- Get image dimensions in about 0.1 milliseconds from headers alone
- `nika:thumbhash` -- Generate 25-byte image placeholders
- `nika:dominant_color` -- Extract color palettes
- `nika:pipeline` -- Chain media operations in-memory with zero intermediate files

Tier 2 -- Default on (media-core feature). Six tools for image processing:
- `nika:thumbnail` -- SIMD-accelerated image resize with Lanczos3
- `nika:convert` -- Format conversion between PNG, JPEG, and WebP
- `nika:strip` -- Remove EXIF metadata by decode-and-re-encode
- `nika:metadata` -- Extract universal metadata from images, audio, video
- `nika:optimize` -- Lossless PNG optimization via oxipng
- `nika:svg_render` -- SVG to PNG rasterization via resvg

Tier 3 -- Opt-in. Thirteen specialized tools for perceptual hashing, PDF extraction, chart generation, C2PA content provenance (for EU AI Act compliance), QR code validation, image quality assessment, and web content processing.

[CODE EXAMPLE]
```yaml
- id: process_image
  invoke:
    tool: nika:thumbnail
    args:
      hash: "{{with.imported.hash}}"
      width: 800
      height: 600
      format: webp
```

**Source 2: External MCP servers.** Any MCP-compatible server can be connected. Nika ships with 100+ pre-configured aliases for popular services:

[CODE EXAMPLE]
```yaml
# Call an external MCP server tool
- id: create_issue
  invoke:
    tool: github:create_issue
    args:
      repo: "supernovae/nika"
      title: "Bug found by automated analysis"
      body: "{{with.analysis.summary}}"
```

The MCP client (the nika-mcp crate, 9K lines) handles connection pooling, retry with exponential backoff, automatic reconnection, response caching, and tool schema validation. It uses rmcp v0.16 as the protocol implementation.

### agent: -- The Autonomous Loop

[EMPHASIS] Now we arrive at the most powerful verb. `agent:` creates a multi-turn autonomous loop -- an agent that gets tools, a goal, and guardrails, and runs until it achieves its objective.

[CODE EXAMPLE]
```yaml
- id: research_agent
  agent:
    goal: "Research the competitive landscape for AI workflow engines"
    model: claude-sonnet-4-20250514
    provider: anthropic
    tools:
      - nika:read
      - nika:write
      - nika:glob
      - nika:grep
    max_turns: 20
    stop_conditions:
      - "Report has been written to output.md"
    guardrails:
      - type: length
        max: 50000
      - type: regex
        pattern: "\\bREFERENCES\\b"
        must_match: true
```

Let me break this down.

The agent runs in a loop. Each turn, it:
1. Receives the conversation history plus available tools
2. Decides which tool to call (or whether to respond)
3. Calls the tool and receives the result
4. Decides if it is done (checks stop conditions)
5. If not done, loops back to step 1

The `guardrails:` are quality gates applied to the agent's final response. Four types:

- **length** -- min/max character bounds
- **schema** -- JSON schema validation on the response
- **regex** -- pattern matching (must_match or must_not_match)
- **llm** -- an LLM-based quality judgment (you give it a prompt like "Is this report comprehensive?" and a model evaluates)

When a guardrail fails, the escalation path is: retry, then escalate (use a better model), then fail.

[PAUSE]

**Host:** Agents can also spawn sub-agents. The `SpawnAgentTool` lets an agent create child agents with their own tools and goals, up to a configurable depth limit (default 3, max 10). This enables hierarchical task decomposition -- a planning agent that spawns research agents that spawn data collection agents.

And all of this supports streaming. Every provider -- all 22 of them -- can stream tokens in real time. In the TUI, you watch the agent think and act in real time.

---

## Segment 4: Why Five? The Design Philosophy (2 minutes)

**Host:** So why exactly five verbs? This is a design question worth addressing directly.

[PAUSE]

Every action an AI workflow needs to perform falls into one of five categories:

1. **Think** (infer) -- Generate text, reason, classify, extract. Anything that requires an LLM.
2. **Do** (exec) -- Execute a command on the host machine. Anything the operating system can do.
3. **Connect** (fetch) -- Communicate over HTTP. Anything the network can reach.
4. **Use** (invoke) -- Call a tool through a protocol. Anything an MCP server exposes.
5. **Decide** (agent) -- Run autonomously with tools until a goal is met. The combination of all the above with a feedback loop.

If you removed any one of these, you would lose an entire category of capability. Without `exec:`, you could not run local scripts. Without `fetch:`, you could not call APIs that are not MCP servers. Without `invoke:`, you could not use structured tool protocols. Without `agent:`, you could not build autonomous systems.

And if you added a sixth -- say, a `transform:` verb for data manipulation -- you would not gain any new capability, because `infer:` with structured output already handles transformation, and pipe transforms in the binding system handle simple cases inline.

[EMPHASIS] Five is the minimum complete set. That is why it is five.

---

## Wrap-up & Preview (2 minutes)

**Host:** Let us recap. Five verbs, each with surprising depth:

- `infer:` -- LLM generation with 22 providers, structured output, vision, extended thinking
- `exec:` -- Shell commands with Unicode-aware security blocklist
- `fetch:` -- HTTP with nine extraction modes for web intelligence
- `invoke:` -- MCP tools with 24 builtins and 100+ external aliases
- `agent:` -- Autonomous loops with guardrails, streaming, and sub-agent spawning

These five operations compose into DAG-scheduled workflows where independent tasks run in parallel automatically, data flows through typed bindings, and structured output guarantees valid JSON.

[PAUSE]

Next episode, we are going deep into the Rust architecture. The 12-crate workspace design. The three-phase AST and why it matters. The IndexedDag with Kahn's algorithm. The zero-I/O core principle. How Nika abstracts 9 LLM providers into a single interface. And why 8,300+ tests is just the beginning.

[MUSIC: Outro theme]

**Host:** This has been "Building Nika." See you in Episode 3.

---

## Show Notes

### Links
- Nika Schema Reference: `nika/workflow@0.12`
- MCP Protocol Specification: [modelcontextprotocol.io](https://modelcontextprotocol.io)
- rig-core (Rust LLM framework): [github.com/0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig)
- rmcp (Rust MCP client): [crates.io/crates/rmcp](https://crates.io/crates/rmcp)

### Code Examples Referenced
- Simple infer (one-liner)
- Full infer with structured output (five-layer cascade)
- Multimodal vision with content blocks
- exec with security blocklist
- fetch with 9 extraction modes
- invoke with builtin and external MCP tools
- agent with guardrails and stop conditions

### Technical Deep Dives
- **Structured Output Layers:** DynamicSubmitTool, Extract, Validate+Repair, LLM Retry, Manual Coercion
- **Security Blocklist:** 28+ patterns, NFKC normalization, zero-width character stripping
- **Fetch Extraction:** markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt
- **Agent Guardrails:** length, schema, regex, llm -- with retry/escalate/fail cascade
- **Sub-Agent Spawning:** SpawnAgentTool, depth_limit (default 3, max 10)

### Key Metrics
- 9 LLM providers supported (8 cloud + 1 native via rig-core)
- 24 built-in media/file tools in 3 tiers
- 100+ MCP server aliases pre-configured
- 9 fetch extraction modes
- 31 pipe transform operations
- ~99.99% structured output compliance via 5-layer cascade
