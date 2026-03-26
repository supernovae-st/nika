# Research Report: Vercel `npx skills` CLI (v1.4.5)

**Date:** 2026-03-23
**Source:** github.com/vercel-labs/skills | skills.sh
**Method:** Live CLI testing, source code analysis, API probing

---

## Summary

Vercel's `skills` CLI is an open, agent-agnostic package manager for distributing
AI coding agent instructions. It installs Markdown-based skill files (`SKILL.md`)
into agent-specific directories via symlinks from a canonical `.agents/skills/`
store. It supports 42 agents (41 named + 1 "universal"), has a central registry at
skills.sh, and uses content-hash-based integrity checking. There is no version
pinning -- only content hash comparison against the source repo's HEAD.

---

## 1. All Commands

### `npx skills add <package>`
Installs skills from a GitHub repo, GitLab repo, local path, or well-known URL.

```bash
# GitHub (most common)
npx skills add vercel-labs/agent-skills
npx skills add vercel-labs/agent-skills --all          # All skills, all agents, skip prompts
npx skills add vercel-labs/agent-skills --skill pr-review commit
npx skills add vercel-labs/agent-skills --agent claude-code cursor
npx skills add vercel-labs/agent-skills -g              # Global (user-level)
npx skills add vercel-labs/agent-skills --copy           # Copy files instead of symlink
npx skills add vercel-labs/agent-skills --list           # List available skills without installing
npx skills add vercel-labs/agent-skills --full-depth     # Search all subdirectories

# Other sources
npx skills add https://example.com/my-skill/SKILL.md   # URL (well-known discovery)
npx skills add ./path/to/local/skill                     # Local directory
npx skills add github:owner/repo                         # Explicit GitHub prefix
npx skills add gitlab:owner/repo                         # GitLab support
```

**Output:** Interactive agent selection, security audit (Gen AI, Socket, Snyk), installation summary.

### `npx skills list` / `npx skills ls`
Lists installed skills with their paths and linked agents.

```bash
npx skills list                    # Project skills
npx skills ls -g                   # Global skills
npx skills ls -a claude-code       # Filter by agent
npx skills ls --json               # Machine-readable JSON output
```

**JSON output format:**
```json
[
  {
    "name": "deploy-to-vercel",
    "path": "/project/.agents/skills/deploy-to-vercel",
    "scope": "project",
    "agents": ["Claude Code", "CodeBuddy", "Continue", "Gemini CLI"]
  }
]
```

### `npx skills find [query]`
Searches the skills.sh registry. Interactive when no query provided, non-interactive with a query.

```bash
npx skills find                    # Interactive search (TUI)
npx skills find rust               # Non-interactive search, shows top 6 results
npx skills find workflow           # Search by keyword
```

**Output format:**
```
github/awesome-copilot@rust-mcp-server-generator  7.3K installs
 https://skills.sh/github/awesome-copilot/rust-mcp-server-generator
```

### `npx skills init [name]`
Creates a new skill template with SKILL.md frontmatter.

```bash
npx skills init my-skill           # Creates my-skill/SKILL.md
npx skills init                     # Creates ./SKILL.md in current directory
```

### `npx skills check`
Checks installed skills for updates by comparing content hashes against GitHub HEAD.

```bash
npx skills check
```

Uses `GITHUB_TOKEN` or `GH_TOKEN` for private repos. Compares `skillFolderHash`
(SHA-256 of all files) from lockfile against current GitHub tree hash.

### `npx skills update`
Updates all skills to their latest versions (re-clones from source).

```bash
npx skills update
```

### `npx skills remove [skills]`
Removes installed skills. Interactive when no skill name provided.

```bash
npx skills remove                   # Interactive selection
npx skills remove web-design        # Remove by name
npx skills rm --global frontend     # Remove global skill
npx skills remove --all             # Remove all skills from all agents
```

### `npx skills experimental_install`
Restores skills from `skills-lock.json` (like `npm ci`).

### `npx skills experimental_sync`
Syncs skills from `node_modules/` into agent directories. Discovers `SKILL.md`
files inside npm packages.

```bash
npx skills experimental_sync        # Interactive
npx skills experimental_sync -y     # Skip prompts
```

---

## 2. Directory Structure After Install

### Canonical store: `.agents/skills/`

The **single source of truth** is always `.agents/skills/<skill-name>/`. All
agent-specific directories are **symlinks** pointing back to this canonical location.

```
project/
 .agents/
   skills/
     deploy-to-vercel/           # <-- Real files live here
       SKILL.md
       resources/
         deploy.sh
         deploy-codex.sh
       Archive.zip
     vercel-react-best-practices/
       SKILL.md
       AGENTS.md                  # Optional: expanded instructions
       README.md
       rules/
         _sections.md
         _template.md
         async-api-routes.md
         bundle-barrel-imports.md
         ...40+ rule files
     web-design-guidelines/
       SKILL.md                   # Simple single-file skill
 .claude/
   skills/
     deploy-to-vercel -> ../../.agents/skills/deploy-to-vercel
     vercel-react-best-practices -> ../../.agents/skills/vercel-react-best-practices
 .cursor/
   skills/                       # (Cursor also uses .agents/skills directly)
 .windsurf/
   skills/
     deploy-to-vercel -> ../../.agents/skills/deploy-to-vercel
 .roo/
   skills/
     deploy-to-vercel -> ../../.agents/skills/deploy-to-vercel
 skills/                         # OpenClaw format
   deploy-to-vercel -> ../.agents/skills/deploy-to-vercel
 skills-lock.json                # Project-level lockfile
```

### With `--global` flag

Global skills go to the user's config directory:

| Agent | Global path |
|-------|-------------|
| Claude Code | `~/.claude/skills/` |
| Codex | `~/.codex/skills/` |
| Most agents | `~/.config/agents/skills/` (XDG) |
| Roo Code | `~/.roo/skills/` |
| Windsurf | `~/.windsurf/skills/` |

Global lockfile: `~/.agents/skills-lock.json`

---

## 3. How It Resolves 42 Agents

The CLI has a hardcoded `agents` object with 42 entries. Each agent is categorized as
either **universal** (shares `.agents/skills/`) or **native** (has its own directory).

### Resolution strategy

1. Real files are **always** placed in `.agents/skills/<skill-name>/`
2. For **universal agents** (those whose `skillsDir` is `.agents/skills/`): nothing
   extra needed -- they read from `.agents/skills/` directly
3. For **non-universal agents** (those with custom `skillsDir` like `.claude/skills/`):
   a **symlink** is created from `.<agent>/skills/<skill-name>` pointing to
   `../../.agents/skills/<skill-name>`
4. With `--copy` flag: files are **copied** instead of symlinked

### Universal agents (share `.agents/skills/`)

These agents all read from the same `.agents/skills/` directory, so only one copy of
files is needed:

- Amp
- Cline
- Codex
- Cursor
- Gemini CLI
- GitHub Copilot
- Kimi Code CLI
- OpenCode
- Warp

(Replit and Universal are hidden from the universal list via `showInUniversalList: false`)

### Non-universal agents (get symlinks)

Each of these gets its own `.<name>/skills/` directory with symlinks:

| Agent | Directory |
|-------|-----------|
| Antigravity | `.agent/skills/` |
| Augment | `.augment/skills/` |
| Claude Code | `.claude/skills/` |
| OpenClaw | `skills/` |
| CodeBuddy | `.codebuddy/skills/` |
| Command Code | `.commandcode/skills/` |
| Continue | `.continue/skills/` |
| Cortex Code | `.cortex/skills/` |
| Crush | `.crush/skills/` |
| Droid | `.factory/skills/` |
| Goose | `.goose/skills/` |
| Junie | `.junie/skills/` |
| iFlow CLI | `.iflow/skills/` |
| Kilo Code | `.kilocode/skills/` |
| Kiro CLI | `.kiro/skills/` |
| Kode | `.kode/skills/` |
| MCPJam | `.mcpjam/skills/` |
| Mistral Vibe | `.vibe/skills/` |
| Mux | `.mux/skills/` |
| OpenHands | `.openhands/skills/` |
| Pi | `.pi/skills/` |
| Pochi | `.pochi/skills/` |
| Qoder | `.qoder/skills/` |
| Qwen Code | `.qwen/skills/` |
| Roo Code | `.roo/skills/` |
| Trae | `.trae/skills/` |
| Trae CN | `.trae/skills/` (same as Trae) |
| Windsurf | `.windsurf/skills/` |
| Zencoder | `.zencoder/skills/` |
| Neovate | `.neovate/skills/` |
| AdaL | `.adal/skills/` |

---

## 4. Skill Format Validation

### What `parseSkillMd()` validates

The CLI validates SKILL.md files during discovery with these checks:

1. **Frontmatter must be valid YAML** (parsed via `gray-matter`)
2. **`name` field is required** and must be a string
3. **`description` field is required** and must be a string
4. **Internal skills are filtered** unless `metadata.internal` is `true` and the
   environment allows it

### SKILL.md frontmatter schema

```yaml
---
name: my-skill                        # REQUIRED: kebab-case identifier
description: What this skill does     # REQUIRED: when/how to use it
license: MIT                          # Optional
metadata:
  author: supernovae                  # Optional
  version: "1.0.0"                    # Optional (informational only)
  internal: false                     # Optional: hide from public discovery
---
```

### What `npx skills check` validates

`check` does NOT validate the SKILL.md format. It only checks for **content updates**:

1. Reads `skills-lock.json` for tracked skills
2. Groups skills by source repository
3. For each skill with a `skillFolderHash` and `skillPath`:
   - Fetches the current GitHub tree hash via API (`fetchSkillFolderHash`)
   - Compares against the stored `skillFolderHash`
4. Reports which skills have updates available
5. Skills without `skillFolderHash` or `skillPath` are "skipped" with a reason

There is no structural/schema validation command. The CLI validates on install, not as
a standalone linter.

---

## 5. Publishing a Skill

### Repository structure for a skill package

A skill package is just a GitHub repository containing `SKILL.md` files. The CLI
discovers skills by recursively scanning for `SKILL.md` files.

#### Single-skill repo

```
my-skill/
 SKILL.md
 README.md            # Optional: human-readable docs
 resources/           # Optional: scripts, data files
   deploy.sh
```

#### Multi-skill repo (recommended)

```
supernovae-st/nika-skills/
 README.md
 skills/
   nika-workflow-authoring/
     SKILL.md
     rules/
       syntax-basics.md
       verb-reference.md
   nika-mcp-integration/
     SKILL.md
   nika-media-pipeline/
     SKILL.md
     resources/
       example-workflows/
```

#### Priority search directories

The CLI searches these paths in priority order within a cloned repo:

1. Root `SKILL.md` (if found and `--full-depth` not set, stops here)
2. `skills/`
3. `skills/.curated/`
4. `skills/.experimental/`
5. `skills/.system/`
6. `.agent/skills/`
7. `.agents/skills/`
8. `.claude/skills/`
9. `.cline/skills/`
10. `.codebuddy/skills/`
11. `.codex/skills/`
12. `.commandcode/skills/`
13. Then any other subdirectories recursively (up to depth 5)

### Publishing workflow

1. Create a GitHub repo with SKILL.md files
2. Users install with `npx skills add owner/repo`
3. Skills appear on https://skills.sh automatically after first install
4. Install count tracked via telemetry (disable with `DISABLE_TELEMETRY=1`)

### Well-known discovery (for websites)

For non-GitHub sources, host a `.well-known/skills/index.json`:

```json
{
  "skills": [
    {
      "name": "my-skill",
      "path": "/skills/my-skill/SKILL.md"
    }
  ]
}
```

Then install with: `npx skills add https://mysite.com`

### npm distribution

Skills can also be distributed via npm packages. Include `SKILL.md` in the package,
then users run `npx skills experimental_sync` to discover and install from
`node_modules/`.

---

## 6. The 42 Supported Agents (Complete List)

| # | ID | Display Name | Skills Directory | Global Directory |
|---|-----|-------------|-----------------|-----------------|
| 1 | amp | Amp | `.agents/skills/` | `~/.config/agents/skills/` |
| 2 | antigravity | Antigravity | `.agent/skills/` | `~/.config/agents/skills/` |
| 3 | augment | Augment | `.augment/skills/` | `~/.config/agents/skills/` |
| 4 | claude-code | Claude Code | `.claude/skills/` | `~/.claude/skills/` |
| 5 | openclaw | OpenClaw | `skills/` | (custom) |
| 6 | cline | Cline | `.agents/skills/` | `~/.config/agents/skills/` |
| 7 | codebuddy | CodeBuddy | `.codebuddy/skills/` | `~/.config/agents/skills/` |
| 8 | codex | Codex | `.agents/skills/` | `~/.codex/skills/` |
| 9 | command-code | Command Code | `.commandcode/skills/` | `~/.config/agents/skills/` |
| 10 | continue | Continue | `.continue/skills/` | `~/.config/agents/skills/` |
| 11 | cortex | Cortex Code | `.cortex/skills/` | `~/.config/agents/skills/` |
| 12 | crush | Crush | `.crush/skills/` | `~/.config/agents/skills/` |
| 13 | cursor | Cursor | `.agents/skills/` | `~/.config/agents/skills/` |
| 14 | droid | Droid | `.factory/skills/` | `~/.config/agents/skills/` |
| 15 | gemini-cli | Gemini CLI | `.agents/skills/` | `~/.config/agents/skills/` |
| 16 | github-copilot | GitHub Copilot | `.agents/skills/` | `~/.config/agents/skills/` |
| 17 | goose | Goose | `.goose/skills/` | `~/.config/agents/skills/` |
| 18 | junie | Junie | `.junie/skills/` | `~/.config/agents/skills/` |
| 19 | iflow-cli | iFlow CLI | `.iflow/skills/` | `~/.config/agents/skills/` |
| 20 | kilo | Kilo Code | `.kilocode/skills/` | `~/.config/agents/skills/` |
| 21 | kimi-cli | Kimi Code CLI | `.agents/skills/` | `~/.config/agents/skills/` |
| 22 | kiro-cli | Kiro CLI | `.kiro/skills/` | `~/.config/agents/skills/` |
| 23 | kode | Kode | `.kode/skills/` | `~/.config/agents/skills/` |
| 24 | mcpjam | MCPJam | `.mcpjam/skills/` | `~/.config/agents/skills/` |
| 25 | mistral-vibe | Mistral Vibe | `.vibe/skills/` | `~/.config/agents/skills/` |
| 26 | mux | Mux | `.mux/skills/` | `~/.config/agents/skills/` |
| 27 | opencode | OpenCode | `.agents/skills/` | `~/.config/agents/skills/` |
| 28 | openhands | OpenHands | `.openhands/skills/` | `~/.config/agents/skills/` |
| 29 | pi | Pi | `.pi/skills/` | `~/.config/agents/skills/` |
| 30 | pochi | Pochi | `.pochi/skills/` | `~/.config/agents/skills/` |
| 31 | qoder | Qoder | `.qoder/skills/` | `~/.config/agents/skills/` |
| 32 | qwen-code | Qwen Code | `.qwen/skills/` | `~/.config/agents/skills/` |
| 33 | replit | Replit | `.agents/skills/` | `~/.config/agents/skills/` |
| 34 | roo | Roo Code | `.roo/skills/` | `~/.roo/skills/` |
| 35 | trae | Trae | `.trae/skills/` | `~/.trae/skills/` |
| 36 | trae-cn | Trae CN | `.trae/skills/` | `~/.trae/skills/` |
| 37 | warp | Warp | `.agents/skills/` | `~/.config/agents/skills/` |
| 38 | windsurf | Windsurf | `.windsurf/skills/` | `~/.windsurf/skills/` |
| 39 | zencoder | Zencoder | `.zencoder/skills/` | `~/.config/agents/skills/` |
| 40 | neovate | Neovate | `.neovate/skills/` | `~/.config/agents/skills/` |
| 41 | adal | AdaL | `.adal/skills/` | `~/.config/agents/skills/` |
| 42 | universal | Universal | `.agents/skills/` | `~/.config/agents/skills/` |

---

## 7. Nika Skill Package Design

### Repository: `github.com/supernovae-st/nika-skills`

```
supernovae-st/nika-skills/
 README.md
 SKILL.md                              # Root skill (workflow authoring guide)
 skills/
   nika-workflow-authoring/
     SKILL.md                          # 5 verbs, syntax, with: bindings
     rules/
       verb-infer.md
       verb-exec.md
       verb-fetch.md
       verb-invoke.md
       verb-agent.md
       bindings-and-templates.md
       dag-and-dependencies.md
       error-handling.md
   nika-media-pipeline/
     SKILL.md                          # CAS, media tools, pipeline chaining
     rules/
       cas-basics.md
       tier1-always-on.md
       tier2-media-core.md
       tier3-opt-in.md
       pipeline-chaining.md
   nika-mcp-integration/
     SKILL.md                          # NovaNet MCP, invoke verb, aliases
     rules/
       mcp-connection.md
       invoke-patterns.md
       100-aliases.md
   nika-fetch-extraction/
     SKILL.md                          # 9 extract modes, response modes
     rules/
       extract-modes.md
       response-modes.md
       scraping-patterns.md
   nika-course-exercises/
     SKILL.md                          # 12-level Liberation course guide
   nika-cli-reference/
     SKILL.md                          # CLI commands quick reference
```

### Root SKILL.md example

```yaml
---
name: nika-workflow-authoring
description: >
  Write Nika semantic YAML workflows (.nika.yaml). Use when creating AI task
  pipelines, chaining LLM inference with shell commands, HTTP requests, and MCP
  tool calls. Triggers on tasks involving workflow files, DAG construction,
  template bindings, or the 5 Nika verbs (infer, exec, fetch, invoke, agent).
license: AGPL-3.0-or-later
metadata:
  author: supernovae-st
  version: "0.39.1"
---

# Nika Workflow Authoring

Nika is a semantic YAML workflow engine for AI tasks, schema `nika/workflow@0.12`.

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize {{with.data}}"` |
| `exec:` | Shell command | `exec: "cat data.csv"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com/data"` |
| `invoke:` | MCP tool call | `invoke: "nika:thumbnail"` |
| `agent:` | Multi-turn loop | `agent: "Research this topic"` |

...
```

### Install command

```bash
# All Nika skills
npx skills add supernovae-st/nika-skills --all

# Specific skill only
npx skills add supernovae-st/nika-skills --skill nika-workflow-authoring

# For Claude Code only
npx skills add supernovae-st/nika-skills --agent claude-code

# Global install
npx skills add supernovae-st/nika-skills -g --all
```

---

## 8. Cross-Agent Installation Behavior

When you run `npx skills add`, this is the exact flow:

1. **Clone/fetch** the source repository to a temp directory
2. **Discover** all `SKILL.md` files in the repo
3. **Prompt** user to select which skills and which agents (unless `--all`)
4. **Copy** real files to `.agents/skills/<skill-name>/`
5. **Create symlinks** from each selected non-universal agent directory:
   - `.claude/skills/<skill-name>` -> `../../.agents/skills/<skill-name>`
   - `.windsurf/skills/<skill-name>` -> `../../.agents/skills/<skill-name>`
   - `.roo/skills/<skill-name>` -> `../../.agents/skills/<skill-name>`
   - etc.
6. **Universal agents** (Amp, Cline, Codex, Cursor, Gemini CLI, etc.) need no
   symlinks because they read `.agents/skills/` directly
7. **Write lockfile** (`skills-lock.json`) with source, hash, and timestamps

### Key insight

The canonical store is ALWAYS `.agents/skills/`. Agent directories are just
symlink facades. This means:

- Deleting `.agents/skills/<name>/` removes it for ALL agents
- Editing a file in `.claude/skills/<name>/` edits it for ALL agents (it is the
  same file via symlink)
- With `--copy`, each agent gets its own independent copy (no symlinks)

---

## 9. Version Pinning

**There is no semantic version pinning.** The system uses **content-hash integrity**
instead:

### Lockfile format (`skills-lock.json`)

```json
{
  "version": 1,
  "skills": {
    "deploy-to-vercel": {
      "source": "vercel-labs/agent-skills",
      "sourceType": "github",
      "computedHash": "d2b5bcc7c09fdb649129ffc7dd7b647ec1b155736974b974292675afd588cb39",
      "sourceUrl": "https://github.com/vercel-labs/agent-skills.git",
      "skillPath": "skills/deploy-to-vercel",
      "skillFolderHash": "abc123...",
      "installedAt": "2026-03-23T00:49:00.000Z",
      "updatedAt": "2026-03-23T00:49:00.000Z"
    }
  }
}
```

### How integrity works

- `computedHash`: SHA-256 of all files in the local skill directory (computed
  after installation by `computeSkillFolderHash()`)
- `skillFolderHash`: Hash of the skill folder as it exists on GitHub at
  install time (fetched via GitHub API tree endpoint)
- `npx skills check` compares `skillFolderHash` against GitHub HEAD
- `npx skills update` re-clones and replaces files

### What you cannot do

- Pin to a specific git tag or commit (always uses default branch HEAD)
- Lock to a semver range
- Prevent auto-updates (manual `npx skills update` required)

### Workaround for pinning

Use the GitHub tree URL with a ref:
```bash
npx skills add https://github.com/owner/repo/tree/v1.0.0/skills/my-skill
```

---

## 10. The Skills Registry / Marketplace

### skills.sh

The central registry is at **https://skills.sh/**. It is NOT a publish target --
skills are automatically indexed when users install them.

### API endpoint

```
GET https://skills.sh/api/search?q={query}&limit={n}
```

Response:
```json
{
  "query": "rust",
  "searchType": "fuzzy",
  "skills": [
    {
      "id": "apollographql/skills/rust-best-practices",
      "skillId": "rust-best-practices",
      "name": "rust-best-practices",
      "installs": 4315,
      "source": "apollographql/skills"
    }
  ],
  "count": 5,
  "duration_ms": 44
}
```

### How skills appear on the registry

1. User runs `npx skills add owner/repo`
2. CLI sends telemetry to `https://add-skill.vercel.sh/t` with install event
3. skills.sh indexes the repository and tracks install counts
4. Skills become searchable via `npx skills find` and the website

### Security auditing

On install, the CLI fetches audit data from `https://add-skill.vercel.sh/audit`:

- **Gen AI analysis**: Checks if skill content is safe
- **Socket alerts**: Dependency vulnerability scanning
- **Snyk risk level**: Low/Medium/High risk assessment

Results are displayed during installation and on skills.sh skill pages.

### Disable telemetry

```bash
DISABLE_TELEMETRY=1 npx skills add owner/repo
# or
DO_NOT_TRACK=1 npx skills add owner/repo
```

---

## Key Takeaways for Nika Distribution

### Immediate opportunity

1. Create `github.com/supernovae-st/nika-skills` with the structure in section 7
2. Each SKILL.md becomes a knowledge module for any AI agent
3. The `description` field is critical -- it determines when agents activate the skill
4. Skills can include `.nika.yaml` example files alongside the Markdown

### What skills.sh gives you

- Instant discoverability via `npx skills find nika`
- Install tracking (vanity metric + momentum signal)
- Cross-agent distribution with zero extra work
- Security audit badge on the registry page

### Limitations to be aware of

- **No version pinning** -- users always get HEAD of default branch
- **No programmatic validation** beyond name+description presence
- **No execution model** -- skills are pure Markdown instructions, not executable code
- **Telemetry is opt-out**, not opt-in
- **Agent detection is heuristic** -- checks for config directories on disk

### Strategic consideration

The skills format is essentially "CLAUDE.md as a package." For Nika, the most
valuable skills would be:

1. **Workflow authoring** -- teach any AI agent to write `.nika.yaml` files
2. **Verb reference** -- precise syntax for all 5 verbs with examples
3. **Error code lookup** -- NIKA-XXX error resolution guides
4. **Course creation** -- help agents generate course exercises

This lets Claude Code, Cursor, Windsurf, Copilot, and any other agent become
Nika-literate without users reading docs manually.

---

## Sources

1. CLI source: `~/.npm/_npx/*/node_modules/skills/dist/cli.mjs` (v1.4.5, decompiled)
2. GitHub: https://github.com/vercel-labs/skills
3. Agent skills example: https://github.com/vercel-labs/agent-skills
4. Registry: https://skills.sh/
5. API: `https://skills.sh/api/search`
6. Audit API: `https://add-skill.vercel.sh/audit`

## Methodology

- Tools: `npx skills` CLI (live testing), source code grep, GitHub API, curl
- Pages analyzed: ~15 source files, 6 SKILL.md files, 2 API endpoints
- Confidence: **High** -- all findings verified against live CLI v1.4.5 and source code
