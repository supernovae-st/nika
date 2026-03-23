# Research Report: Cursor AI Rules, AGENTS.md, CLAUDE.md & AI Coding Assistant Instructions

**Date:** 2026-03-23
**Researcher:** Claude Opus 4.6 (1M context)
**Pages analyzed:** 12+
**Tools used:** curl, web archive, GitHub API, raw GitHub

---

## Summary

Cursor rules have evolved significantly in 2025-2026. The `.cursorrules` root file is **deprecated**; the modern approach uses `.cursor/rules/*.mdc` files with frontmatter metadata controlling when rules apply. Four rule types exist: Always Apply, Apply Intelligently, Apply to Specific Files, and Apply Manually. Meanwhile, `AGENTS.md` has emerged as a cross-tool standard backed by GitHub Copilot and Cursor, and Claude Code's `CLAUDE.md` offers the richest hierarchical instruction system. All three ecosystems are converging on similar patterns: scoped, version-controlled, markdown-based instruction files.

---

## 1. Cursor Rules: .cursorrules vs .cursor/rules/*.mdc

### .cursorrules (LEGACY -- DEPRECATED)

- Single file in project root
- Still supported but **will be deprecated**
- Cursor officially recommends migrating to Project Rules or `AGENTS.md`
- Source: [Cursor Docs - Rules](https://docs.cursor.com/docs/context/rules)

### .cursor/rules/*.mdc (CURRENT)

Each rule is a separate `.mdc` file (Markdown with metadata) in `.cursor/rules/`. The format:

```
---
description: When to apply this rule (used by AI for auto-detection)
globs: "**/*.nika.yaml"
alwaysApply: false
---

# Rule content in Markdown
Your instructions here...
```

**Key properties in frontmatter:**
- `description` -- Natural language description; critical for "Apply Intelligently" type
- `globs` -- File patterns (glob syntax) for "Apply to Specific Files" type
- `alwaysApply` -- Boolean; when `true`, rule loads in every session

Source: [Cursor Docs (web archive)](https://web.archive.org/web/2025/https://docs.cursor.com/context/rules)

---

## 2. Four Rule Types (Activation Modes)

| Rule Type | `alwaysApply` | `globs` | `description` | When Applied |
|-----------|---------------|---------|---------------|--------------|
| `Always Apply` | `true` | empty | optional | Every chat session |
| `Apply Intelligently` | `false` | empty | **required** | When Agent decides it's relevant based on description |
| `Apply to Specific Files` | `false` | **set** | optional | When file matches the glob pattern |
| `Apply Manually` | `false` | empty | empty | Only when @-mentioned in chat |

**Important nuances:**
- "Apply Intelligently" requires a good `description` field -- without it, the rule won't be found
- "Apply to Specific Files" uses standard glob syntax: `**/*.rs`, `src/**/*.yaml`, etc.
- Rules **only affect Agent (Chat)** -- they do NOT impact Cursor Tab or Inline Edit (Cmd+K)
- User Rules also do NOT apply to Inline Edit

Source: [Cursor Docs - Rules](https://docs.cursor.com/docs/context/rules)

---

## 3. Nested Rules (Monorepo Support)

Cursor supports `.cursor/rules/` directories at any level of the project tree:

```
project/
  .cursor/rules/        # Project-wide rules
  backend/
    server/
      .cursor/rules/    # Backend-specific rules
  frontend/
    .cursor/rules/      # Frontend-specific rules
```

Nested rules **automatically attach** when files in their directory are referenced. This is ideal for monorepos.

---

## 4. Rule Precedence

Rules are applied in this order (earlier sources take precedence on conflicts):

```
Team Rules > Project Rules > User Rules
```

- **Team Rules**: Dashboard-managed, plain text (no MDC format), enforced across team
- **Project Rules**: `.cursor/rules/*.mdc` files, version-controlled
- **User Rules**: Global in Cursor Settings > Rules, personal preferences

---

## 5. Best Practices from Cursor Official Docs

Direct from cursor.com/docs/context/rules:

1. **Keep rules under 500 lines**
2. **Split large rules into multiple, composable rules**
3. **Provide concrete examples or referenced files**
4. **Avoid vague guidance -- write rules like clear internal docs**
5. **Reuse rules when repeating prompts in chat**
6. Good rules are **focused, actionable, and scoped**

### File References

Rules can reference other files using `@filename.ts` syntax. This includes the referenced file in the rule's context. Confirmed in FAQ:
> "Can rules reference other rules or files? Yes. Use `@filename.ts` to include files in your rule's context."

### Creating Rules

Two methods:
- `New Cursor Rule` command in command palette
- `Cursor Settings > Rules` (shows all rules and their status)
- You can also ask the agent to create a new rule from chat

---

## 6. Cursor Rules for Domain-Specific Languages / YAML DSL

For a YAML DSL like Nika, the best strategy combines:

1. **A glob-scoped rule** for the file extension: `globs: "**/*.nika.yaml"`
   - Contains syntax reference, common mistakes, verb patterns
   - This is exactly what you already have in `nika-workflows.mdc`

2. **An "Always Apply" rule** for general project conventions:
   - Error codes, architecture patterns, testing commands
   - `alwaysApply: true`

3. **"Apply Intelligently" rules** for specific subsystems:
   - `description: "MCP integration patterns and invoke verb usage"`
   - `description: "Media pipeline tools and CAS storage patterns"`

The existing `nika-workflows.mdc` (189 lines, glob-scoped) is well within best practices.

---

## 7. Popular Project Cursor Rules Patterns

### rules_template (1,063 stars)
- **Structure**: 6 `.mdc` files in `.cursor/rules/`:
  - `rules.mdc` -- Always-on general rules
  - `plan.mdc` -- Planning workflow (always-on)
  - `implement.mdc` -- Implementation workflow
  - `debug.mdc` -- Debugging workflow
  - `memory.mdc` -- Documentation/context structure
  - `directory-structure.mdc` -- Project layout
- Uses symlinks to share rules across Cursor/CLINE/RooCode
- Source: https://github.com/Bhartendu-Kumar/rules_template

### rulebook-ai (580 stars)
- Cross-tool rule generator (Cursor, Copilot, Claude Code, Gemini CLI, Codex CLI)
- "Packs" system: composable rule sets (light-spec, medium-spec, heavy-spec)
- Generates `.cursor/rules/`, `CLAUDE.md`, `GEMINI.md`, `.github/copilot-instructions.md`
- Source: https://github.com/botingw/rulebook-ai

### awesome-cursorrules (major collection)
- 100+ community-contributed rules for every framework
- Most still use legacy `.cursorrules` format (single file)
- Source: https://github.com/PatrickJS/awesome-cursorrules

**No major open-source project (Next.js, Tailwind, etc.) ships `.cursorrules` or `.cursor/rules/` files in their repo.** These are developer-side configuration, not library-side.

---

## 8. Cursor MCP Integration

Cursor supports MCP servers for tool augmentation. From the docs index, relevant pages exist at:
- `/docs/mcp` -- MCP configuration
- `/docs/agent/tools/browser` -- Browser tool

MCP in Cursor allows connecting external tool servers that provide additional capabilities to the Agent. This is separate from rules -- rules are prompt-level instructions, MCP provides tool-level capabilities.

Cursor does NOT have a built-in "rules via MCP" integration. Rules remain file-based.

---

## 9. AGENTS.md Standard

**Repository:** https://github.com/agentsmd/agents.md (open format)
**Website:** https://agents.md

### What it is
- A simple, open **markdown format** for guiding coding agents
- "README for agents" -- a dedicated, predictable place for agent instructions
- Plain markdown, no frontmatter or metadata
- Supported by: **GitHub Copilot, Cursor, Claude Code** (reads them automatically)

### Format
Just markdown with sections. Minimal example:
```markdown
# Sample AGENTS.md file

## Dev environment tips
- Use `pnpm dlx turbo run where <project_name>` to jump to a package

## Testing instructions
- Run `pnpm turbo run test --filter <project_name>`
- Fix any test or type errors until the whole suite is green

## PR instructions
- Title format: [<project_name>] <Title>
- Always run `pnpm lint` and `pnpm test` before committing.
```

### Placement
- Project root for project-wide instructions
- Subdirectories for scoped instructions
- Multiple `AGENTS.md` files supported (nearest takes precedence)

### GitHub Copilot Integration
GitHub Copilot coding agent explicitly supports `AGENTS.md`:
> "You can create one or more AGENTS.md files, stored anywhere within the repository. When Copilot is working, the nearest AGENTS.md file in the directory tree will take precedence."
> "Alternatively, you can use a single CLAUDE.md or GEMINI.md file stored in the root of the repository."

Source: [GitHub Docs - Adding repository custom instructions](https://docs.github.com/en/copilot/customizing-copilot/adding-repository-custom-instructions)

---

## 10. Claude Code CLAUDE.md Best Practices

**Source:** [Claude Code Docs - Memory](https://docs.anthropic.com/en/docs/claude-code/memory)

### Hierarchy (most specific wins)

| Location | Scope |
|----------|-------|
| `/Library/Application Support/ClaudeCode/CLAUDE.md` | Organization-wide (managed by IT) |
| `~/.claude/CLAUDE.md` | Personal (global) |
| `CLAUDE.md` or `.claude/CLAUDE.md` (project root) | Project (version-controlled) |
| `subdirectory/CLAUDE.md` | Subdirectory (on-demand loading) |
| `.claude/rules/*.md` | Scoped rules (like Cursor's `.cursor/rules/`) |

### Key Best Practices (from official docs)

1. **Target under 200 lines per CLAUDE.md file** -- longer files consume more context and reduce adherence
2. **Use markdown headers and bullets** to group related instructions -- Claude scans structure like readers do
3. **Write concrete, verifiable instructions** -- specific enough to check compliance
4. **Avoid contradictions** -- if two rules conflict, Claude may pick arbitrarily
5. **Review periodically** to remove outdated or conflicting instructions

### File References

CLAUDE.md files support imports:
```markdown
- git workflow @docs/git-instructions.md
```
Imported files are expanded and loaded into context at launch.

### Two Memory Systems

1. **CLAUDE.md** -- You write these, persistent instructions
2. **Auto Memory** -- Claude accumulates learnings across sessions automatically
   - Stored at `~/.claude/projects/<project>/memory/`
   - `MEMORY.md` acts as an index, loaded every session
   - Machine-local, plain markdown, editable/deletable

### Rules System

`.claude/rules/*.md` provides Cursor-like scoped rules:
- Can be scoped to specific file types or subdirectories
- More structured approach than monolithic CLAUDE.md

### Commands

- `claude init` -- Generate a starting CLAUDE.md automatically (analyzes codebase)
- `claude setup` -- Interactive setup wizard (CLAUDE.md, skills, hooks)
- `claude memory` -- Browse and open memory files

---

## 11. GitHub Copilot Custom Instructions

**Source:** [GitHub Docs](https://docs.github.com/en/copilot/customizing-copilot/adding-repository-custom-instructions)

### Three Types

1. **Repository-wide**: `.github/copilot-instructions.md`
   - Applies to all requests in the repo context
   - Natural language in Markdown
   - Max ~2 pages recommended

2. **Path-specific**: `.github/instructions/NAME.instructions.md`
   - Frontmatter with `applyTo` glob:
     ```yaml
     ---
     applyTo: "**/*.rs"
     ---
     ```
   - Can use `excludeAgent: "code-review"` or `"coding-agent"`
   - Multiple patterns: `applyTo: "**/*.ts,**/*.tsx"`

3. **Agent instructions**: `AGENTS.md` (or `CLAUDE.md` / `GEMINI.md`)
   - Nearest file in directory tree takes precedence

### Precedence
Personal instructions > Repository instructions > Organization instructions

### Key Feature
Copilot coding agent can **auto-generate** `copilot-instructions.md` by analyzing your codebase. GitHub provides a detailed mega-prompt for this at `github.com/copilot/agents`.

---

## Cross-Tool Comparison Matrix

| Feature | Cursor | Claude Code | GitHub Copilot |
|---------|--------|-------------|----------------|
| **Rule location** | `.cursor/rules/*.mdc` | `CLAUDE.md` + `.claude/rules/*.md` | `.github/copilot-instructions.md` |
| **Format** | MDC (Markdown + frontmatter) | Plain Markdown | Plain Markdown + frontmatter |
| **Glob scoping** | Yes (in frontmatter) | Yes (in rules) | Yes (`applyTo` in frontmatter) |
| **Always-on** | `alwaysApply: true` | Root CLAUDE.md | Repository-wide file |
| **Auto-detect** | "Apply Intelligently" | Subdirectory on-demand | Path-specific instructions |
| **File refs** | `@filename.ts` | `@docs/file.md` (import) | N/A |
| **Cross-tool** | AGENTS.md supported | AGENTS.md supported | AGENTS.md, CLAUDE.md, GEMINI.md |
| **Auto-memory** | No | Yes (auto memory) | Copilot Memory (new) |
| **Team/Org rules** | Dashboard (Team plan) | Organization CLAUDE.md | Organization instructions |
| **Deprecated** | `.cursorrules` | N/A | N/A |
| **Max recommended** | 500 lines per rule | 200 lines per CLAUDE.md | ~2 pages |

---

## Optimal Length Guidelines

| Tool | Format | Recommended Max |
|------|--------|----------------|
| Cursor `.mdc` rule | Single rule file | **500 lines** (official) |
| Claude Code `CLAUDE.md` | Per file | **200 lines** (official) |
| GitHub Copilot instructions | Per file | **~2 pages** (official) |
| AGENTS.md | Per file | No official limit; keep focused |

**Consensus:** Keep individual rule files focused and under 500 lines. Split into multiple files rather than creating monoliths.

---

## Sources

1. [Cursor Docs - Rules](https://docs.cursor.com/docs/context/rules) (via web archive) -- Official rule types, MDC format, best practices, FAQ
2. [awesome-cursorrules](https://github.com/PatrickJS/awesome-cursorrules) -- Community rule collection, 100+ examples
3. [rules_template](https://github.com/Bhartendu-Kumar/rules_template) (1,063 stars) -- Cross-tool rule template with 6 `.mdc` files
4. [rulebook-ai](https://github.com/botingw/rulebook-ai) (580 stars) -- Universal AI environment manager, cross-tool sync
5. [AGENTS.md spec](https://github.com/agentsmd/agents.md) -- Open format for guiding coding agents
6. [Claude Code Docs - Memory](https://docs.anthropic.com/en/docs/claude-code/memory) -- CLAUDE.md hierarchy, auto memory, rules system
7. [GitHub Docs - Custom Instructions](https://docs.github.com/en/copilot/customizing-copilot/adding-repository-custom-instructions) -- Path-specific instructions, AGENTS.md support

---

## Confidence Level

**High** -- All findings come from official documentation (Cursor docs via web archive, Anthropic docs, GitHub docs) and verified open-source repositories. The Cursor docs were extracted from cached RSC payloads but match the official canonical URL structure.

---

## Recommendations for Nika

Based on this research, here is what would make sense for Nika's AI coding assistant integration:

### Already done well
- `.cursor/rules/nika-workflows.mdc` (189 lines, glob-scoped) is excellent
- `CLAUDE.md` hierarchy (monorepo root + nika root + tools/nika) follows best practices
- `.claude/rules/` used for scoped architecture/git rules

### Potential additions
1. **AGENTS.md** in project root -- cross-tool standard, read by Cursor AND Copilot AND Claude Code
2. **More .mdc rules** for Cursor users:
   - `nika-engine.mdc` (Apply Intelligently, description: "Nika engine internals, error codes, AST phases")
   - `nika-tui.mdc` (glob: `**/nika-tui/**/*.rs`)
   - `nika-testing.mdc` (Apply Intelligently, description: "Testing conventions, cargo test --lib, insta snapshots")
3. **`.github/copilot-instructions.md`** for GitHub Copilot coding agent users
4. **`.github/instructions/rust.instructions.md`** with `applyTo: "**/*.rs"` for Rust conventions

### What NOT to do
- Don't put everything in one giant file (neither `.cursorrules` monolith nor 500+ line CLAUDE.md)
- Don't duplicate between CLAUDE.md and `.cursor/rules/` -- use AGENTS.md as the shared base
- Don't rely on rules for security enforcement -- they are AI guidance, not hard controls
