# Episode 7: The Brain and The Body -- How Nika Talks to NovaNet via MCP

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 07 |
| **Duration** | ~25 minutes |
| **Topics** | MCP protocol, NovaNet knowledge graph, invoke: verb, 100+ aliases, Zero Cypher Rule |
| **Guest Suggestions** | An MCP protocol contributor, a knowledge graph engineer, a Neo4j expert |
| **Audience** | Developers building integrated AI systems, protocol enthusiasts, graph database users |
| **Prerequisites** | Episode 2 (invoke: verb basics), Episode 3 (architecture overview) |

---

## Cold Open (30 seconds)

[MUSIC: Neural, connection-building -- like synapses firing]

**Host:** Your workflow scrapes the web, analyzes trends, and generates a report. That is today's intelligence. But what about last week's? Last month's? What if the workflow could remember what it learned, build on previous knowledge, and get smarter over time?

[PAUSE]

That requires two things: a body that executes, and a brain that remembers. In the SuperNovae ecosystem, Nika is the body. NovaNet is the brain. And the Model Context Protocol is the nervous system that connects them.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Episode 7. This is the episode where we zoom out from Nika as a standalone tool and look at it as part of a larger architecture.

Nika does not exist in isolation. It is one half of a dual system -- the execution engine that pairs with NovaNet, a knowledge graph built on Neo4j. Together, they form a system where workflows can execute complex AI tasks AND build persistent, queryable knowledge from their results.

And the protocol that connects them -- MCP, the Model Context Protocol -- is becoming the standard for how AI tools communicate. Understanding how Nika uses MCP is understanding the future of AI interoperability.

Let us start with the protocol itself.

---

## Segment 1: The Model Context Protocol (8 minutes)

**Host:** MCP -- the Model Context Protocol -- is an open protocol developed by Anthropic for connecting AI applications to external tools and data sources. Think of it as a universal adapter: any MCP client can talk to any MCP server, regardless of who built either side.

The protocol works like this:

1. An MCP server declares its capabilities -- what tools it offers, what each tool does, and what parameters it accepts.
2. An MCP client connects, discovers available tools, and can call them.
3. Tool calls are JSON-RPC messages with structured inputs and outputs.

[CODE EXAMPLE]
```
MCP Client (Nika)                    MCP Server (NovaNet)
     |                                      |
     |--- initialize ---------------------->|
     |<-- capabilities (tools list) --------|
     |                                      |
     |--- tools/call (novanet_search) ----->|
     |<-- result (matching nodes) ----------|
     |                                      |
     |--- tools/call (novanet_write) ------>|
     |<-- result (confirmation) ------------|
```

Nika's MCP client is the nika-mcp crate -- 9,000 lines of Rust covering:

**Connection Management.** MCP servers can be spawned as child processes (stdio transport) or connected to via SSE (Server-Sent Events). Nika manages a connection pool so multiple tasks can share a single server connection.

**Retry with Backoff.** MCP calls can fail due to network issues, server overload, or transient errors. Nika implements retry with exponential backoff and jitter. The `McpRetryConfig` specifies max retries, initial delay, and backoff factor.

**Automatic Reconnection.** If a server connection drops, Nika automatically reconnects within a configurable timeout (default 30 seconds).

**Response Caching.** For tool calls that return the same result for the same inputs (like schema lookups), Nika caches responses with a configurable TTL.

**Schema Validation.** Before sending a tool call, Nika validates the arguments against the tool's declared JSON schema. This catches parameter errors before they reach the server.

[CODE EXAMPLE]
```yaml
# Calling an MCP tool from a workflow
- id: search_knowledge
  invoke:
    tool: novanet_search
    args:
      query: "AI workflow engines"
      limit: 10
      locale: "en-US"
```

The `invoke:` verb is how Nika calls MCP tools. But there is a routing layer in between: the BuiltinToolRouter. When you call a tool, Nika first checks if it is a built-in tool (prefixed with `nika:`). If it is, the call is handled locally. If not, it is routed to the appropriate MCP server.

This routing is transparent to the workflow author. You write `invoke: { tool: nika:thumbnail, args: {...} }` or `invoke: { tool: novanet_search, args: {...} }` and the engine figures out where to send the call.

---

## Segment 2: NovaNet -- The Brain (8 minutes)

**Host:** NovaNet is the other half of the SuperNovae architecture. It is a knowledge graph built on Neo4j -- a graph database where information is stored as nodes and relationships.

Let me draw you the high-level architecture:

[CODE EXAMPLE]
```
NovaNet (Brain)              MCP Protocol             Nika (Body)
+-- Knowledge Graph   <-------------------------->  +-- YAML Workflows
|   +-- NodeClasses                                  +-- 5 Verbs
|   +-- ArcClasses                                   +-- DAG Execution
|   +-- MCP Tools                                    +-- Inference Backends
```

NovaNet stores structured knowledge: entities (people, products, concepts), pages (web content), SEO data, locale-specific information, and accumulated intelligence from workflow runs. It exposes this knowledge through MCP tools that Nika can call via `invoke:`.

**The Three-Tier Memory Architecture**

In Episode 1, I mentioned the three tiers. Let me go deeper.

**HOT: RunContext** -- This is an in-memory DashMap (concurrent HashMap) that lives during a single workflow run. Every task result is stored here. Bindings resolve against this store. When the workflow finishes, the RunContext is destroyed.

**WARM: Records** -- After a run completes, key results can be compressed and written as NDJSON files to `.nika/records/`. These are timestamped, run-scoped, and retained for a configurable period (7 days to 90 days to forever). They are local to the machine.

**COLD: NovaNet** -- The most valuable insights are promoted to the permanent knowledge graph. Once in NovaNet, information is queryable across projects, across locales, and across time. It becomes institutional knowledge.

[CODE EXAMPLE]
```yaml
# A workflow that builds knowledge over time
tasks:
  # Fetch fresh data
  - id: scrape_trends
    fetch:
      url: https://news.ycombinator.com/
      extract: links

  # Analyze with LLM
  - id: analyze
    depends_on: [scrape_trends]
    with:
      links: "$scrape_trends"
    infer: "Identify the top 3 AI trends from: {{with.links}}"
    structured:
      schema:
        type: object
        properties:
          trends:
            type: array
            items:
              type: object
              properties:
                name: { type: string }
                evidence: { type: string }
              required: [name, evidence]

  # Store in NovaNet for long-term memory
  - id: persist
    depends_on: [analyze]
    with:
      trends: "$analyze"
    invoke:
      tool: novanet_write
      args:
        type: "TrendAnalysis"
        data: "{{with.trends | to_json}}"
        locale: "en-US"
        date: "{{env.TODAY}}"
```

[EMPHASIS] This workflow does not just analyze trends -- it builds a historical record. Next week, you can query NovaNet for all trend analyses and spot patterns across time. The workflow gets smarter not because the LLM improves, but because the knowledge accumulates.

**The Zero Cypher Rule**

[PAUSE]

This is one of the most important architectural rules in the SuperNovae ecosystem, and it deserves a clear explanation.

**Nika workflows NEVER use raw Cypher.** Cypher is Neo4j's query language. It is powerful, but it is also a direct database access mechanism. If Nika workflows could write arbitrary Cypher, you would have:

1. Tight coupling between Nika and Neo4j's query language
2. SQL-injection-equivalent attacks via template interpolation
3. No abstraction boundary -- the workflow author needs to know the graph schema

Instead, all NovaNet communication goes through MCP tools. The tools provide a high-level, validated API: `novanet_search`, `novanet_write`, `novanet_context`. The tools handle Cypher internally, with parameterized queries that prevent injection.

This is the same principle as using an ORM instead of raw SQL -- except enforced at the protocol level. Nika literally cannot send Cypher to Neo4j because it has no database driver. It only has an MCP client.

---

## Segment 3: 100+ MCP Aliases and the Tool Ecosystem (6 minutes)

**Host:** Beyond NovaNet, Nika comes pre-configured with 100+ MCP server aliases. These are shortcuts for popular services and tools.

[CODE EXAMPLE]
```yaml
# Instead of configuring the MCP server manually:
mcp_servers:
  github:
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "{{env.GITHUB_TOKEN}}"

# You just write:
- id: create_pr
  invoke:
    tool: github:create_pull_request
    args:
      repo: "supernovae/nika"
      title: "Automated PR from Nika workflow"
```

The alias system maps friendly names to MCP server configurations. Nika ships with aliases for:

- **Code platforms** -- GitHub, GitLab, Bitbucket
- **Communication** -- Slack, Discord, Email
- **Content** -- Notion, Confluence
- **Cloud** -- AWS, GCP, Azure
- **Databases** -- PostgreSQL, MongoDB
- **AI services** -- Replicate, ComfyUI, FAL (image generation), ElevenLabs (TTS), DeepL (translation)
- **Browser** -- Puppeteer, Playwright
- **Utilities** -- File system, Sequential thinking

The alias catalog is defined in nika-core, meaning it is part of the zero-I/O core. Adding a new alias is a data change, not a code change.

**MCP Inline Configuration**

For servers not in the alias catalog, workflows can define MCP servers inline:

[CODE EXAMPLE]
```yaml
schema: nika/workflow@0.12

mcp_servers:
  my_custom_server:
    command: "./my-server"
    args: ["--port", "3000"]
    env:
      API_KEY: "{{env.MY_API_KEY}}"
    cwd: "/path/to/server"

tasks:
  - id: call_custom
    invoke:
      tool: my_custom_server:do_something
      args:
        input: "data"
```

The MCP server is spawned as a child process when the workflow starts, and terminated when it finishes. The `env:` block passes environment variables, and `cwd:` sets the working directory.

**Smart Router Pattern**

Nika implements a Smart Router for tool dispatch. When a tool call comes in, the router follows a priority chain:

1. **Builtin** -- Is this a `nika:*` tool? Handle locally with zero network overhead.
2. **MCP** -- Is there a connected MCP server with this tool? Route to the server.
3. **LLM Fallback** -- Can the LLM handle this request through conversation? Use inference.
4. **Error** -- No handler found. Return a clear error with suggestions.

This routing is transparent to the workflow author and to agents. An agent does not need to know whether a tool is built-in or remote -- it just calls the tool and gets a result. The router handles the dispatch.

This pattern is particularly powerful for gradual migration. You might start with an MCP server for image generation (ComfyUI, Replicate). Later, if you add a local image generation capability, you can route those calls to the builtin handler without changing any workflow YAML. The Smart Router absorbs the complexity.

**Connection Lifecycle**

The MCP client pool manages the full lifecycle of server connections:

1. **Spawn** -- When a workflow references an MCP server (via alias or inline config), the pool spawns the server process.
2. **Initialize** -- The client sends an `initialize` message and receives the server's capabilities (tool list, protocol version).
3. **Call** -- Tool calls are routed through the pool, which handles concurrency, timeouts, and retries.
4. **Reconnect** -- If the connection drops mid-workflow, the pool attempts automatic reconnection within 30 seconds.
5. **Shutdown** -- When the workflow completes, all spawned servers are gracefully terminated.

Timeouts are explicit and configurable: 20 seconds for initial connection, 60 seconds for individual tool calls, 30 seconds for reconnection attempts.

[PAUSE]

**Host:** The MCP ecosystem is growing rapidly. Every new MCP server that anyone publishes is immediately usable in Nika workflows via `invoke:`. This is the power of standardized protocols -- Nika does not need to add custom integrations for each service. If a service has an MCP server, Nika can call it.

And the inverse is true too: Nika's 24 built-in tools could be exposed AS an MCP server, making them available to any MCP client -- Claude Code, Cursor, Windsurf, or any other tool that speaks MCP. The protocol is bidirectional.

---

## Segment 4: The Bigger Picture -- Why Brain+Body Matters (2 minutes)

**Host:** Let me step back and explain why this architecture matters for the future of AI.

Most AI tools today are stateless. You send a prompt, you get a response, the context is gone. Even "memory" features in chat interfaces are shallow -- they store facts, not structured knowledge.

The Nika+NovaNet architecture proposes something different: workflows that build knowledge incrementally. Each run adds to the graph. Each analysis enriches the context. Over time, the system does not just execute tasks -- it accumulates understanding.

Imagine a content team running weekly trend analysis workflows. After six months, NovaNet contains a knowledge graph of trend evolution -- which topics emerged, which faded, which connected to other domains. A new workflow can query this history and produce analysis that no single LLM call could generate, because it has context that spans months.

[EMPHASIS] This is the vision: the body executes, the brain remembers, and the protocol ensures they stay loosely coupled. You can upgrade Nika without touching NovaNet. You can switch LLM providers without changing the knowledge graph. You can replace Neo4j with a different graph database as long as it speaks MCP.

That loose coupling through a standard protocol is not just good engineering. It is a statement about how AI systems should be built: composable, replaceable, and open.

---

## Wrap-up & Preview (2 minutes)

**Host:** Let me summarize.

MCP is the standard protocol for AI tool communication. Nika uses it as both a client (calling external tools) and a bridge to NovaNet (the knowledge graph).

The Zero Cypher Rule enforces clean separation: Nika never touches the database directly, always through MCP tools with parameterized queries.

100+ pre-configured aliases make it easy to connect to popular services. Inline MCP server configuration handles custom servers.

And the three-tier memory architecture (HOT RunContext, WARM Records, COLD NovaNet) enables workflows that build persistent knowledge over time.

[PAUSE]

Final episode next time. We are talking about the future: why Nika uses AGPL (and what that means for open source), integration with 43+ AI coding tools, the One Piece philosophy of liberation through open source, and what the roadmap looks like. Episode 8: "The Future of AI Workflows."

[MUSIC: Outro theme]

---

## Show Notes

### MCP Concepts
- **MCP** -- Model Context Protocol (Anthropic, open standard)
- **MCP Client** -- Nika (nika-mcp crate, rmcp v0.16)
- **MCP Server** -- Any tool that speaks MCP (NovaNet, GitHub, Slack, etc.)
- **Tool Discovery** -- Servers declare capabilities, clients discover them
- **JSON-RPC** -- Underlying message format for MCP communication

### NovaNet Integration
| MCP Tool | Purpose |
|----------|---------|
| `novanet_search` | Query the knowledge graph |
| `novanet_write` | Add nodes/relationships |
| `novanet_context` | Retrieve contextual knowledge |

### Three-Tier Memory
| Tier | Store | Lifetime | Technology |
|------|-------|----------|------------|
| HOT | RunContext | One workflow run | DashMap (in-memory) |
| WARM | Records | 7-90 days | NDJSON (disk) |
| COLD | NovaNet | Permanent | Neo4j (via MCP) |

### MCP Client Features (nika-mcp, 9K lines)
- Connection pooling (multiple tasks share connections)
- Retry with exponential backoff + jitter
- Automatic reconnection (30s timeout)
- Response caching with TTL
- Schema validation before calls
- rmcp v0.16 adapter
- Inline server configuration (command + args + env + cwd)

### Architectural Rules
- **Zero Cypher Rule** -- Nika NEVER sends raw database queries
- **MCP Only** -- All NovaNet communication via MCP tools
- **Loose Coupling** -- Nika and NovaNet are separate git repos, separate deployments
- **Protocol Boundary** -- The MCP protocol IS the API contract

### Pre-Configured Alias Categories
- Code platforms (GitHub, GitLab, Bitbucket)
- Communication (Slack, Discord, Email)
- Content (Notion, Confluence)
- Cloud (AWS, GCP, Azure)
- Databases (PostgreSQL, MongoDB)
- AI services (Replicate, ComfyUI, FAL, ElevenLabs, DeepL)
- Browser automation (Puppeteer, Playwright)
- Utilities (file system, sequential thinking)
