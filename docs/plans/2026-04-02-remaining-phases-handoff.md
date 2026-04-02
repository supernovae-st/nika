# Handoff: Remaining Phases — .mcp.json + Docs Sweep

> Date: 2026-04-02 | Status: 9/11 phases DONE | Tests: 9331 passing
> Previous handoff: docs/plans/2026-04-01-nika-toml-handoff-prompt.md (1087 lines, reference only)

---

## Status: What's DONE (do NOT redo)

| Phase | Status | Key Commits |
|---|---|---|
| Phase 1 — nika.toml foundation | DONE | boot.rs walk-up, config.rs find_project_root_from(), init.rs creates nika.toml |
| Phase 2 — working_dir | DONE | exec.rs wired, 8 refs |
| Phase 3 — serve reads nika.toml | DONE | `0aad83ba4` serve config from nika.toml |
| Phase 4 — nika clean | DONE | clean.rs 370 lines |
| Phase 5 — doctor project checks | DONE | doctor.rs lines 442-574, legacy detection, workflow count |
| Phase 0 — smart welcome | DONE | main.rs:1039, 3 modes (no setup, no project, in project) |
| Phase 8 — init wizard | DONE | cliclack prompts, --yes flag |
| Phase 9 — artifacts dir | DONE | DEFAULT_ARTIFACT_DIR = "./artifacts" |
| Phase 10 — CLI UX polish | DONE | verb icons, TTFT, spinner, pretty JSON |

**9331 tests passing.** Zero legacy `.nika/config.toml` references in error messages.

---

## What's LEFT: 2 phases

### Phase 6: .mcp.json Convention (NOT DONE)

**Why:** Security audit found CRITICAL — MCP commands in versioned nika.toml = arbitrary code execution via `git clone`. Solution: follow Claude Code convention with `.mcp.json` at project root.

**Current state:** MCP config uses `~/.nika/mcp.yaml` (global) + `.nika/mcp.yaml` (project). Zero `.mcp.json` support anywhere in the codebase. The MCP config manager lives in `tools/nika-mcp/src/nika_config.rs` (942 lines).

**What to implement:**

1. **Read `.mcp.json`** at project root (Claude Code format):
```json
{
  "mcpServers": {
    "novanet": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "../novanet/Cargo.toml", "--", "mcp"],
      "env": {
        "NEO4J_URI": "bolt://localhost:7687"
      }
    }
  }
}
```

2. **Priority order:** `.mcp.json` (project) > `.nika/mcp.yaml` (project, legacy) > `~/.nika/mcp.yaml` (global user)

3. **Boot integration:** boot.rs Phase 5 (MCP Startup) reads `.mcp.json` from `project_root` (already in BootContext). Parse with `serde_json::from_str`.

4. **Init integration:** `nika init` can optionally create an empty `.mcp.json`:
```json
{
  "mcpServers": {}
}
```

5. **Doctor integration:** `nika doctor` already checks MCP config (line 709+). Add check for `.mcp.json` presence.

**Files to modify:**

| File | Lines | What |
|---|---|---|
| `tools/nika-mcp/src/nika_config.rs` | 942 | Add `.mcp.json` reader in `NikaMcpConfigManager` |
| `tools/nika-engine/src/runtime/boot.rs` | ~826 | Phase 5: read `.mcp.json` from project_root |
| `tools/nika-cli/src/init.rs` | ~370 | Optionally create `.mcp.json` |
| `tools/nika-cli/src/doctor.rs` | ~1315 | Check for `.mcp.json` |

**New struct** (in nika_config.rs or a new types module):
```rust
/// Claude Code convention .mcp.json format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpJsonServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpJsonServer {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}
```

**Conversion:** `McpJsonServer` → `NikaMcpServer` (existing type). Map fields 1:1 (they're nearly identical).

**TDD Tests (4):**

| # | Test | File | Asserts |
|---|---|---|---|
| 1 | `mcp_json_parsed_correctly` | nika_config.rs | .mcp.json with mcpServers map parsed |
| 2 | `mcp_json_preferred_over_legacy_yaml` | nika_config.rs | .mcp.json wins over .nika/mcp.yaml |
| 3 | `mcp_json_merged_with_global_yaml` | nika_config.rs | .mcp.json + ~/.nika/mcp.yaml merged |
| 4 | `mcp_json_fallback_to_yaml` | nika_config.rs | No .mcp.json → reads .nika/mcp.yaml |

**Implementation approach:**

In `NikaMcpConfigManager`, the current `load_merged()` method (lines 245-255) merges global + project YAML. Add a step before project YAML loading:

```rust
pub fn load_merged(&self) -> Result<Vec<(String, NikaMcpServer)>> {
    let mut servers = self.load_global()?;      // ~/.nika/mcp.yaml

    // NEW: Try .mcp.json first (Claude Code convention)
    if let Some(mcp_json) = self.load_mcp_json()? {
        for (name, server) in mcp_json.mcp_servers {
            servers.insert(name, server.into());  // .mcp.json wins
        }
    } else {
        // Fallback: project .nika/mcp.yaml (legacy)
        let project = self.load_project()?;
        for (name, server) in project {
            servers.insert(name, server);
        }
    }

    Ok(servers.into_iter().collect())
}

fn load_mcp_json(&self) -> Result<Option<McpJsonConfig>> {
    let mcp_json_path = self.project_root.join(".mcp.json");
    if !mcp_json_path.exists() { return Ok(None); }
    let content = std::fs::read_to_string(&mcp_json_path)?;
    let config: McpJsonConfig = serde_json::from_str(&content)?;
    Ok(Some(config))
}
```

**Commit:**
```
feat(mcp): read .mcp.json (Claude Code convention) for project MCP servers

Follow the emerging convention used by Claude Code and Cursor.
.mcp.json at project root > .nika/mcp.yaml (legacy) > ~/.nika/mcp.yaml (global).

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

### Phase 7: Final Documentation Sweep (PARTIAL)

**Already done:** CHANGELOG, README project structure section, doctor skill updated.

**Still needed:**

1. **AGENTS.md template in init.rs** — Verify the `include_str!("../rules/claude.md")` content mentions nika.toml (not .nika/config.toml). Read `tools/nika-cli/rules/claude.md` and check.

2. **Error messages audit** — Already verified: zero `.nika/config.toml` references in error.rs and error_domains.rs. DONE.

3. **Course content** — Already verified: zero `.nika/config.toml` references in missions.rs. DONE.

4. **grep sweep** — Run across entire codebase for remaining `.nika/config.toml` references that should say `nika.toml`:
```bash
cd tools && grep -rn '\.nika/config\.toml' --include='*.rs' | grep -v 'test\|legacy\|fallback\|bak\|migration'
```
Any hits = update them.

5. **nika/CLAUDE.md** — Verify the Project Structure section is accurate (already updated with .mcp.json).

**Commit:**
```
docs: final sweep — verify all references use nika.toml, update AGENTS.md template

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Methodology

Use these skills:
1. `/spn-powers:test-driven-development` — 4 tests for Phase 6 (RED first)
2. `/spn-powers:verification-before-completion` — `cd tools && cargo test --workspace --lib` after each phase

**Testing:** `cd tools && cargo test --workspace --lib` (workspace root is `tools/Cargo.toml`, NOT repo root). Current: 9331 tests passing.

**Commit style:**
```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Zero backward compat.** `.nika/mcp.yaml` stays as legacy fallback (read-only, never write). New projects get `.mcp.json`.

---

## Build Notes

If linker errors or temp dir issues: `cd tools && cargo clean && cargo test --workspace --lib`

## Success Criteria

- [ ] `.mcp.json` at project root is read by MCP config manager
- [ ] `.mcp.json` takes priority over `.nika/mcp.yaml` (project-level)
- [ ] Global `~/.nika/mcp.yaml` merged with `.mcp.json`
- [ ] Fallback to `.nika/mcp.yaml` when no `.mcp.json` exists
- [ ] `nika doctor` checks for `.mcp.json`
- [ ] Zero `.nika/config.toml` references in user-facing messages (except legacy/migration code)
- [ ] `cd tools && cargo test --workspace --lib` passes (9331+ tests)
- [ ] 2 commits total (Phase 6 + Phase 7)
