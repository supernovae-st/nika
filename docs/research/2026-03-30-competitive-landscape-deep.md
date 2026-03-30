# Competitive Landscape Deep Analysis: Nika

**Date:** 2026-03-30
**Scope:** 11 direct competitors, emerging tools, claim verification, market positioning
**Tools used:** Perplexity sonar-pro (12 queries), GitHub API (live star counts), prior research cross-reference
**Confidence:** High for star counts and funding (verified via GitHub API + multiple sources). Medium for download/revenue figures (incomplete public data).

---

## Executive Summary

Nika competes in a market dominated by massively funded Python/TypeScript frameworks (LangChain at $260M raised, Dify at $30M+) and GUI-first platforms (n8n at 182k stars, Dify at 135k stars). The top 5 competitors each have 10,000x to 45,000x more GitHub stars than Nika's current 4.

However, Nika occupies a genuinely uncontested niche: **no other tool is a single Rust binary, YAML-native, CLI-first AI workflow engine with DAG execution, MCP client support, integrated TUI, 8 cloud providers + local GGUF, and content-addressable media storage.** Each individual feature has competitors, but the combination is unique. The challenge is not differentiation -- it is distribution and awareness.

The market is consolidating around two poles:
1. **Python/TypeScript agent frameworks** (LangChain, CrewAI, AutoGen, Mastra) -- code-first, library-oriented
2. **Visual low-code platforms** (Dify, Flowise, n8n) -- GUI-first, drag-and-drop

Nika sits in an underserved third category: **declarative CLI tools for AI workflows** -- analogous to Terraform, Docker Compose, or GitHub Actions, but for AI tasks. No funded competitor occupies this space.

---

## 1. Direct Competitor Analysis

### Tier 1: Dominant Frameworks (100k+ stars)

#### LangChain
| Metric | Value |
|--------|-------|
| GitHub | langchain-ai/langchain |
| Stars | **131,619** (live, 2026-03-30) |
| Version | v1.0.2 (GA October 2025) |
| Language | Python, TypeScript |
| Funding | **$260M total** ($10M seed Benchmark Apr 2023, $25M Series A Sequoia Feb 2024, $100M Series B IVP Jul 2025, $125M Series B-2 IVP Oct 2025) |
| Valuation | **$1.25 billion** |
| ARR | >$12-16M (Oct 2025, not yet profitable) |
| Last push | 2026-03-30 (daily activity) |

**Key features:** Agent engineering platform, chains, retrievers, tool integration, LangSmith observability, LangServe deployment, 700+ integrations.

**Limitations:**
- Requires Python runtime, pip install, dependency management
- Heavy abstraction layers criticized as over-engineering
- No YAML-native workflow definition (code-only)
- No built-in TUI, no media pipeline, no local GGUF inference
- Enterprise features (LangSmith) are paid SaaS

**Relevance to Nika:** LangChain is the "default" but serves a fundamentally different user. LangChain users write Python code; Nika users write YAML config. They are complementary rather than directly competitive. LangChain's scale ($1.25B valuation) validates the market but its complexity creates an opening for simpler declarative tools.

---

#### Dify
| Metric | Value |
|--------|-------|
| GitHub | langgenius/dify |
| Stars | **134,998** (live, 2026-03-30) |
| Version | Active development (no single version -- SaaS + self-hosted) |
| Language | Python, TypeScript |
| Funding | **$30M+ Series Pre-A** at $180M valuation |
| Users | 2,000+ teams, 280 enterprises (Maersk, Novartis), 1.4M machines |
| Last push | 2026-03-30 (daily activity) |

**Key features:** Visual AI workflow builder, RAG pipelines, agent capabilities, 50+ built-in tools, model management, prompt IDE, MCP server support, observability.

**Limitations:**
- GUI-first (visual drag-and-drop), not CLI-native
- Requires Docker/cloud deployment
- Python/TypeScript runtime
- No single-binary distribution
- Workflows not designed to live in git as diffable YAML

**Relevance to Nika:** Dify is the closest competitor in feature scope (agents, RAG, tools, multi-provider) but targets a completely different user persona (visual builders, product teams). Nika targets CLI-native developers who want workflows in git. Dify supports MCP but as a server, not client.

---

#### n8n
| Metric | Value |
|--------|-------|
| GitHub | n8n-io/n8n |
| Stars | **181,697** (live, 2026-03-30) -- highest of all competitors |
| Language | TypeScript |
| Funding | $68M+ ($56M Series B 2022, earlier rounds from Sequoia, Felicis) |
| Pricing | Self-hosted free, cloud $20/2500 exec |
| Last push | 2026-03-30 (daily activity) |

**Key features:** Fair-code workflow automation, 400+ integrations, ~70 AI nodes with native LangChain integration, visual builder, self-hosted option, local LLM support via LangChain nodes.

**Limitations:**
- GUI-first visual builder (JSON workflow definitions internally, not YAML)
- Requires Node.js runtime
- AI features are bolted onto a general automation platform, not AI-native
- No DAG-aware execution for AI tasks, no structured output validation
- No MCP client, no media pipeline, no content-addressable storage

**Relevance to Nika:** n8n dominates workflow automation but is not AI-native. Its AI nodes are wrappers around LangChain. For pure AI workflow orchestration (inference, structured output, agent loops), Nika is purpose-built. n8n's 182k stars show massive demand for workflow automation -- Nika could capture the AI-specific subset that wants CLI-native tools.

---

### Tier 2: Major Agent Frameworks (20k-60k stars)

#### AutoGen (Microsoft)
| Metric | Value |
|--------|-------|
| GitHub | microsoft/autogen |
| Stars | **56,444** (live, 2026-03-30) |
| Language | Python, .NET |
| Funding | Microsoft-backed (no separate funding) |
| Last push | 2026-03-29 (active) |

**Key features:** Multi-agent conversation framework, conversational patterns, group chat, code execution, human-in-the-loop.

**Limitations:**
- Python runtime required
- Conversation-based (not DAG/workflow-based)
- No YAML workflow definitions
- Limited MCP support
- No CLI tool, no TUI, no media pipeline

**Relevance to Nika:** AutoGen focuses on multi-agent conversations, not structured workflows. Different paradigm entirely. Nika's `agent:` verb with guardrails and completion modes is more controlled than AutoGen's open-ended conversations.

---

#### CrewAI
| Metric | Value |
|--------|-------|
| GitHub | crewAIInc/crewAI |
| Stars | **47,578** (live, 2026-03-30) |
| Language | Python |
| Funding | **$18M total** ($5.5M seed boldstart, $12.5M Series A Insight Partners Oct 2024) |
| Investors | Andrew Ng, Dharmesh Shah, Craft Ventures |
| Last push | 2026-03-30 (active) |

**Key features:** Role-based agent orchestration, crew metaphor, sequential/hierarchical/consensual processes, model-agnostic, <20 lines to define a crew.

**Limitations:**
- Python runtime required
- Agent-only (no general workflow verbs like exec, fetch)
- No YAML-native definitions (Python decorators/classes)
- No media pipeline, no CAS, no TUI
- No MCP client support

**Relevance to Nika:** CrewAI popularized the "crew of agents" metaphor. Nika's DAG with `agent:` tasks can replicate CrewAI patterns but adds structured workflows (exec, fetch, invoke) that CrewAI lacks. CrewAI is agents-only; Nika is workflows-that-include-agents.

---

#### LangGraph
| Metric | Value |
|--------|-------|
| GitHub | langchain-ai/langgraph |
| Stars | **27,935** (live, 2026-03-30) |
| Language | Python |
| Funding | Part of LangChain ($260M) |
| Last push | 2026-03-29 (active) |

**Key features:** Stateful multi-actor agent graphs, cyclic execution, persistent state, human-in-the-loop, built on LangChain.

**Limitations:**
- Requires LangChain ecosystem
- Python runtime, code-only (no YAML definition)
- Heavyweight dependency chain
- Enterprise features via LangSmith (paid SaaS)
- No CLI-native execution, no TUI

**Relevance to Nika:** LangGraph is the closest in concept (graph-based workflow execution) but implemented as a Python library, not a CLI tool. Nika's DAG execution is conceptually similar but declarative (YAML) vs imperative (Python code).

---

#### Haystack (deepset)
| Metric | Value |
|--------|-------|
| GitHub | deepset-ai/haystack |
| Stars | **24,656** (live, 2026-03-30) |
| Version | **v2.26.1** (2026-03-20) |
| Language | Python |
| Funding | deepset raised ~$30M Series A (unconfirmed exact figure) |
| Last push | 2026-03-30 (active) |

**Key features:** Open-source AI orchestration framework, modular component system, pipeline graphs, RAG-focused, production-ready.

**CRITICAL UPDATE:** Haystack 2.x **abandoned YAML for pipeline definitions**. Pipelines are now defined in pure Python code. YAML is only used for serialization (save/load). This means Haystack is no longer a "YAML pipeline tool" -- it is a Python-first framework.

**Limitations:**
- Python-first (YAML for serialization only, not definition)
- RAG/search focused, not general AI workflow
- No shell exec, no MCP client, no media pipeline
- No CLI tool, no TUI
- Requires Python runtime and pip

**Relevance to Nika:** Haystack was the closest "YAML pipeline" competitor but moved away from YAML-first definitions in v2.x. This strengthens Nika's claim to being the only YAML-native AI workflow engine. Haystack is now firmly in the Python-first camp.

---

### Tier 3: Emerging Frameworks (15k-25k stars)

#### Mastra
| Metric | Value |
|--------|-------|
| GitHub | mastra-ai/mastra |
| Stars | **22,471** (live, 2026-03-30) |
| Language | TypeScript |
| Team | From the team behind Gatsby.js |
| Last push | Active |

**Key features:** TypeScript-first AI framework, agents, RAG, workflows, memory, streaming, playground, evals, MCP support, model routing, structured output streaming.

**Relevance to Nika:** Mastra is a TypeScript library, not a CLI tool. It supports MCP but as a TypeScript integration, not a standalone client. Growing fast (22k stars) but targets Node.js developers, not CLI-native users.

---

#### OpenAI Agents SDK
| Metric | Value |
|--------|-------|
| GitHub | openai/openai-agents-python |
| Stars | **20,418** (live, 2026-03-30) |
| Language | Python |

**Key features:** Lightweight multi-agent framework from OpenAI directly.

**Relevance to Nika:** OpenAI-specific, Python-only. Not a workflow engine.

---

#### Google ADK (Agent Development Kit)
| Metric | Value |
|--------|-------|
| GitHub | google/adk-python |
| Stars | **18,662** (live, 2026-03-30) |
| Language | Python |

**Key features:** Code-first Python toolkit for building, evaluating, deploying AI agents. Google ecosystem integration.

**Relevance to Nika:** Google-specific, Python-only. Not a workflow engine.

---

### Tier 4: Visual Builders and Adjacent Tools

#### Flowise
| Metric | Value |
|--------|-------|
| GitHub | FlowiseAI/Flowise |
| Stars | **51,248** (live, 2026-03-30) |
| Version | v3.1.1 (2026-03-23) |
| Language | TypeScript |
| Last push | 2026-03-30 (active) |

**Key features:** Visual AI agent builder, drag-and-drop, LangChain-based, chatflow/agentflow, self-hosted.

**Limitations:** GUI-only, no CLI, no YAML definitions, Node.js runtime.

---

#### Rivet (Ironclad)
| Metric | Value |
|--------|-------|
| GitHub | Ironclad/rivet |
| Stars | **4,525** (live, 2026-03-30) |
| Language | TypeScript + Rust (Tauri) |
| Last push | **2026-03-20** BUT last meaningful commit was **2025-10-06** (only dependabot/community PRs since) |
| Status | **Effectively abandoned** -- no core team commits since Oct 2025 |

**Key features:** Visual AI programming environment, node graph editor, TypeScript library.

**Relevance to Nika:** Rivet validates the "visual AI workflow" concept but appears abandoned. Its Tauri (Rust) wrapper is superficial -- the core is TypeScript. Not a real competitor.

---

#### RIG (0xPlaygrounds)
| Metric | Value |
|--------|-------|
| GitHub | 0xPlaygrounds/rig |
| Stars | **6,712** (live, 2026-03-30) |
| Language | **Rust** |
| Last push | 2026-03-29 (active) |

**Key features:** Modular LLM applications in Rust, multi-provider support, embeddings, vector stores, agent workflows, type-safe, async.

**Relevance to Nika:** RIG is the closest Rust-based competitor. However, RIG is a **library** (you write Rust code using it), not a **CLI tool** (you write YAML and run it). Nika actually uses rig-core internally for provider abstraction. They are complementary: RIG is to Nika what reqwest is to curl.

---

### Tier 5: Infrastructure Orchestrators (not AI-native)

#### Temporal
| Metric | Value |
|--------|-------|
| GitHub | temporalio/temporal |
| Stars | **19,229** (live, 2026-03-30) |
| Funding | $103M Series B (2023) |
| Language | Go |

**Key features:** Durable workflow execution, fault tolerance, long-running processes, enterprise-grade.

**Relevance to Nika:** Temporal is workflow infrastructure, not AI-specific. No LLM verbs, no structured output, no media pipeline. Could theoretically be used to orchestrate AI tasks but requires writing Go/Java/Python code. Different level of abstraction.

---

#### Prefect
| Metric | Value |
|--------|-------|
| GitHub | PrefectHQ/prefect |
| Stars | **21,995** (live, 2026-03-30) |
| Language | Python |

**Key features:** Data pipeline orchestration, Python-native, scheduling, observability.

**Relevance to Nika:** Data pipeline tool, not AI workflow engine. No LLM primitives, no structured output, no agent loops. Would require writing Python code wrapping LLM calls.

---

#### Dagster
| Metric | Value |
|--------|-------|
| GitHub | dagster-io/dagster |
| Stars | **15,167** (live, 2026-03-30) |
| Language | Python |

**Key features:** Data orchestration platform, software-defined assets, type system, testing.

**Relevance to Nika:** Similar to Prefect -- data pipeline tool. Not AI-native. No overlap with Nika's feature set.

---

## 2. Emerging Competitors

### Rust-Based AI Tools
The Rust AI ecosystem is growing but focused on **libraries, not tools:**
- **RIG** (6.7k stars) -- Rust LLM library (Nika uses it internally)
- **Candle** (Hugging Face) -- Rust ML inference framework
- **rustformers** -- Rust transformer implementations
- **rust-genai** -- Unified Rust API for AI providers
- **Hebbs** -- Rust memory engine for AI agents
- **Mule AI** -- Go-based (not Rust) AI agent framework

**Finding: No other Rust-based AI workflow CLI tool exists.** Nika is alone in this specific niche.

### YAML-Based AI Workflow Tools
- **Haystack** -- moved AWAY from YAML in v2.x (Python-first now)
- **PromptFlow** (Microsoft) -- YAML-based but Azure-locked
- **Kestra** -- YAML workflow engine but AI features are bolted-on, not native
- **CloudSlang** -- YAML process automation, not AI-focused

**Finding: No YAML-native AI workflow engine exists besides Nika and (partially) PromptFlow.** Haystack's departure from YAML strengthens Nika's position.

### "Daggr" from Gradio Team
**Finding: Does not exist.** No evidence of any tool called "Daggr" from the Gradio team in any source. This appears to be a fabrication or confusion.

### CLI-First AI Orchestration Tools
**Finding: This category is essentially empty.** All major AI tools are either:
- Python/TypeScript libraries (LangChain, CrewAI, Mastra)
- GUI platforms (Dify, Flowise, n8n)
- IDE plugins (Cursor, GitHub Copilot)

No funded competitor offers a standalone CLI binary for AI workflow execution. Claude Code and similar tools are chat interfaces, not workflow engines.

---

## 3. Claim Verification

### Claim 1: "Only YAML-native AI workflow engine as a single binary"

**VERDICT: TRUE**

Evidence:
- Haystack 2.x abandoned YAML definitions (Python-first now)
- PromptFlow uses YAML but requires Python runtime + Azure SDK (not a single binary)
- Kestra uses YAML but requires JVM (not a single binary, not AI-native)
- n8n uses JSON internally, not YAML
- No other tool compiles to a single binary with YAML workflow definitions for AI tasks

**Counter-examples:** None found. PromptFlow is the closest but fails the "single binary" test.

---

### Claim 2: "First non-IDE CLI tool implementing MCP client"

**VERDICT: LIKELY TRUE (with caveats)**

Evidence:
- Confirmed MCP clients: Claude Desktop, ChatGPT, Cursor, VS Code, Zed, MCPJam
- All known MCP clients are either AI assistants (Claude, ChatGPT) or IDEs (Cursor, VS Code, Zed)
- LangChain/LangGraph/Mastra can consume MCP servers via adapters but are libraries, not CLI tools
- No standalone CLI tool was found that implements MCP client protocol natively

**Caveats:**
- The MCP ecosystem is moving fast; new clients appear frequently
- Some CLI tools may implement MCP without being documented
- "First" claims are inherently hard to verify

---

### Claim 3: "Only AI tool with integrated TUI"

**VERDICT: PARTIALLY TRUE**

Evidence:
- No AI workflow engine was found with a full interactive TUI (panels, live updates, keyboard navigation)
- Some tools have CLI output (progress bars, streaming) but not a full TUI
- Ollama has a CLI but no TUI
- lazygit-style TUIs exist for git but not for AI workflows

**Caveats:**
- Some LLM chat tools (e.g., chatgpt-cli, aichat) have TUI-like interfaces but are chat tools, not workflow engines
- The claim is valid for workflow engines specifically, but not for all AI tools

---

### Claim 4: "Only tool combining 8 cloud providers + local GGUF in one binary"

**VERDICT: TRUE (for CLI tools)**

Evidence:
- LangChain supports many providers but requires Python + pip install per provider
- Dify supports multiple providers but requires Docker deployment
- No single binary tool was found that ships with 8 cloud provider clients + local GGUF inference built-in
- rust-genai supports multiple providers but is a library, not a tool
- Ollama supports local models but not cloud providers

**Counter-examples:** None for single-binary distribution. LangChain/Dify match on provider count but not on distribution model.

---

### Claim 5: "Content-addressable storage for AI media workflows"

**VERDICT: TRUE (unique in AI workflow space)**

Evidence:
- No AI workflow tool was found using CAS for media asset management
- CAS is well-established in version control (git) and container registries (OCI) but not in AI tooling
- Dify and n8n handle media as file uploads, not content-addressed blobs
- The combination of CAS + media pipeline (thumbnail, convert, optimize) + LLM inference in one tool is unique

**Counter-examples:** None found.

---

## 4. Competitive Matrix

| Feature | Nika | LangChain | Dify | CrewAI | n8n | Flowise | Haystack | Temporal |
|---------|------|-----------|------|--------|-----|---------|----------|----------|
| Stars | 4 | 131k | 135k | 48k | 182k | 51k | 25k | 19k |
| Language | Rust | Python | Py/TS | Python | TS | TS | Python | Go |
| Single binary | YES | No | No | No | No | No | No | Yes |
| YAML-native | YES | No | No | No | No | No | No | No |
| CLI-first | YES | No | No | No | No | No | No | No |
| TUI | YES | No | No | No | No | No | No | No |
| GUI | No | LangSmith | YES | No | YES | YES | No | Web UI |
| MCP client | YES | Adapter | Server | No | No | No | No | No |
| DAG execution | YES | LangGraph | YES | Seq/Hier | YES | YES | YES | YES |
| LLM verbs | 5 native | Library | Visual | Agent-only | Nodes | Nodes | Components | None |
| Structured output | 5-layer | Basic | Basic | No | No | No | No | N/A |
| Agent loops | YES | LangGraph | YES | YES | No | YES | No | No |
| Media pipeline | YES (30+ tools) | No | No | No | No | No | No | No |
| CAS storage | YES | No | No | No | No | No | No | No |
| Local GGUF | YES | Via Ollama | Via Ollama | Via Ollama | Via LangChain | Via Ollama | No | No |
| Cloud providers | 8 built-in | 700+ (pip) | 10+ | Model-agnostic | Via nodes | Via nodes | 5+ | N/A |
| Learning course | 12 levels | Docs | Docs | Docs | Academy | Docs | Tutorials | Docs |
| Pricing | Free (AGPL) | Free + paid | Free + paid | Free + paid | Free + paid | Free | Free | Free + paid |
| Funding | $0 | $260M | $30M+ | $18M | $68M+ | Unknown | ~$30M | $103M |

---

## 5. Market Positioning Analysis

### "Terraform for AI" Positioning

**Finding: No tool currently claims this positioning.** Terraform itself is evolving toward AI infrastructure management (via MCP servers for HCP), but no tool occupies the "declarative AI workflow definition" space the way Terraform occupies "declarative infrastructure."

**Validity assessment:** The analogy is strong:

| Terraform | Nika |
|-----------|------|
| HCL files | .nika.yaml files |
| Infrastructure providers (AWS, GCP, Azure) | LLM providers (Anthropic, OpenAI, Gemini) |
| terraform plan | nika check |
| terraform apply | nika run |
| State management | CAS + artifact tracking |
| Provider ecosystem | MCP server ecosystem |
| Single binary (Go) | Single binary (Rust) |

**Risk:** "Terraform for AI" could be misunderstood as "infrastructure provisioning for AI workloads" (which is what actual Terraform does). A clearer positioning might be **"Docker Compose for AI workflows"** or **"GitHub Actions for AI tasks"** -- these better communicate the declarative, composable nature.

### Target Persona

Based on the competitive gap analysis, Nika's ideal user is:

1. **DevOps/Platform engineers** who already use Terraform, Docker Compose, GitHub Actions -- they think in YAML, prefer CLI tools, want workflows in git
2. **Solo developers / indie hackers** who need multi-provider AI workflows without learning Python frameworks
3. **AI engineers** frustrated with Python dependency hell who want reproducible, single-binary execution
4. **Content creators / media teams** who need AI + media processing pipelines (the CAS + media pipeline is unique)
5. **Open source advocates** who want AGPL-licensed tools without vendor lock-in

**Who would NOT use Nika:**
- Data scientists (prefer Python notebooks)
- Non-technical business users (prefer GUI builders like Dify/n8n)
- Enterprise teams requiring SOC2/vendor support (no enterprise offering)
- Teams already deep in LangChain ecosystem (switching cost too high)

### Market Size Context

The AI developer tools market is estimated at $10B+ by 2026 (Gartner), with "agentic AI workflows" as the fastest-growing segment. However, the CLI-native declarative subset is tiny -- perhaps <1% of the market. Nika needs to either:
1. **Own the niche** (CLI-native AI workflows) and grow it
2. **Expand the niche** by making YAML AI workflows accessible enough to attract GUI-first users

---

## 6. Funding Gap Reality Check

| Tool | Funding | Stars | Nika Multiple |
|------|---------|-------|---------------|
| LangChain | $260M | 131k | 32,900x stars |
| Temporal | $103M | 19k | 4,800x stars |
| n8n | $68M+ | 182k | 45,400x stars |
| Dify | $30M+ | 135k | 33,750x stars |
| deepset | ~$30M | 25k | 6,164x stars |
| CrewAI | $18M | 48k | 11,895x stars |
| **Nika** | **$0** | **4** | **1x** |

Total competitor funding in this space: **>$509M**.

This is not necessarily a disadvantage for positioning -- it means the market is validated and well-funded. But it means Nika must compete on differentiation (which it has) and developer experience, not on marketing budget or enterprise sales.

---

## 7. Strategic Recommendations

### Strengths to Emphasize
1. **Zero-dependency single binary** -- no Python, no Node, no Docker. `curl | sh` and run.
2. **YAML-in-git** -- diffable, reviewable, composable workflows. CI/CD native.
3. **8 providers + GGUF in one binary** -- no pip install per provider.
4. **MCP client** -- future-proof integration protocol.
5. **Media pipeline + CAS** -- unique feature no competitor has.

### Weaknesses to Address
1. **4 stars** -- critical distribution problem. Need launch strategy.
2. **No GUI** -- limits audience to CLI-native developers.
3. **No enterprise offering** -- cannot compete for enterprise budgets.
4. **AGPL license** -- some companies avoid AGPL. (But this is a deliberate philosophical choice.)
5. **Solo maintainer risk** -- competitors have 10-300+ person teams.

### Positioning Options (ranked by clarity)

1. **"GitHub Actions for AI"** -- declarative YAML, DAG execution, provider-agnostic. Clearest analogy.
2. **"Docker Compose for AI workflows"** -- multi-service orchestration via YAML. Strong for DevOps audience.
3. **"Terraform for AI"** -- valid but risks confusion with actual Terraform for AI infra.
4. **"The Rust-native AI workflow engine"** -- appeals to Rust community specifically.
5. **"AI without Python"** -- provocative, clear differentiation, but potentially alienating.

---

## Sources

1. GitHub API (live queries, 2026-03-30) -- star counts, push dates, release versions
2. Perplexity sonar-pro (12 queries) -- funding, market analysis, ecosystem research
3. LangChain blog / Crunchbase -- funding rounds confirmed
4. Dify.ai announcements -- $30M Pre-A at $180M valuation
5. CrewAI Crunchbase -- $18M total, $12.5M Series A Insight Partners
6. n8n community posts -- star milestones, funding history
7. Haystack official docs -- confirmed Python-first, YAML serialization only in v2.x
8. Ironclad/rivet GitHub -- last core commit Oct 2025 (effectively abandoned)
9. Prior Nika research: /docs/research/2026-03-23-competitive-landscape.md

---

## Methodology

- **GitHub stars:** Live API calls to api.github.com on 2026-03-30 (not cached/estimated)
- **Funding:** Cross-referenced Perplexity results with Crunchbase-sourced data where available
- **Feature comparison:** Based on official documentation and repository inspection
- **Claim verification:** Systematic search for counter-examples across 12 Perplexity queries
- **Limitations:** Download counts (PyPI/npm/crates.io) not systematically collected; some funding figures are approximate
