# GitNexus integration

> Graph-aware code intelligence for Nika — via MCP in Claude Code only.
> Never touches global Claude Code configs. Analyze runs with `--skip-agents-md`
> to preserve our authority-chain `.claude/CLAUDE.md`.

## What it is

[GitNexus](https://github.com/abhigyanpatwari/GitNexus) indexes a codebase
into a knowledge graph (files, functions, imports, calls, clusters) and
exposes it via MCP to AI agents.

Our use: **MCP-only**, activated during review swarms and contributor
onboarding. Not a runtime dependency. Not a source of truth.

## License

**PolyForm Noncommercial 1.0.0.** Fine for personal dev. For commercial
SaaS deployments of Nika, license required via [akonlabs.com](https://akonlabs.com).
For now, we run it locally only.

## Commands

| Command | Purpose |
|---|---|
| `bash scripts/gitnexus/install.sh` | One-shot install with backups + integrity check |
| `bash scripts/gitnexus/verify.sh` | Confirm setup intact (runs anytime, idempotent) |
| `bash scripts/gitnexus/rollback.sh <BACKUP_DIR>` | Full reverse (restores from backup) |
| `gitnexus analyze --skip-agents-md --no-stats` | Re-index repo (after big changes) |

## What `install.sh` does (safe path)

1. **Pre-flight** — Node 20+, valid settings.json, count hooks + MCP servers
2. **Backup** — `~/.nika-backups/gitnexus-<timestamp>/` (6 files + analyze log)
3. **Harden** — add `.gitnexus/`, `AGENTS.md`, etc. to `.gitignore`
4. **MCP register** — add `gitnexus` to `.mcp.json` (nika + linear stay intact)
5. **Analyze** — `--skip-agents-md` protects our CLAUDE.md, `--no-stats` avoids volatile counts
6. **Verify** — runs `verify.sh` to confirm nothing broke

## What we deliberately DON'T do

- ❌ Run `gitnexus setup` — mass-writes to `~/.claude.json` + `~/.claude/settings.json` (globals)
- ❌ Install global hooks — 7s latency per Grep/Glob/Bash call
- ❌ Let it touch `CLAUDE.md` — HTML markers would contradict our authority chain
- ❌ Commit `.gitnexus/` — PolyForm-NC license compat unclear + 53 MB not useful in git
- ❌ Install the 6 skills in `.claude/skills/gitnexus-*/` to the repo — gitignored

## Artifacts on disk (local only, gitignored)

```
.gitnexus/
├── lbug             LadybugDB graph store (~53 MB for 140 files / 10k LOC)
└── meta.json        {repoPath, lastCommit, indexedAt, stats}

.claude/skills/gitnexus-{cli,debugging,exploring,guide,impact-analysis,refactoring}/
                     Helper skills for Claude Code to use the graph (regeneratable)

~/.nika-backups/gitnexus-<YYYYMMDD-HHMMSS>/
                     Pre-install snapshots (keep for rollback)
```

## Typical workflow

```bash
# Install once
bash scripts/gitnexus/install.sh

# Verify anytime (especially after Claude Code updates)
bash scripts/gitnexus/verify.sh

# Re-index after a big batch of commits
gitnexus analyze --skip-agents-md --no-stats

# Use from Claude Code — MCP tools available automatically:
#   mcp__gitnexus__query        (Cypher-like queries on the graph)
#   mcp__gitnexus__context      (architectural summary)
#   mcp__gitnexus__impact       (blast radius analysis)
#   mcp__gitnexus__detect_changes
#   mcp__gitnexus__list_repos
```

## When to use vs not

| Context | Use GitNexus? | Why |
|---|---|---|
| Daily craft (writing a crate) | NO | ripgrep + `cargo-modules` sufficient |
| Gate 11 review swarm | YES | Impact analysis saves 15 min per review |
| Contributor onboarding | YES | `atlas.nika.sh/graph` (future) gives newcomers visual DAG |
| Shadow zone audit | YES | Graph queries "what imports X" = GitNexus sweet spot |
| Marketing content | NO | Too technical; keep to `atlas.nika.sh` niche page |

## Integration with our hygiene

- `verify.sh` is a superset of `scripts/hygiene/check-all.sh` (includes all 15 vectors)
- Re-indexing is idempotent — re-run after every 5-10 admissions
- GitNexus graph is advisory only; `Cargo.toml` remains authoritative for crate
  membership + deps

## Rollback

If GitNexus breaks anything:

```bash
# Find latest backup
ls -1dt ~/.nika-backups/gitnexus-* | head -1

# Full rollback
bash scripts/gitnexus/rollback.sh "$(ls -1dt ~/.nika-backups/gitnexus-* | head -1)"
```

Rollback removes `.gitnexus/`, the 6 helper skills, the `gitnexus` MCP entry,
and restores `settings.json`, `.mcp.json`, `.gitignore`, `CLAUDE.md`.

🦋
