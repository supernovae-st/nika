# AI Workflow Project Structure Landscape (2025-2026)

> Research date: 2026-04-01
> Purpose: Understand how competing AI/LLM workflow and agent tools structure their projects on disk, to inform Nika's `nika init` and `.nika/` conventions.

## Executive Summary

There is **no emerging standard** for AI workflow project layout in 2025-2026. Each tool invented its own conventions. The landscape splits into three archetypes:

1. **Config-at-root** (LangGraph, CrewAI, Prompt Flow) -- a single config file signals the project root
2. **Database-backed** (Flowise, n8n, Dify) -- workflows live in SQLite/Postgres, not files
3. **Code-first** (DSPy, AutoGen, Semantic Kernel) -- no config file, Python/C# code IS the project

Nika's YAML-per-workflow model with a `.nika/` runtime directory is closest to Prompt Flow's `flow.dag.yaml` + `.promptflow/` pattern, but with significant differentiation (multi-workflow, encrypted vault, DAG-native).

---

## Tool-by-Tool Analysis

### 1. LangGraph (LangChain)

| Aspect | Detail |
|--------|--------|
| **Config file** | `langgraph.json` (JSON) at project root |
| **Schema** | `"$schema": "https://langgra.ph/schema.json"` |
| **Root detection** | Presence of `langgraph.json` |
| **Scaffold command** | `langgraph new <name>` |
| **Other CLI** | `langgraph build`, `langgraph up`, `langgraph dev`, `langgraph test` |
| **Runtime/cache dir** | No documented `.langgraph/` directory |
| **Secrets** | `.env` file referenced via `"env": ".env"` in config |
| **Workflow organization** | Python modules referenced by import path in `graphs` field |

**langgraph.json fields:**
```json
{
  "$schema": "https://langgra.ph/schema.json",
  "python_version": "3.11",
  "dependencies": ["langchain-openai", "."],
  "graphs": {
    "agent": "./src/agent.py:graph"
  },
  "env": ".env",
  "store": {
    "index": {
      "embed": "openai:text-embedding-3-large",
      "dims": 3072,
      "fields": ["title", "content"]
    }
  },
  "dockerfile_lines": ["RUN apt-get install -y libmagic-dev"]
}
```

**Typical layout:**
```
my-app/
  langgraph.json          # Root marker + config
  .env                    # Secrets
  requirements.txt
  my_agent/
    __init__.py
    agent.py              # Graph construction
    utils/
      tools.py
      nodes.py
      state.py
```

**Notes:** LangGraph Studio (desktop app) detects projects by scanning for `langgraph.json`. Graphs are Python objects, not YAML. The config is deployment-oriented (Docker, LangSmith).

---

### 2. CrewAI

| Aspect | Detail |
|--------|--------|
| **Config file** | `crew.yaml` (YAML) at project root, plus `agents.yaml`, `tasks.yaml` |
| **Root detection** | Presence of `crew.yaml` or `pyproject.toml` with crewai deps |
| **Scaffold command** | `crewai create crew <name>` |
| **Other CLI** | `crewai run`, `crewai test`, `crewai train` |
| **Runtime/cache dir** | `.crewai/` (cache/, logs/) |
| **Secrets** | `.env` file, env vars |
| **Workflow organization** | YAML files per agent/task, Python for custom tools |

**Typical layout:**
```
marketing-research/
  crew.yaml               # Main orchestration config
  agents/
    researcher.yaml        # role, goal, backstory, tools
    writer.yaml
  tasks/
    research.yaml          # description, agent, expected_output
    write.yaml
  tools/
    web_search.py
  .crewai/
    cache/                 # LLM response cache
    logs/                  # Execution traces
  .env
  requirements.txt
  main.py                  # Programmatic kickoff
```

**Notes:** CrewAI is the closest competitor to Nika in terms of YAML-first declarative approach. Separates agents and tasks into their own YAML files (vs. Nika's single-file-per-workflow). `.crewai/` is the only documented hidden runtime directory among code-first tools.

---

### 3. AutoGen (Microsoft)

| Aspect | Detail |
|--------|--------|
| **Config file** | `OAI_CONFIG_LIST` (JSON list of LLM configs) |
| **Root detection** | None -- pure Python, no marker file |
| **Scaffold command** | None (`pip install pyautogen`, write Python) |
| **Runtime/cache dir** | None documented (logs/traces ad-hoc) |
| **Secrets** | Env vars or inline in `OAI_CONFIG_LIST` |
| **Workflow organization** | Python scripts defining agents + GroupChat |

**OAI_CONFIG_LIST format:**
```json
[
  {"model": "gpt-4o", "api_key": "sk-...", "temperature": 0.7},
  {"model": "claude-sonnet-4-20250514", "api_key": "sk-ant-..."}
]
```

**Typical layout:**
```
my-autogen-project/
  OAI_CONFIG_LIST         # LLM configs (gitignored)
  main.py                 # Agent definitions + orchestration
  tools.py                # Custom functions
  requirements.txt
```

**Notes:** AutoGen v0.4+ (2025) is fully code-first with no YAML layer. The AG2 fork adds some structure but remains Python-centric. No project scaffolding. Least opinionated about filesystem layout.

---

### 4. Haystack (deepset)

| Aspect | Detail |
|--------|--------|
| **Config file** | No project-level config; pipelines serialized as `pipeline.yml` |
| **Root detection** | None -- library, not a framework |
| **Scaffold command** | None (`pip install haystack-ai`) |
| **Runtime/cache dir** | None documented |
| **Secrets** | Env vars with `COMPONENTNAME_PARAMNAME` convention, `.env` |
| **Workflow organization** | YAML pipeline files in `pipelines/` directory (convention) |

**Pipeline YAML format:**
```yaml
components:
  cleaner:
    init_parameters:
      remove_empty_lines: true
      remove_extra_whitespaces: true
connections: []
max_runs_per_component: 100
metadata: {}
```

**Typical layout:**
```
haystack-project/
  pipelines/
    qa_pipeline.yml
    rag_pipeline.yml
  components/                # Custom Python components
  config.yaml                # App-level config
  .env
  requirements.txt
```

**Notes:** Haystack 2.x is a library, not an opinionated framework. Pipeline YAML is serialization output (from `Pipeline.dump()`), not a hand-authored format. Env var override pattern: `Pipeline.load(file, overwrite_with_env_variables=True)`.

---

### 5. DSPy (Stanford)

| Aspect | Detail |
|--------|--------|
| **Config file** | None -- `dspy.settings.configure()` in Python |
| **Root detection** | None |
| **Scaffold command** | None (proposed `dspy-wizard` never shipped) |
| **Runtime/cache dir** | `~/.dspy/` (global cache for prompts, metrics, optimizer artifacts) |
| **Secrets** | Env vars or `dspy.settings.configure(api_key=...)` |
| **Workflow organization** | Python modules: programs, signatures, modules |

**Typical layout:**
```
my-dspy-project/
  program.py               # Main dspy.Module subclass
  signatures.py             # Reusable Signature definitions
  modules.py                # Custom modules
  run.py                    # Execution script
  config.py                 # dspy.settings.configure()
  data/                     # Training data for optimization
  .env
  requirements.txt
```

**Notes:** DSPy is the most academic/research-oriented tool. Cache is global (`~/.dspy/`) not per-project. No YAML layer at all. Programs are optimizable Python objects.

---

### 6. Rivet (Ironclad)

| Aspect | Detail |
|--------|--------|
| **Config file** | `.rivet-project` (single file containing all graphs) |
| **Root detection** | Presence of `.rivet-project` file |
| **Scaffold command** | None -- GUI-only (File > New Project) |
| **Runtime/cache dir** | None documented |
| **Secrets** | Environment variables via Rivet Core integration |
| **Workflow organization** | Graphs stored inside single `.rivet-project` file (YAML) |

**Notes:** Rivet is GUI-first (visual graph editor). The `.rivet-project` file is a monolithic YAML containing all graphs, nodes, and connections. Subgraph nodes allow graph-to-graph references. No CLI. Rivet Core (TypeScript library) executes projects programmatically.

---

### 7. Flowise

| Aspect | Detail |
|--------|--------|
| **Config file** | None on disk -- database-backed |
| **Root detection** | N/A (server-based) |
| **Scaffold command** | `npx flowise start` |
| **Runtime/cache dir** | Database (SQLite default, MySQL, PostgreSQL) |
| **Secrets** | Per-chatflow API keys, `.env` for server config |
| **Workflow organization** | Stored in database, exportable as JSON |

**Notes:** Flowise is a no-code drag-and-drop builder. Workflows never live on disk as files (they're in the DB). Export produces JSON snapshots. This is the opposite end of the spectrum from Nika's file-first approach.

---

### 8. ComfyUI

| Aspect | Detail |
|--------|--------|
| **Config file** | `extra_model_paths.yaml` (model path mapping) |
| **Root detection** | Installation directory with `main.py` |
| **Scaffold command** | None -- git clone + run |
| **Runtime/cache dir** | `output/` (generated files), `custom_nodes/` (extensions) |
| **Secrets** | None -- local-only, no API keys by default |
| **Workflow organization** | JSON files in `ComfyUI/user/default/workflows/` |

**Typical layout:**
```
ComfyUI/
  main.py
  extra_model_paths.yaml
  models/
    checkpoints/
    loras/
    vae/
    controlnet/
  custom_nodes/              # Third-party extensions
  output/                    # Generated images
  user/
    default/
      workflows/             # Saved workflow JSON files
        text-to-image/
        inpainting/
```

**Notes:** ComfyUI is image-generation focused. Workflows are visual node graphs saved as JSON. No concept of "projects" -- it's an application, not a framework. Subgraphs provide modularity since 2025.

---

### 9. Prompt Flow (Microsoft)

| Aspect | Detail |
|--------|--------|
| **Config file** | `flow.dag.yaml` (YAML DAG definition) |
| **Root detection** | Presence of `flow.dag.yaml` (CLI/VS Code scan upward) |
| **Scaffold command** | `pf init` |
| **Runtime/cache dir** | `.promptflow/` (connections, runs, metadata) |
| **Secrets** | `.promptflow/connections.json`, `pf connection create` CLI |
| **Workflow organization** | One `flow.dag.yaml` per flow directory |

**flow.dag.yaml format:**
```yaml
inputs:
  user_query:
    type: string
    default: "What is AI?"
outputs:
  answer:
    type: string
    reference: ${summarize.output}
nodes:
  - name: search
    type: python
    source: search.py
    inputs:
      query: ${inputs.user_query}
  - name: summarize
    type: llm
    source: summarize.jinja2
    inputs:
      context: ${search.output}
```

**Typical layout:**
```
my-flow/
  flow.dag.yaml             # Root marker + DAG definition
  flow.tools.json            # Custom tool registry
  .promptflow/
    connections.json         # Encrypted API keys/endpoints
    runs/
      <run-id>/              # Execution logs, metrics, JSONL
  search.py                  # Python tool source
  summarize.jinja2           # Prompt template
  data/
    sample.json              # Test inputs
```

**Notes:** Prompt Flow is the **closest architectural analog to Nika**. Both use YAML DAG files as the root marker, both have a hidden runtime directory (`.promptflow/` vs `.nika/`), both store connections/secrets separately. Key differences: Prompt Flow is one-flow-per-directory, Nika is many-workflows-per-project. Prompt Flow uses Jinja2 templates, Nika uses `{{with.}}` bindings.

---

### 10. Semantic Kernel (Microsoft)

| Aspect | Detail |
|--------|--------|
| **Config file** | `appsettings.json` (C#) or none (Python) |
| **Root detection** | Standard .NET/Python project detection |
| **Scaffold command** | VS Code extension "Semantic Kernel Tools" (right-click > add plugin) |
| **Runtime/cache dir** | None documented |
| **Secrets** | .NET Secret Manager, Azure Key Vault, env vars |
| **Workflow organization** | `plugins/` directory with `skprompt.txt` + `config.json` per function |

**Plugin directory structure:**
```
plugins/
  OfficePlugin/
    ScheduleMeeting/
      skprompt.txt           # Prompt template
      config.json            # Function metadata
    SummarizeEmailThread/
      skprompt.txt
      config.json
```

**Notes:** Semantic Kernel is a library/SDK, not a workflow engine. Plugins are loaded via `kernel.ImportPluginFromPromptDirectory()`. The `skprompt.txt` + `config.json` per-function pattern is unique. Python version also supports `.yaml` function definitions.

---

## Bonus: Other Notable Tools

### n8n (Self-hosted)

| Aspect | Detail |
|--------|--------|
| **Config file** | None on disk -- database-backed |
| **Runtime dir** | `~/.n8n/` (contains `database.sqlite`, config) |
| **Secrets** | `N8N_ENCRYPTION_KEY` in `.env`, credentials encrypted in DB |
| **Export** | `n8n export:workflow --all --output=workflows.json` |

### Windmill.dev

| Aspect | Detail |
|--------|--------|
| **Config file** | `windmill.yaml` (workspace config) |
| **Scaffold command** | `wmill init` |
| **Organization** | `f/` (flows), `u/` (user scripts), Git-synced |
| **Secrets** | Server-side resource management |

### Dify.ai

| Aspect | Detail |
|--------|--------|
| **Storage** | PostgreSQL (via docker-compose) |
| **Organization** | Database-backed, no file-based workflows |

---

## Cross-Cutting Analysis

### Project Root Detection Mechanisms

| Mechanism | Tools |
|-----------|-------|
| Named config file at root | LangGraph (`langgraph.json`), Prompt Flow (`flow.dag.yaml`), Windmill (`windmill.yaml`) |
| Project file | Rivet (`.rivet-project`), CrewAI (`crew.yaml`) |
| No detection (code-first) | AutoGen, DSPy, Haystack, Semantic Kernel |
| Server-based (N/A) | Flowise, n8n, Dify |

### Config File Format Preferences

| Format | Tools |
|--------|-------|
| **YAML** | CrewAI, Haystack, Prompt Flow, ComfyUI (model paths), Windmill, Semantic Kernel (.py variant) |
| **JSON** | LangGraph, AutoGen (`OAI_CONFIG_LIST`), Flowise (export), ComfyUI (workflows), Rivet (internal YAML but often treated as JSON) |
| **None (code-only)** | DSPy, AutoGen v0.4+ |

### Hidden Runtime Directories

| Directory | Tool | Contents |
|-----------|------|----------|
| `.promptflow/` | Prompt Flow | connections.json, runs/<id>/, metadata |
| `.crewai/` | CrewAI | cache/, logs/ |
| `~/.n8n/` | n8n | database.sqlite, config (global, not per-project) |
| `~/.dspy/` | DSPy | Prompt cache, optimizer artifacts (global) |
| `.nika/` | **Nika** | vault.enc, traces/, cache/ (per-project) |

### Secrets Architecture Comparison

| Approach | Tools | Security Level |
|----------|-------|----------------|
| `.env` file (plaintext) | LangGraph, CrewAI, AutoGen, Haystack, ComfyUI | Low |
| Encrypted in DB | n8n, Flowise, Dify | Medium |
| Encrypted vault file | **Nika** (XChaCha20Poly1305 + Argon2i) | High |
| Connection objects | Prompt Flow (`.promptflow/connections.json`) | Medium |
| Platform vault | Semantic Kernel (Azure Key Vault) | High |

### Scaffold / Init Commands

| Command | Tool |
|---------|------|
| `langgraph new <name>` | LangGraph |
| `crewai create crew <name>` | CrewAI |
| `pf init` | Prompt Flow |
| `wmill init` | Windmill |
| `npx flowise start` | Flowise (server, not project) |
| `nika init` | **Nika** |
| None | AutoGen, DSPy, Haystack, Semantic Kernel, ComfyUI, Rivet |

### Workflow Granularity

| Pattern | Tools | Description |
|---------|-------|-------------|
| One workflow = one file | **Nika** (`.nika.yaml`), Prompt Flow (`flow.dag.yaml` per dir), ComfyUI (JSON per workflow) | Each workflow is self-contained |
| One project = one config + many code files | LangGraph, CrewAI | Config references code modules |
| One file = entire project | Rivet (`.rivet-project`) | Monolithic |
| No files (database) | Flowise, n8n, Dify | Server stores everything |

---

## Key Insights for Nika

### What Nika Already Does Better

1. **Encrypted secrets vault** -- Only Nika and Semantic Kernel (via Azure) have real encryption. Everyone else uses plaintext `.env`.
2. **Per-workflow files** -- `.nika.yaml` is self-contained and portable. CrewAI fragments across `agents.yaml`, `tasks.yaml`, `crew.yaml`. LangGraph requires Python modules.
3. **Multi-workflow projects** -- Prompt Flow is one-flow-per-directory. Nika supports many `.nika.yaml` files in one project.
4. **YAML-native** -- Unlike LangGraph (JSON config + Python code) or DSPy (pure Python), Nika workflows are readable YAML.

### What Nika Could Learn

1. **`langgraph.json` schema URL** -- LangGraph includes `"$schema": "https://langgra.ph/schema.json"` for editor autocomplete. Nika could publish a JSON Schema for `.nika.yaml` and reference it.
2. **Prompt Flow's `pf connection create`** -- Dedicated CLI for managing connections/secrets is cleaner than `nika keys set` (which conflates provider selection with secret storage).
3. **CrewAI's separation of concerns** -- Agents and tasks in separate YAML files allows reuse across crews. Nika's `include:` with `prefix:` achieves similar but is less intuitive.
4. **LangGraph Studio** -- Desktop app that auto-detects `langgraph.json` and visualizes graphs. Nika's TUI serves a similar role but could be more discoverable.
5. **Windmill's Git sync** -- `wmill deploy` pushes to Git automatically. Nika workflows are already Git-friendly but lack built-in sync tooling.

### Competitive Positioning Table

| Feature | Nika | LangGraph | CrewAI | Prompt Flow |
|---------|------|-----------|--------|-------------|
| Config format | YAML | JSON | YAML | YAML |
| Config file | `*.nika.yaml` | `langgraph.json` | `crew.yaml` | `flow.dag.yaml` |
| Root marker | `.nika.yaml` files or `nika.yaml` | `langgraph.json` | `crew.yaml` | `flow.dag.yaml` |
| Runtime dir | `.nika/` | None | `.crewai/` | `.promptflow/` |
| Secrets | Encrypted vault | `.env` plaintext | `.env` plaintext | connections.json |
| Init command | `nika init` | `langgraph new` | `crewai create crew` | `pf init` |
| Language | Rust | Python | Python | Python |
| Multi-workflow | Yes | No (one project = one app) | No (one crew) | No (one flow per dir) |
| Schema validation | `nika check` | None | None | VS Code extension |
| DAG visualization | `nika workflow graph` + TUI | LangGraph Studio | None | VS Code extension |
| Course/learning | `nika init --course` (12 levels) | None | None | None |

---

## Emerging Patterns (2025-2026)

1. **AGENTS.md as convention** -- Multiple tools now expect an `AGENTS.md` file at repo root for AI agent instructions (similar to `CLAUDE.md`, `CURSOR.md`). This is the closest thing to an "emerging standard."

2. **No RFC or standard body** -- Unlike CI/CD (where GitHub Actions YAML, Tekton CRDs, Argo CRDs have some convergence), AI workflow formats remain entirely proprietary. No CNCF, OASIS, or W3C effort exists.

3. **Config-at-root wins** -- Tools with explicit root markers (`langgraph.json`, `flow.dag.yaml`, `crew.yaml`) provide better DX than code-first tools (AutoGen, DSPy) that have no project boundary detection.

4. **Hidden dot-directory is standard** -- `.promptflow/`, `.crewai/`, `~/.n8n/`, `~/.dspy/` -- every tool that needs runtime state uses a dot-directory. Per-project (`.promptflow/`) is better than global (`~/.dspy/`).

5. **YAML > JSON for workflow definitions** -- Most tools that let users author workflows use YAML (CrewAI, Prompt Flow, Haystack, Nika). JSON is used for machine-generated configs (LangGraph, ComfyUI, Flowise exports).

6. **Monorepo awareness is rare** -- Only Windmill and LangGraph have any concept of multi-project organization. Most tools assume one project = one directory. Nika's `include:` with partial workflows is unique.

---

## Sources

1. LangGraph documentation and CLI reference -- langgraph.json schema, CLI commands
2. CrewAI documentation -- crewai create scaffold, YAML config
3. Microsoft AutoGen v0.4 release -- agent architecture, OAI_CONFIG_LIST
4. Deepset Haystack 2.x docs -- pipeline serialization, YAML format
5. Stanford DSPy documentation -- settings, cache, module patterns
6. Rivet (Ironclad) documentation -- .rivet-project format
7. Flowise documentation -- chatflow storage, database architecture
8. ComfyUI documentation -- workflow JSON, directory structure
9. Microsoft Prompt Flow docs -- flow.dag.yaml, .promptflow/, connections
10. Semantic Kernel docs -- plugin directory, skprompt.txt, kernel builder
11. n8n documentation -- ~/.n8n/, database.sqlite, encryption
12. Windmill.dev documentation -- wmill CLI, Git sync

## Methodology

- Tools used: Perplexity AI (sonar-pro) for web search across all 10+ tools
- Pages analyzed: ~40 documentation pages and GitHub repos
- Time period: 2025-01 through 2026-04
- Confidence: **High** for LangGraph, Prompt Flow, CrewAI, ComfyUI (well-documented). **Medium** for DSPy, Semantic Kernel, Haystack (conventions inferred from examples). **Low** for Rivet, Flowise, Dify (limited public documentation on project structure).
