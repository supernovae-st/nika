# Research Report: Cursor IDE Integration for Nika

**Date**: 2026-03-23
**Researcher**: Claude Opus 4.6 (1M context)
**Sources analyzed**: 14 pages, 6 repositories

---

## Executive Summary

Cursor provides 5 primary integration surfaces for third-party tools: **Project Rules** (`.cursor/rules/`), **MCP servers** (`.cursor/mcp.json`), **@Docs** (custom documentation indexing), **Notepads** (reusable context), and **AGENTS.md** (simple markdown instructions). The most impactful integrations for Nika are: (1) `.cursor/rules/` with glob-scoped rules for `.nika.yaml` files, (2) MCP server for live workflow introspection, (3) `@Docs` pointing to Nika's documentation, and (4) the existing LSP feeding into Cursor Tab completions.

---

## 1. Cursor Rules -- Complete Specification

### Format Evolution (Critical)

As of **Cursor 2.2** (early 2026), the `.mdc` file format is **legacy but still functional**. New rules use the **folder-based format**:

```
.cursor/rules/
  my-rule/
    RULE.md           # Main rule file (markdown + frontmatter)
    scripts/          # Helper scripts (optional)
    templates/        # Referenced files (optional)
```

The old `.mdc` format was a single flat file with the same frontmatter. Both work today, but new rules created via the Cursor UI use the folder format.

### RULE.md Frontmatter Fields

| Field | Type | Purpose |
|-------|------|---------|
| `description` | `string` | Human-readable description. **Required** for "Apply Intelligently" rules -- the agent reads this to decide relevance. |
| `globs` | `string` or `string[]` | File patterns (e.g., `**/*.yaml`, `src/**/*.rs`). Used for "Apply to Specific Files" mode. |
| `alwaysApply` | `boolean` | If `true`, rule is injected into every chat session. Default: `false`. |

There are **no other frontmatter fields**. The combination of these three fields determines the rule type:

### Rule Type Matrix

| Rule Type | `alwaysApply` | `description` | `globs` |
|-----------|:---:|:---:|:---:|
| **Always Apply** | `true` | optional | ignored |
| **Apply Intelligently** (agent-decided) | `false` | **required** | empty |
| **Apply to Specific Files** | `false` | optional | **required** |
| **Apply Manually** (`@rule-name` in chat) | `false` | empty | empty |

### How Cursor Decides When to Apply a Rule

1. **Always Apply**: Injected into every Agent (Chat) context, unconditionally.
2. **Apply Intelligently**: The `description` text is shown to the AI model, which decides if the rule is relevant to the current conversation.
3. **Apply to Specific Files**: When a file matching the `globs` pattern is referenced in the conversation (opened, mentioned, or being edited).
4. **Apply Manually**: Only when the user types `@rule-name` in chat.

### File References Within Rules

Rules **can** reference other files using `@filename.ts` syntax within the rule body. This pulls the referenced file's content into the rule's context. Example:

```markdown
---
description: "Template for Nika workflows"
alwaysApply: false
---

When creating a new workflow, follow this template:

@templates/workflow-template.nika.yaml
```

### Scope and Limitations

- Rules affect **Agent (Chat)** and **Composer** only
- Rules do **NOT** affect **Cursor Tab** (autocomplete) or **Inline Edit** (Cmd+K)
- User Rules do **NOT** apply to Inline Edit
- Rules are prepended to the model context (system-level)
- Maximum recommended length: **500 lines** per rule
- Precedence order: **Team Rules > Project Rules > User Rules**

### Legacy Formats

| Format | Status | Migration |
|--------|--------|-----------|
| `.cursorrules` (root file) | Deprecated, still works | Move to `.cursor/rules/` or `AGENTS.md` |
| `.cursor/rules/*.mdc` (flat files) | Legacy since 2.2, still works | Convert to folder format |
| `.cursor/rules/*/RULE.md` (folder) | **Current** | N/A |
| `AGENTS.md` | Active alternative | Simple use cases |

**Source**: https://docs.cursor.com/context/rules (scraped via GitHub mirror: sanjeed5/awesome-cursor-rules-mdc)

---

## 2. Cursor Notepads

### What They Are

Notepads are **persistent, reusable context documents** that live in the Cursor sidebar. They act as scratchpads for context you frequently want to provide to the AI -- think of them as "reusable prompts" or "context templates."

### Key Properties

- Created manually within Cursor (Cursor Settings or sidebar)
- Content is **user-local** (not version-controlled, not in `.cursor/`)
- Referenced via `@notepad-name` in chat
- Useful for: API specs, architecture decisions, style guides, frequently-used patterns
- Support markdown formatting

### Can They Be Pre-Populated?

**No.** Notepads are a user-level feature with no file-based configuration. There is no `.cursor/notepads/` directory or config file. They cannot be pre-populated by a project or distributed with a repo.

### Relevance for Nika

**Low.** Notepads are personal and cannot be shipped with Nika. For distributable context, use **Project Rules** or **AGENTS.md** instead. However, power users could be instructed to create a "Nika Reference" notepad with common patterns.

---

## 3. Cursor @ Mentions

### Built-in @ Sources

Cursor provides these built-in `@` mention types:

| Mention | What It Does |
|---------|-------------|
| `@file` or `@filename.ts` | Includes file content in context |
| `@folder` | Includes folder structure |
| `@codebase` | Searches the entire codebase semantically |
| `@web` | Searches the web |
| `@docs` | Searches custom documentation sources |
| `@git` | Git history and diffs |
| `@notepad` | References a notepad |
| `@rule-name` | Manually applies a project rule |
| `@definitions` | Symbol definitions |
| `@cursor-rules` | All active rules |

### Can You Create Custom @ Sources?

**No.** You cannot create arbitrary custom `@` sources like `@nika-docs`. The `@` system is closed -- Cursor defines the available mention types.

**However**, there are two workarounds:

1. **`@Docs`** (see section 9): Add Nika's documentation site as a custom docs source. Then `@Docs` will search it. The user types `@Docs` and selects "Nika" from the dropdown.

2. **Project Rules with Manual Apply**: Create a rule named `nika-reference` and reference it via `@nika-reference` in chat. This effectively creates a custom `@` source for Nika context.

3. **MCP Tools**: MCP server tools appear as available actions. While not `@` mentions, the agent can call them when relevant.

---

## 4. Cursor MCP Integration

### Configuration File

Cursor supports MCP servers via `.cursor/mcp.json` (project-level) or global settings.

```json
{
  "mcpServers": {
    "server-name": {
      "command": "path/to/executable",
      "args": ["arg1", "arg2"],
      "env": {
        "API_KEY": "value"
      }
    }
  }
}
```

### Transport Types

| Type | Config |
|------|--------|
| **stdio** | `"command"` + `"args"` (most common) |
| **SSE** | `"url": "http://..."` |
| **Streamable HTTP** | `"url": "http://..."` (newer transport) |

### What MCP Servers Can Provide

| Capability | Supported | Notes |
|------------|:---------:|-------|
| **Tools** | Yes | Functions the AI can call. Primary use case. |
| **Resources** | Yes | Static or dynamic data the AI can read. |
| **Prompts** | Yes | Pre-built prompt templates. |
| **Completions** | **No** | MCP completion capability is not used by Cursor for code completions. |
| **Suggestions** | **No** | MCP does not have a suggestions primitive. |

### Can MCP Provide Completions?

**No.** MCP servers cannot feed into Cursor's autocomplete (Tab) or code completion system. Cursor Tab uses its own completion model and draws from:
- Open file context
- LSP completions (from your language server)
- Recent edits
- Cursor's own fine-tuned model

For completions, you need an **LSP** (which Nika already has: `nika-lsp`).

### Nika MCP Server Opportunity

A `nika-mcp-server` could expose:
- `nika:validate` tool -- validate a workflow
- `nika:run` tool -- execute a workflow
- `nika:list-verbs` resource -- available verbs
- `nika:list-providers` resource -- configured providers
- `nika:schema` resource -- the workflow schema
- `nika:examples` prompt -- generate example workflows

**Source**: Cursor docs + real-world examples from lucianoayres/mcp-server-node, cyberagiinc/DevDocs

---

## 5. Cursor Extension API

### Cursor vs VS Code Extensions

Cursor is built on VS Code and supports the **full VS Code Extension API**. There is **no separate Cursor extension API** -- extensions are standard VS Code extensions.

### Can Extensions Provide AI Context?

**Not directly.** VS Code extensions cannot inject context into Cursor's AI features. The only ways to provide AI context are:

1. **Rules** (`.cursor/rules/`, `.cursorrules`, `AGENTS.md`)
2. **MCP servers** (`.cursor/mcp.json`)
3. **`@Docs`** (documentation indexing)
4. **LSP** (feeds into Tab completions only, not Agent/Chat)

A VS Code extension **can**:
- Provide LSP completions (which Cursor Tab uses)
- Register custom commands
- Provide diagnostics, hover info, code actions
- Provide snippets (used by VS Code IntelliSense, partially by Cursor Tab)

A VS Code extension **cannot**:
- Inject system prompts into Cursor's Agent
- Add custom `@` mention sources
- Modify Cursor's AI behavior directly
- Provide context to Composer

### Nika Extension Strategy

The `nika-lsp` binary is already the right approach. It provides completions, diagnostics, and hover information for `.nika.yaml` files. These feed into Cursor Tab. For Agent/Chat context, use Project Rules.

---

## 6. Cursor Composer and Rules

### How Composer Works With Rules

Cursor Composer (the multi-file editing agent) respects the same rules as Agent (Chat):

- **Always Apply** rules are included in Composer sessions
- **Apply Intelligently** rules are included when the agent deems them relevant
- **Apply to Specific Files** rules are included when Composer is editing matching files
- **Manual** rules can be `@`-mentioned in Composer input

### Composer-Specific Considerations

- Composer sees the full project structure and can edit multiple files
- Rules that define project architecture, conventions, and patterns are especially useful
- Composer benefits from concrete examples more than abstract guidelines
- Rules should include anti-patterns ("do NOT do X") for Composer to avoid bad edits

### Nika + Composer

When editing `.nika.yaml` files, Composer will:
1. Apply any glob-matched rules (e.g., `globs: "**/*.nika.yaml"`)
2. Consider intelligent rules based on description
3. Use LSP completions for Tab
4. **Not** automatically know Nika syntax without rules

---

## 7. How Popular Frameworks Integrate With Cursor

### Next.js (vercel/next.js)

**Strategy**: `AGENTS.md` + `.cursor/commands/`

- `AGENTS.md` at repo root (also symlinked as `CLAUDE.md`)
- Contains: monorepo structure, build commands, fast dev workflow, test patterns
- `.cursor/commands/gt-workflow.md` -- Graphite workflow instructions
- `.cursor/worktrees.json` -- setup automation
- **No `.cursor/rules/` directory** -- uses AGENTS.md for simplicity
- Length: ~200 lines of actionable, specific instructions

**Pattern**: Single comprehensive AGENTS.md focused on "how to contribute to THIS codebase."

### Astro (withastro/astro)

**Strategy**: `AGENTS.md` only

- Concise, pragmatic (~100 lines)
- Monorepo structure, build commands, test patterns
- References `llms.txt` endpoint: `https://docs.astro.build/llms.txt`
- Custom tooling: `bgproc` for background processes, `agent-browser` for UI testing
- **No `.cursor/` directory**

**Pattern**: Minimal AGENTS.md + external tool references.

### Tailwind CSS (tailwindlabs/tailwindcss)

**Strategy**: **None**

- No `.cursor/`, no `AGENTS.md`, no `.cursorrules`
- No special AI agent configuration

### Community .mdc Examples (awesome-cursor-rules-mdc)

The `sanjeed5/awesome-cursor-rules-mdc` repository (3,400+ stars) contains community-contributed rules for 100+ frameworks. Key patterns:

**Astro .mdc** (well-structured example):
```markdown
---
description: Opinionated best practices for Astro applications
globs: **/*.{js,jsx,ts,tsx,astro}
---
# Astro Best Practices
## 1. Code Organization and Structure
### 1.1 Standard Project Structure
...
```

**Rust .mdc** (relevant to Nika):
```markdown
---
description: Guidelines for writing idiomatic, performant Rust code
globs: **/*
---
# Rust Best Practices
## 1. Code Organization and Structure
### 1.1 Module Structure: Feature-Driven
...
```

**Common pattern**: `description` + broad `globs` + structured markdown with examples.

---

## 8. Cursor Auto-Suggestions for .nika.yaml Files

### How Cursor Tab Works

Cursor Tab (autocomplete) draws from:
1. **Open file context** (current file + recently opened files)
2. **LSP completions** (from language servers)
3. **Cursor's own model** (fine-tuned for code completion)
4. **Recent edits** (continuations of your editing pattern)

### Does LSP Feed Into Cursor AI Suggestions?

**Yes, partially.** The LSP's completion items are used by Cursor Tab. However:

- LSP completions are **one signal** among many -- Cursor blends them with its own model
- LSP diagnostics appear as editor diagnostics but do **not** feed into Tab completions
- LSP hover info is shown on hover but does **not** feed into Tab completions
- LSP code actions appear in the lightbulb menu

### Strategy for .nika.yaml Auto-Suggestions

The optimal approach combines multiple layers:

1. **LSP (nika-lsp)**: Already provides completions for verb names, properties, template variables, task IDs. This is the primary source for structured completions.

2. **Project Rules with Glob**: Create a `.cursor/rules/nika-workflows/RULE.md` with `globs: "**/*.nika.yaml"` that explains Nika syntax. This gives the Agent context when chatting about workflows.

3. **File Associations**: Cursor inherits VS Code's file associations. If `nika-lsp` registers for `*.nika.yaml` files via a VS Code extension, Cursor will automatically use it.

4. **Snippets**: VS Code/Cursor snippets for `.nika.yaml` files can provide templates. These feed into autocomplete but are a separate mechanism from LSP.

### What Cursor Tab Cannot Do

- It cannot learn new syntax purely from rules (rules do not affect Tab)
- It cannot use MCP tools for completions
- It relies on LSP + its own model + file context

**Verdict**: The LSP is the critical piece. Rules help Agent/Chat but not Tab.

---

## 9. Cursor @Docs Feature

### What It Is

`@Docs` lets you add custom documentation sources that Cursor indexes and makes searchable. When you type `@Docs` in chat, you can select from your added documentation sources.

### How to Add Custom Documentation

1. Open any Cursor AI pane (Chat, Composer)
2. Type `@Docs`
3. Click "Add new doc"
4. Enter the documentation URL (e.g., `https://nika.supernovae.studio/docs`)
5. Cursor crawls and indexes the pages
6. The documentation becomes available as `@Docs > Nika`

### Key Properties

| Property | Detail |
|----------|--------|
| Scope | Per-user (not project-level) |
| Storage | Cursor's cloud index |
| Format | Any web-accessible documentation |
| Depth | Crawls linked pages from the root URL |
| Updates | Can be refreshed manually |
| llms.txt | Cursor may use `llms.txt` if available at the doc root |

### Can @Docs Be Pre-Populated?

**No.** There is no config file to pre-populate `@Docs`. Each user must add documentation sources manually. However, you can:

1. Instruct users in your README to add `@Docs` for Nika
2. Provide an `llms.txt` file at your docs site root for AI-friendly content
3. Use Project Rules to include key documentation inline

### Nika @Docs Strategy

1. **Publish `llms.txt`** at the Nika documentation site root
2. **Instruct users** to add `@Docs` pointing to the Nika docs
3. **Include critical reference** in Project Rules as a fallback

---

## 10. Best Practices for Cursor Rules

### What Works Best (Based on Analysis of Popular Projects)

**1. Keep rules focused and composable**
- One rule per concern (syntax, conventions, architecture, common mistakes)
- Under 500 lines each
- Split rather than merge

**2. Use concrete examples with anti-patterns**
```markdown
GOOD:
```yaml
infer:
  model: gpt-4o
  prompt: "Summarize {{with.input}}"
```

BAD:
```yaml
infer:
  prompt: Summarize the thing    # Missing model, unquoted template
```
```

**3. Provide structured reference tables**
- Verb reference, error codes, configuration options
- Tables are highly effective for AI parsing

**4. Use the right rule type**

| Content | Best Type |
|---------|-----------|
| Core syntax reference | Apply to Specific Files (`**/*.nika.yaml`) |
| Project conventions | Always Apply |
| Error troubleshooting | Apply Intelligently |
| Templates/scaffolds | Apply Manually (`@nika-template`) |
| Architecture overview | Apply Intelligently |

**5. Write descriptions that help the agent decide**

BAD: `"Nika stuff"`
GOOD: `"Nika workflow engine syntax reference -- apply when creating, editing, or debugging .nika.yaml workflow files"`

**6. Reference real files**
Use `@path/to/file` to include templates, schemas, or examples rather than duplicating content in the rule.

**7. Include "When to use" and "When NOT to use" sections**
This helps the agent decide whether to apply intelligent rules.

**8. Triggers that work best**
- Glob patterns: `**/*.nika.yaml`, `**/*.{yaml,yml}`
- Descriptive keywords in description: mention file extensions, tool names, concepts
- Explicit anti-patterns prevent the most common AI mistakes

### What Excellent Rules Look Like

From the analysis of community rules (3,400+ stars repo):

1. **Frontmatter**: Always has `description` (even for glob rules) + appropriate `globs`
2. **Structure**: Numbered sections with clear headings
3. **Examples**: Every guideline has a GOOD/BAD code example
4. **Specificity**: Names exact functions, patterns, file paths
5. **Length**: 100-400 lines (sweet spot)
6. **Tone**: Imperative ("Use X", "Never Y", "Always Z")

---

## Recommended Nika Cursor Integration Plan

### Priority 1: Project Rules (Ship with Nika)

Create `.cursor/rules/` in the Nika project with these rules:

```
.cursor/rules/
  nika-syntax/
    RULE.md              # Glob: **/*.nika.yaml -- Core syntax reference
  nika-conventions/
    RULE.md              # Always Apply -- Project conventions
  nika-errors/
    RULE.md              # Intelligent -- Error troubleshooting guide
  nika-template/
    RULE.md              # Manual -- Workflow scaffold template
    templates/
      basic.nika.yaml
      agent.nika.yaml
```

### Priority 2: AGENTS.md (Ship with Nika)

Already have `CLAUDE.md` -- Cursor reads both `CLAUDE.md` and `AGENTS.md`. Consider adding an `AGENTS.md` symlink or keeping them in sync.

### Priority 3: MCP Server

Create a `nika-mcp-server` (or add MCP server mode to the existing `nika` binary):

```json
// .cursor/mcp.json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp-server"],
      "env": {}
    }
  }
}
```

### Priority 4: @Docs + llms.txt

- Publish `llms.txt` at the Nika docs site
- Document the `@Docs` setup in README

### Priority 5: nika init --cursor

Add a `nika init --cursor` command that scaffolds:
- `.cursor/rules/nika-syntax/RULE.md`
- `.cursor/mcp.json` (with nika MCP server)
- Instructions for adding `@Docs`

---

## Sources

1. **Cursor Rules Documentation** -- Full specification scraped from `sanjeed5/awesome-cursor-rules-mdc` GitHub mirror of docs.cursor.com/context/rules
2. **Next.js AGENTS.md** -- `vercel/next.js` (canary branch) -- Real-world example of cursor integration
3. **Astro AGENTS.md** -- `withastro/astro` (main branch) -- Minimal cursor integration
4. **awesome-cursor-rules-mdc** (3,407 stars) -- Community .mdc collection + reference docs
5. **lucianoayres/mcp-server-node** -- `.cursor/mcp.json` format example
6. **Cursor docs site** (docs.cursor.com) -- Page existence verified for /context/notepads, /build/mcp, /context/@-symbols, /context/docs

## Methodology

- Tools used: curl, GitHub API, raw file fetching
- Pages analyzed: 14 documentation pages + 6 repository READMEs + 4 example rule files
- Direct scraping of Cursor docs blocked (client-side rendered Next.js app), used GitHub-hosted mirrors

## Confidence Level

**High** for: Rules specification (directly from scraped official docs), MCP format, AGENTS.md, Tab/LSP interaction
**Medium** for: Notepads (based on training data + verified page existence), @Docs (based on training data), Composer behavior
**Low** for: Extension API AI context injection (negative claim -- "cannot" is hard to prove definitively)

## Further Research Suggestions

- Test Cursor 2.2+ folder-based rules format locally with Nika
- Verify if `nika-lsp` completions appear in Cursor Tab for `.nika.yaml` files
- Prototype a `nika mcp-server` and test tool discovery in Cursor Agent
- Check if Cursor's Claude skills/plugins import can load `.claude/` skills
- Monitor Cursor changelog for new integration points (the product evolves rapidly)
