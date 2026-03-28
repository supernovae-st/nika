# Research Report: Hermes Agent by Nous Research -- Deep Dive

**Date**: 2026-03-27
**Author**: Thibaut + Claude
**Sources**: GitHub repo (README, AGENTS.md, RELEASE_v0.4.0.md, source code), agentskills.io

---

## Executive Summary

Hermes Agent is the self-improving AI agent by Nous Research. Its differentiating thesis: **the agent should get better every time you use it**, through a closed loop of skill creation, memory curation, and (for Nous) RL training data generation. It is an open-source Python agent (MIT license) that runs on any LLM via OpenAI-compatible APIs, lives permanently on a VPS/cloud, talks to users over Telegram/Discord/Slack/WhatsApp/Signal/SMS/Matrix, and generates reinforcement learning training trajectories for Nous's next-generation models.

v0.4.0 (released 2026-03-23) is the "platform expansion" release: OpenAI-compatible API server, 6 new messaging adapters, MCP server management with OAuth 2.1, gateway prompt caching, and 200+ bug fixes.

---

## 1. Architecture of the Self-Improvement Loop

The learning loop has **4 layers**, from fast (runtime) to slow (model training):

### Layer 1: Memory (Declarative Knowledge, ~seconds)

Two file-backed stores in `~/.hermes/memories/`:

| Store | Purpose | Char Limit |
|-------|---------|------------|
| `MEMORY.md` | Agent's observations: env facts, project conventions, tool quirks | 2,200 chars |
| `USER.md` | User profile: preferences, communication style, workflow habits | 1,375 chars |

**Frozen snapshot pattern**: Both files are loaded into the system prompt at session start as a frozen snapshot. Mid-session writes update the files on disk immediately (durable) but do NOT change the system prompt -- this preserves the Anthropic prefix cache for the entire session. The snapshot refreshes on the next session start.

**Entry format**: `\n$\n` (section sign) delimiter. Entries can be multiline. No IDs -- uses substring matching for replace/remove operations.

**Security**: Memory content is scanned for prompt injection (invisible unicode, exfiltration patterns like `curl ... $API_KEY`, role hijack patterns) before being accepted. Blocked entries get a clear error.

**File locking**: Uses `fcntl.LOCK_EX` on a separate `.lock` file to prevent concurrent memory writes from dropping entries (gateway mode creates one AIAgent per session).

### Layer 2: Skills (Procedural Knowledge, ~minutes)

Skills are directories in `~/.hermes/skills/` following the **agentskills.io** open standard:

```
skills/
  my-skill/
    SKILL.md           # YAML frontmatter + markdown instructions
    references/        # Supporting documentation
    templates/         # Output templates
    scripts/           # Executable code
    assets/            # Resources
```

**SKILL.md format** (agentskills.io spec):
```yaml
---
name: skill-name              # Required, max 64 chars, lowercase+hyphens
description: Brief description # Required, max 1024 chars
version: 1.0.0                # Optional
license: MIT                  # Optional
platforms: [macos]            # Optional OS filter
metadata:                     # Arbitrary key-value
  hermes:
    tags: [fine-tuning, llm]
---

# Instructions, procedures, examples...
```

**Progressive disclosure**: Skill metadata is shown in `skills_list` (Tier 1). Full content loaded via `skill_view` on demand (Tier 2-3). This keeps the token cost low until a skill is actually needed.

**Agent-managed CRUD**: The `skill_manage` tool provides 6 actions:
- `create` -- New skill with SKILL.md + directory structure
- `edit` -- Full rewrite of SKILL.md
- `patch` -- Targeted find-and-replace within any skill file
- `delete` -- Remove a skill entirely
- `write_file` -- Add supporting files (references, templates, scripts, assets)
- `remove_file` -- Remove a supporting file

**Security scanning**: Every write (create, edit, patch, write_file) is followed by a security scan (`skills_guard.py`). Blocked content triggers a rollback to the previous state. "Caution" findings are allowed but logged; "dangerous" findings prompt the user for confirmation.

**Skill slash commands**: `agent/skill_commands.py` scans `~/.hermes/skills/` and registers each skill as a slash command. When invoked, the skill content is injected as a **user message** (not system prompt) to preserve prompt caching.

### Layer 3: Background Review (The Nudge System)

This is the heart of the self-improvement loop. **After** the main conversation turn completes (AFTER the user sees the response), Hermes spawns a **background thread** with a review agent:

**Trigger conditions**:
- **Memory review**: Every `memory_nudge_interval` turns (default: 10), if the memory tool is available and a memory store is loaded.
- **Skill review**: Every `skill_nudge_interval` iterations (default: 10), if the `skill_manage` tool is available.
- Both can trigger simultaneously (combined review prompt).

**How it works**:
1. After `run_conversation()` completes, the trigger counters are checked.
2. If either threshold is met, `_spawn_background_review()` creates a snapshot of the full message history.
3. A new `AIAgent` is forked in a background thread with:
   - Same model, provider, platform
   - `max_iterations=8` (lightweight review)
   - `quiet_mode=True` (no terminal output)
   - Shared `_memory_store` instance (writes to the same files)
   - `stdout`/`stderr` redirected to `/dev/null`
4. The review agent receives one of three prompts:
   - **Memory review**: "Has the user revealed preferences, persona, expectations? Save using memory tool."
   - **Skill review**: "Was a non-trivial approach used? Trial and error? User corrections? Save/update a skill."
   - **Combined review**: Both of the above.
5. The review agent runs its own tool-calling loop (up to 8 iterations), can call `memory(action='add')` or `skill_manage(action='create'/'patch')`.
6. After completion, successful tool actions are surfaced as a compact summary to the user's display.

**Key design decisions**:
- Runs AFTER the response is delivered (never competes with the user's task)
- Never modifies the main conversation history
- Best-effort (wrapped in try/except, failures are silent)
- Counters reset when the relevant tool is explicitly used by the main agent (prevents redundant reviews)

### Layer 4: RL Training Pipeline (Atropos)

Hermes Agent integrates with Nous Research's **Atropos** RL training framework to generate training data and run reinforcement learning:

**Two-phase operation**:
- **Phase 1 (OpenAI Server)**: Uses `/v1/chat/completions` with structured `tools=` parameter. Server handles tool call parsing. Good for evaluation, SFT data generation, testing. Creates placeholder tokens.
- **Phase 2 (VLLM ManagedServer)**: Uses `/generate` endpoint for exact token IDs + logprobs. Client-side tool call parsers reconstruct structured tool_calls from raw output. Required for full RL training with GRPO/PPO.

**Environment hierarchy**:
```
BaseEnv (atroposlib)
  -> HermesAgentBaseEnv (hermes_base_env.py)
       -> TerminalTestEnv (stack validation)
       -> HermesSweEnv (SWE-bench training)
       -> TerminalBench2EvalEnv (89-task benchmark)
       -> TBLiteEnv (100 calibrated fast tasks)
       -> YCBenchEnv (long-horizon strategic)
```

**Training loop**:
1. Environment provides a task (dataset item)
2. Agent runs multi-turn tool-calling loop against VLLM/SGLang server
3. `ToolContext` gives reward functions full access to the agent's sandbox (terminal, files, browser)
4. Reward function scores the rollout (e.g., `pytest -v` exit code)
5. Scores + token logprobs flow into Atropos for GRPO/PPO training

**Tool call parsers**: 11 client-side parsers for Phase 2: Hermes/ChatML XML, Mistral, Llama 3 JSON, Qwen, Qwen3 Coder, DeepSeek V3, DeepSeek V3.1, Kimi K2, Longcat, GLM 4.5/4.7.

**Terminal backends**: 6 backends for sandboxed execution:
- Local, Docker, SSH, Daytona (serverless persistence), Singularity, Modal (serverless GPU)

---

## 2. Skills: Creation, Storage, and Sharing

### Creation Flow

1. **Autonomous**: After complex tasks (5+ tool calls), the background review agent evaluates whether the approach should be saved as a skill.
2. **Explicit**: User asks the agent to create a skill, or manually creates one.
3. **Hub install**: `hermes skills install <identifier>` from the Skills Hub.

### Storage

All skills live in `~/.hermes/skills/` (single source of truth). This directory is seeded from bundled skills on install. Agent edits, hub installs, and bundled skills all coexist here.

The bundled skills in the repo are organized by category:
- `skills/creative/` -- excalidraw
- `skills/leisure/` -- find-nearby
- `skills/media/` -- youtube-content
- `skills/mlops/training/` -- GRPO RL training
- `skills/productivity/` -- google-workspace, OCR, powerpoint
- `skills/red-teaming/` -- godmode (jailbreak research)
- `skills/research/` -- arxiv, domain-intel, polymarket
- `optional-skills/` -- blockchain, mcp, migration, security, telephony

### Sharing: The AgentSkills.io Open Standard

[agentskills.io](https://agentskills.io) is an open standard originally developed by Anthropic and adopted by the ecosystem. Hermes Agent is a first-class implementor.

**Key specification points**:
- A skill is a directory containing at minimum a `SKILL.md` file
- YAML frontmatter with `name` (required, max 64 chars, lowercase+hyphens) and `description` (required, max 1024 chars)
- Optional fields: `license`, `compatibility`, `metadata`, `allowed-tools`
- Optional directories: `scripts/`, `references/`, `assets/`
- Progressive disclosure architecture: metadata first, full content on demand

**Skills Hub** (`hermes skills` / `/skills` slash command):
- Search, browse, inspect, install skills from multiple sources
- Trust levels: `builtin`, `trusted`, `community`
- Security scanning on install (same guard as agent-created skills)
- Unified search across official, GitHub, and community sources

### Skills Guidance in System Prompt

From `prompt_builder.py`:
```
After completing a complex task (5+ tool calls), fixing a tricky error,
or discovering a non-trivial workflow, save the approach as a
skill with skill_manage so you can reuse it next time.

When using a skill and finding it outdated, incomplete, or wrong,
patch it immediately with skill_manage(action='patch') -- don't wait to be asked.
Skills that aren't maintained become liabilities.
```

This is critical: skills are not just created, they are **self-healing**. The agent is instructed to patch outdated skills during use.

---

## 3. The Memory System

### Architecture

| Component | Type | Persistence | Injection |
|-----------|------|------------|-----------|
| MEMORY.md | Declarative | File-backed, bounded (2200 chars) | Frozen in system prompt at session start |
| USER.md | User model | File-backed, bounded (1375 chars) | Frozen in system prompt at session start |
| Honcho | Dialectic | External service (Plastic Labs) | Prefetched per-turn context |
| Session DB | Episodic | SQLite with FTS5 | On-demand via `session_search` tool |

### Honcho Integration (Plastic Labs)

Honcho provides dialectic user modeling -- it builds a deepening model of who the user is across sessions. Three recall modes:
- `hybrid` (default): Prefetch context baked into system prompt (turn 1) or attached to user message (later turns)
- `tools`: Agent calls Honcho tools explicitly
- Configurable via `~/.hermes/honcho.json` or `~/.honcho/config.json`

When Honcho is active with `memory_mode=honcho`, local USER.md writes are disabled to avoid conflicts.

### Session Search (Cross-Session Recall)

`session_search` uses SQLite FTS5 full-text search with LLM summarization. The guidance says: "When the user references something from a past conversation or you suspect relevant cross-session context exists, use session_search to recall it before asking them to repeat themselves."

### Memory Guidance in System Prompt

```
You have persistent memory across sessions. Save durable facts using the memory
tool: user preferences, environment details, tool quirks, and stable conventions.
Memory is injected into every turn, so keep it compact and focused on facts that
will still matter later.

Prioritize what reduces future user steering -- the most valuable memory is one
that prevents the user from having to correct or remind you again. User preferences
and recurring corrections matter more than procedural task details.

Do NOT save task progress, session outcomes, completed-work logs, or temporary TODO
state to memory; use session_search to recall those from past transcripts.
If you've discovered a new way to do something, solved a problem that could be
necessary later, save it as a skill with the skill tool.
```

The separation is intentional: memory = facts, skills = procedures, sessions = episodes.

---

## 4. Gateway Architecture (Telegram/Discord/WhatsApp/...)

### Overview

The gateway (`gateway/run.py`) is a single persistent process that bridges multiple messaging platforms to the same Hermes agent:

```
hermes gateway start
  -> Telegram adapter (polling/webhook)
  -> Discord adapter (WebSocket)
  -> Slack adapter
  -> WhatsApp adapter (bridge)
  -> Signal adapter
  -> DingTalk adapter
  -> SMS (Twilio) adapter
  -> Mattermost adapter
  -> Matrix adapter
  -> Webhook adapter
  -> OpenAI-compatible API server (/v1/chat/completions)
```

### Key Design Decisions

1. **One AIAgent per session**: Gateway creates a fresh `AIAgent` per incoming message, but caches the instance per session to preserve Anthropic prompt cache across turns.

2. **Session isolation**: Each platform+user combination gets its own session. Sessions are stored in SQLite via `SessionStore`.

3. **Cross-platform conversation continuity**: The same user can start on Telegram and continue on Discord (via session keys).

4. **MEDIA: protocol**: Agent includes `MEDIA:/absolute/path/to/file` in responses to send native attachments (images, audio, video, documents) on each platform.

5. **Auto-reconnect**: Failed platforms reconnect with exponential backoff.

6. **Security**: DM pairing for authorization, command approval for dangerous operations, PII redaction config, container isolation.

7. **Cron scheduler**: Built-in cron with delivery to any platform. `[SILENT]` response suppresses delivery. Jobs stored in `jobs.json` with timezone awareness.

### Platform-Specific Adaptations

- **Telegram**: MarkdownV2 rendering, topic/thread-based sessions, auto-reconnect polling, media-group aggregation
- **Discord**: Persistent typing indicator, document caching, voice channel TTS, thread participation persistence
- **WhatsApp**: Bridge subprocess, LID format self-chat, outbound message routing
- **Signal**: Attachment handling, group message filtering, Note to Self echo-back protection
- **API Server**: `/v1/chat/completions` endpoint, `/api/jobs` REST API for cron management, SQLite-backed response persistence, CORS protection

---

## 5. What We Can Learn for Nika

### Direct Inspirations

| Hermes Feature | Nika Equivalent / Opportunity |
|----------------|-------------------------------|
| Background review agent | **Post-workflow analysis task**: After a workflow completes, optionally spawn an `infer:` task that reviews the execution trace and suggests improvements to the workflow YAML itself |
| Skill creation/patching | **Workflow templates with self-improvement**: Workflows that generate improved versions of themselves based on execution results |
| MEMORY.md / USER.md | **Workflow-level context files**: Already have `context:` in header, but could add persistent user preferences |
| Session search (FTS5) | **Trace search**: `nika trace search` across past execution traces |
| agentskills.io standard | **Workflow packages with SKILL.md**: Each `.nika.yaml` package could include a SKILL.md for agent discovery |
| Nudge interval system | **Adaptive workflows**: After N executions, suggest optimizations |
| Gateway architecture | **Telegram trigger for Nika workflows**: `nika gateway` that receives messages and dispatches workflows |
| Atropos RL environments | **Workflow generation benchmarks**: Environments that score workflow quality |
| Tool call parsers | **Multi-provider tool format handling**: Already handled by rig-core |
| Progressive skill disclosure | **Workflow catalog with lazy loading**: Show metadata, load full YAML on demand |

### Key Architectural Lessons

1. **Frozen snapshot pattern**: Hermes injects memory into the system prompt ONCE at session start, never mutates it mid-session. This preserves prompt caching. Nika should adopt this for `context:` files -- load once at workflow start, never re-read mid-execution.

2. **Background review as a separate agent**: The review agent is a full AIAgent fork, not an inline step. This separation prevents the review from competing with the user's task. For Nika, post-workflow analysis should be a completely separate workflow execution.

3. **Bounded memory with substring matching**: No IDs, no databases -- just a text file with character limits and substring-based replace/remove. Extremely simple, extremely robust. Nika's context system could benefit from this simplicity.

4. **Security scanning at every write boundary**: Both memory and skills scan content for injection on every write. Nika already has SSRF protection and command blocklists, but could add content scanning for `infer:` outputs that get used as inputs to `exec:` tasks.

5. **The learning loop is opt-in and gradual**: Memory nudges every 10 turns, skill reviews every 10 iterations. Not every turn, not never. The system gently improves without being intrusive.

### What Hermes Does NOT Do (Nika's Advantages)

- **No DAG execution**: Hermes is a linear agent loop. No parallel task execution, no dependency graphs, no fan-out/fan-in. Nika's DAG engine is fundamentally more powerful for structured workflows.
- **No declarative workflows**: Everything is imperative Python. No YAML workflow files, no reproducibility guarantees. Nika's `.nika.yaml` format is a major differentiator.
- **No typed data flow**: Hermes passes strings between tools. Nika has `with:` bindings, JSONPath, pipe transforms, structured output with schema validation.
- **No artifact system**: Hermes writes to the filesystem directly. Nika has a proper artifact system with CAS, format validation, and manifest generation.
- **No media pipeline**: Hermes has basic vision and TTS. Nika has 24 builtin media tools with CAS, thumbnails, optimization, provenance.
- **No embedded runtime**: Hermes is a CLI/gateway application. Nika is an embeddable engine (`nika-engine` crate).

---

## 6. v0.4.0 Release Highlights

Released 2026-03-23, this is a massive release with 200+ bug fixes:

- **OpenAI-compatible API server**: Expose Hermes as `/v1/chat/completions` endpoint
- **6 new messaging adapters**: Signal, DingTalk, SMS (Twilio), Mattermost, Matrix, Webhook
- **4 new inference providers**: GitHub Copilot, Alibaba Cloud/DashScope, Kilo Code, OpenCode
- **MCP server management**: `hermes mcp` with full OAuth 2.1 PKCE flow
- **@ context references**: Claude Code-style `@file` and `@url` injection with tab completion
- **Gateway prompt caching**: Cache AIAgent per session for Anthropic cache preservation
- **Context compression overhaul**: Structured summaries, iterative updates, token-budget tail protection
- **Plugin system**: TUI extension hooks, `hermes plugins install/remove/list`, slash command registration
- **Skin engine**: Data-driven CLI theming (banner, spinner, tool prefix, branding)
- **Background memory/skill review**: Replaced inline nudges with background thread reviews

Notable community: 280 PRs by @teknium1 (core), 15+ community contributors.

---

## Sources

1. [GitHub README.md](https://github.com/nousresearch/hermes-agent/blob/main/README.md) -- Project overview, quick start, feature table
2. [GitHub AGENTS.md](https://github.com/nousresearch/hermes-agent/blob/main/AGENTS.md) -- Full developer guide, project structure, patterns
3. [GitHub RELEASE_v0.4.0.md](https://github.com/nousresearch/hermes-agent/blob/main/RELEASE_v0.4.0.md) -- Detailed release notes, 200+ items
4. [Source: run_agent.py](https://github.com/nousresearch/hermes-agent/blob/main/run_agent.py) -- Core agent loop, nudge system, background review
5. [Source: tools/memory_tool.py](https://github.com/nousresearch/hermes-agent/blob/main/tools/memory_tool.py) -- Memory system implementation
6. [Source: tools/skill_manager_tool.py](https://github.com/nousresearch/hermes-agent/blob/main/tools/skill_manager_tool.py) -- Skill CRUD operations
7. [Source: tools/skills_tool.py](https://github.com/nousresearch/hermes-agent/blob/main/tools/skills_tool.py) -- Skill listing and viewing
8. [Source: agent/prompt_builder.py](https://github.com/nousresearch/hermes-agent/blob/main/agent/prompt_builder.py) -- System prompt assembly, context file scanning
9. [Source: environments/README.md](https://github.com/nousresearch/hermes-agent/blob/main/environments/README.md) -- Atropos RL integration documentation
10. [Source: environments/hermes_base_env.py](https://github.com/nousresearch/hermes-agent/blob/main/environments/hermes_base_env.py) -- Base RL environment
11. [agentskills.io](https://agentskills.io) -- Open standard specification
12. [agentskills.io/specification](https://agentskills.io/specification) -- SKILL.md format spec

## Methodology

- Tools used: GitHub raw file fetching, website scraping, source code analysis
- Files analyzed: 12 primary sources, ~15,000 lines of source code
- Repository: ~7,500 lines in run_agent.py, ~3,000 tests, ~40+ tool files

## Confidence Level

**High** -- All findings are based on direct source code analysis from the current `main` branch. The self-improvement loop, memory system, skills architecture, gateway design, and RL pipeline are all confirmed from implementation code, not just documentation claims.

## Further Research Suggestions

- Deep dive into Honcho (Plastic Labs) dialectic user modeling -- how does it differ from MEMORY.md/USER.md?
- Benchmark Hermes's background review agent: how often does it produce useful skills vs. noise?
- Analyze the `toolset_distributions.py` -- how does probabilistic toolset selection affect RL training?
- Compare Hermes's context compression (`agent/context_compressor.py`) with Nika's approach
- Study the `skills_guard.py` security scanner patterns in detail
- Monitor agentskills.io ecosystem adoption (who else implements it?)
