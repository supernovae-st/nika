# Ecosystem and Vision: The Brain, The Body, and the Future of AI Orchestration

## Nika, NovaNet, and the Architecture of Democratized AI

Most AI tools are standalone. You install LangChain, you write chains, you call APIs, and the results live in your Python variables or your database. There is no larger system, no persistent knowledge, no learning across sessions. Each workflow execution starts from scratch, as if the system has amnesia.

The SuperNovae architecture is built on a fundamentally different premise: AI systems need both a brain and a body. The brain remembers things — entities, relationships, locale-specific knowledge, past results, learned patterns. The body does things — executes workflows, calls LLMs, processes media, fetches data. And the two communicate through a standard protocol that keeps them independent but connected.

NovaNet is the brain. Nika is the body. The Model Context Protocol (MCP) is the nervous system that connects them.

---

## NovaNet: The Knowledge Graph Brain

NovaNet is a knowledge graph engine built on Neo4j. It manages a rich data model with 47 nodes and 153 arcs, defining everything from business entities to locales to content pages to SEO keywords. The graph structure means relationships between concepts are first-class citizens, not afterthoughts. An entity like "QR Code AI" is connected to its locales (fr-FR, en-US), its pages (/home, /pricing), its keywords, its competitors, and its content history — all through typed arcs (relationships) that carry their own properties.

For the QR Code AI product — the company behind both Nika and NovaNet — this knowledge graph is the single source of truth. When a Nika workflow generates a French landing page, it does not start by asking the LLM "what do you know about QR codes in France?" It starts by querying NovaNet for the entity's French locale data, including the correct French expression ("code QR" rather than "QR code"), the appropriate register (formal "vous" for B2B audiences), content taboos (never use "gratuit" in headlines because it implies low quality), and market-specific data points.

This approach — grounding LLM generation in structured knowledge — is what separates Nika workflows from raw LLM calls. The LLM provides fluency and creativity; the knowledge graph provides accuracy and consistency. The combination produces output that is both eloquent and correct, which is something neither component can achieve alone.

---

## The Golden Rule: Knowing vs Doing

The relationship between Nika and NovaNet is governed by what the project calls "The Golden Rule" — five lines that define every architectural boundary:

Knowing things belongs to NovaNet. This means entities, locales, semantic relationships, knowledge atoms, and persistent data. If it is a fact that should survive beyond a single workflow execution, it belongs in NovaNet.

Doing things belongs to Nika. This means workflow execution, LLM calls, shell commands, HTTP requests, media processing, and artifact generation. If it is an action, it belongs in Nika.

Connecting is handled by MCP. The Model Context Protocol provides a clean JSON-RPC 2.0 interface between the two systems. Nika calls NovaNet's MCP tools (novanet_context, novanet_search, novanet_write) to read and write knowledge. NovaNet never calls Nika — the communication is always body-to-brain.

Thinking is the province of Records — compressed summaries of workflow execution that capture what was learned. Records bridge the gap between ephemeral workflow results and persistent knowledge.

Remembering is the flow from Records to NovaNet. When a workflow produces a valuable Record, it can be promoted from Nika's local warm storage (NDJSON files on disk) to NovaNet's cold storage (graph nodes with entity-linked semantic context). This promotion is what enables cross-session learning — the system gets smarter over time.

---

## The Zero Cypher Rule

One of the most distinctive architectural rules in the SuperNovae codebase is that Nika contains zero Cypher queries. Cypher is Neo4j's query language, and it is how NovaNet interacts with its graph database. But Nika never touches Cypher. It never connects to Neo4j directly. It never constructs graph queries.

Instead, all Nika-to-NovaNet communication goes through MCP tools. When a Nika workflow needs to look up an entity, it calls `invoke: novanet_context` with the entity key. When it needs to search for related knowledge, it calls `invoke: novanet_search`. When it needs to store a result, it calls `invoke: novanet_write`.

This zero-Cypher rule has profound architectural consequences. It means Nika and NovaNet can evolve independently — NovaNet can change its schema, optimize its queries, or even switch to a different graph database, and Nika workflows do not change. It means Nika can run without NovaNet entirely — workflows that do not need knowledge graph features simply do not call NovaNet tools. It means the security boundary is clean — Nika never has direct database access, so a compromised workflow cannot corrupt the knowledge graph.

---

## Three-Tier Memory Architecture

The memory architecture of the Nika ecosystem is designed in three tiers, inspired by how computer memory works (registers, cache, main memory) and by One Piece's Punk Records (the externalized brain of Dr. Vegapunk).

The HOT tier is the RunContext — an in-memory DashMap that holds task results during a single workflow execution. When task A completes and task B references `$task_A`, the result is retrieved from the RunContext. This data is ephemeral: it is created when the workflow starts and destroyed when it ends. In computer architecture terms, this is the register file.

The WARM tier is Punk Records — NDJSON (newline-delimited JSON) files stored locally on disk in `.nika/records/`. Records are compressed summaries of workflow executions, containing the task ID, a summary, key findings, the model used, tokens spent, and a confidence score. Records persist across sessions but are local to the machine. In computer architecture terms, this is the cache. The name "Punk Records" is directly from One Piece — it is the externalized memory system that Dr. Vegapunk uses to store knowledge beyond his biological brain's capacity.

The COLD tier is NovaNet — the knowledge graph where records are promoted when they prove valuable. A Record node in NovaNet is linked to its source entity (RECORD_OF arc), its locale (FOR_LOCALE arc), similar records (SIMILAR_TO arc), and its temporal predecessors (PRECEDED_BY arc). Records in NovaNet are permanent, queryable, and accessible across all future workflow executions. In computer architecture terms, this is main memory.

The promotion flow works like this: a workflow executes and produces Records (HOT to WARM). If a Record's confidence is high and it contains valuable knowledge, the orchestrator or the user promotes it to NovaNet (WARM to COLD). Future workflows can then recall relevant Records from NovaNet and use them as context for LLM prompts, creating a feedback loop where the system literally learns from its own past executions.

---

## Integration with 43+ AI Agents

One of Nika's most recent developments is its AI Integration Suite — a system designed so that when someone installs Nika, every AI coding tool on their machine instantly understands how to write `.nika.yaml` workflows.

The integration works through four tiers:

The first tier uses AGENTS.md files and Universal Agent Skills. AGENTS.md is an emerging standard (adopted by 60,000+ repositories and 20+ tools) that provides instructions to AI coding agents. Nika generates AGENTS.md files with skill definitions in `.agents/skills/` directories that teach AI tools the Nika workflow syntax, the five verbs, provider configuration, and common patterns. This tier reaches many AI agents including Claude Code, Cursor, GitHub Copilot, Gemini Code Assist, Windsurf, Roo Code, Cline, and others.

The second tier is deep integration with Claude Code with skills, agents, and MCP connectivity. This provides intelligent capabilities — Claude Code can run Nika workflows, validate YAML syntax, generate new workflows from natural language descriptions, and use Nika's LSP for intelligent completion.

The third tier generates native rules in each tool's format. For Cursor, this means `.cursor/rules/*.mdc` files. For VS Code Copilot, this means `.github/copilot-instructions.md`. For Windsurf, this means `.windsurfrules`. Each file is short (around 30 lines) and only generated for tools that are actually detected on the machine.

The fourth tier is an MCP server that exposes Nika's capabilities to any MCP-capable tool, plus an `llms.txt` file for web-based AI content discovery.

The setup is triggered by `nika setup`, which detects installed editors and AI tools, generates appropriate configuration files, installs the Nika VS Code extension, configures shell completions, and sets up git hooks for co-author attribution.

---

## The Open Source Philosophy

Nika is licensed under AGPL-3.0-or-later. This is not a casual choice — it is a philosophical statement about the future of AI tooling.

The AGPL requires that anyone who runs AGPL software over a network must make the source code available to users. In practical terms, this means a cloud provider cannot take Nika, wrap it in an API, charge money for it, and keep their modifications private. They must share their changes with the community.

The project's creator, Thibaut Melen, is an open source activist who frames the AI industry through the lens of One Piece's narrative. In this framing, the open source community is the pirate alliance fighting for freedom against the "World Government" of closed-source big tech. The AGPL is the weapon that ensures the pirates' tools remain free.

This is not just rhetoric — it is embedded in the project's identity. The SuperNovae ship (the project's metaphorical vessel) flies the Nika butterfly flag alongside the NovaNet brain flag. The hull is inscribed with the five verbs like sacred commandments. Blue butterflies — symbolizing courage, renewal, transformation, and new beginnings — swarm around the ship as the visual metaphor for liberation spreading through the AI ecosystem.

The project's symbol is a blue butterfly, chosen because butterflies represent transformation (caterpillar to butterfly, manual scripts to declarative workflows) and because they are impossible to contain (try catching all the butterflies). The Nika butterfly is the project's way of saying: this technology is free, it spreads freely, and no one can lock it up.

---

## Where This Is Going

The vision for Nika extends well beyond its current capabilities. The roadmap describes six priorities across three waves, designed to transform Nika from a workflow executor into an intelligent, memory-equipped orchestration system.

The first wave adds model routing and record compression. Model routing means workflows can use different LLM providers for different cognitive tasks within the same execution — a cheap fast model for data extraction, an expensive capable model for creative generation, a deep-thinking model for planning and review. Record compression means task results are summarized at completion boundaries, preventing the context window degradation that is the primary failure mode of long-running AI workflows.

The second wave adds orchestrate mode and context budget management. Orchestrate mode is perhaps the most ambitious planned feature: when given a `goal:` instead of explicit tasks, Nika's orchestrator dynamically generates, executes, evaluates, and improves `.nika.yaml` workflows to achieve the goal. The orchestrator plans in YAML — the plans are workflows themselves, making them deterministic, auditable, and reusable. Context budget management tracks token usage across the workflow and ensures each task receives only the context it needs, preventing the "dumb zone" where LLM performance degrades from context overload.

The third wave adds persistent memory and runtime introspection. Persistent memory implements the full three-tier architecture described above, with automatic promotion of valuable Records to NovaNet's knowledge graph. Runtime introspection provides six new built-in tools that let agents query the current workflow's runtime state — accumulated records, active threads, orchestration progress, cost reports, DAG structure, and individual task status.

The end state is a system where Nika workflows become smarter over time. Each execution produces Records. The best Records are promoted to the knowledge graph. Future executions recall relevant past experiences and use them as context. The knowledge graph grows richer. The workflows produce better results. The cycle continues.

---

## The Schema Version Strategy

Nika's schema versioning strategy is worth examining because it encodes a philosophy about evolution and compatibility.

Every workflow must declare its schema version on the first line: `schema: "nika/workflow@0.12"`. Features are gated by schema version — if you try to use structured output (a @0.11 feature) in a @0.10 workflow, the analyzer rejects it with error NIKA-149. This means workflows are explicitly versioned, and the system can enforce that features are used correctly.

The project has gone through twelve schema versions, from @0.1 (basic workflows) through @0.12 (vision, extraction, guardrails). Each version added features without breaking existing workflows — a @0.1 workflow still runs on Nika v0.42.0. But the project has an explicit policy of zero backward compatibility concern because it has zero users in production. This means schema versions can be cleaned up, aliases can be removed, and breaking changes can be made freely. Only @0.12 matters. This is a luxury of pre-release development that the project uses aggressively — instead of accumulating compatibility debt, it maintains a clean, consistent current version.

The version gating system also enables progressive learning. The course starts with @0.12 for all exercises, but the schema version mechanism means that different schema versions could theoretically be used to control feature availability. A future "learning mode" could gate advanced features behind higher schema versions, revealing capabilities as the learner progresses.

---

## The Knowledge Overhang Concept

One of the most fascinating theoretical concepts behind the Nika-NovaNet architecture is "knowledge overhang" — the idea that language models possess knowledge they cannot access without proper scaffolding.

Modern LLMs have been trained on vast corpora and "know" far more than they can express in any single conversation. The limiting factor is not the model's knowledge but the quality of the context provided. An LLM asked to "write a French landing page for QR Code AI" will produce generic marketing copy. The same LLM given context that includes the French expression "code QR" (not "QR code"), the B2B register preference for "vous" over "tu", the taboo against using "gratuit" in headlines, and specific market data about QR adoption in France will produce dramatically better output.

This is the knowledge overhang — the gap between what the model could produce with perfect context and what it actually produces with the context it receives. NovaNet's knowledge graph is designed to close this gap by providing structured, entity-specific, locale-aware context to every LLM call in a Nika workflow.

The planned three-tier memory system extends this concept across time. Cross-session Records provide scaffolding from past executions, activating latent capabilities that would otherwise be dormant. A workflow that generated a French landing page last month produced a Record with insights about French QR code marketing. When a similar workflow runs this month, that Record is recalled from NovaNet and injected as context, immediately activating the LLM's knowledge about French QR code marketing without having to rediscover it.

This is how the system gets smarter over time. Each execution produces Records. The best Records enter the knowledge graph. Future executions benefit from accumulated Records. The knowledge overhang shrinks with each iteration, and the system converges on expert-level output for its domain.

---

## The Bigger Picture: Democratizing AI

The ultimate vision behind the Nika-NovaNet ecosystem is the democratization of AI orchestration. Today, building sophisticated AI automation requires significant engineering expertise — you need to understand LLM APIs, manage context windows, handle structured output parsing, implement retry logic, set up observability, manage costs, and coordinate multiple providers. This is accessible to well-funded engineering teams but not to individual developers, small businesses, or domain experts who know what they want the AI to do but not how to make it happen.

Nika's answer is to capture all that complexity in a single binary and expose it through a simple, learnable YAML format. The five verbs provide the vocabulary. The binding system provides the data flow. The DAG provides the execution order. The provider abstraction handles the API complexity. The built-in tools handle media processing and file operations. The MCP protocol handles external integrations. The event system provides observability. The error codes provide debugging. The course provides learning. And the AGPL license ensures all of this remains free and open.

In the One Piece framing that animates the project: the One Piece — the treasure at the end of the Grand Line — is the democratization of AI itself. Not AI as a service controlled by a few corporations, but AI as a tool controlled by everyone who uses it. Nika is one ship in the fleet sailing toward that treasure. The butterfly flag flies. The five verbs are carved into the hull. The code is open. The journey continues.
