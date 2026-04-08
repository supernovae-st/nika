# MEGA EXECUTION PROMPT — AI Rules Architecture + Editors + Distribution

**Copy-paste this ENTIRE prompt into a fresh Claude Code session.**
**Expected runtime: 10-15 hours. Full autonomy. TDD. Code review at each phase.**

---

```markdown
# Mission: Implement Progressive Discovery AI Rules + Editor Polish + Distribution Fixes

You are executing a detailed implementation plan for Nika — a Rust YAML workflow engine
(379K LOC, 17 crates, 10,435+ tests, v0.77.0). You will work for 10-15 hours in full
autonomy, following TDD, with verification at every phase.

## CRITICAL RULES

1. **TDD**: Write tests FIRST, watch them fail, then implement. No exceptions.
2. **1 fix = 1 commit**: Each logical change gets its own commit.
3. **Commit format**: `type(scope): description` with co-author `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
4. **NEVER use Claude/Anthropic co-author** — ONLY Nika 🦋
5. **cargo test --workspace --lib** after EVERY phase (must stay green)
6. **cargo clippy --workspace -- -D warnings** after EVERY phase (must be clean)
7. **Zero dead code** — delete aggressively, never leave stale code
8. **Zero backward compat** — v0, break what needs breaking
9. **AGPL-3.0-or-later** on all new files
10. **Run from `tools/` directory** for all cargo commands (workspace root)
11. **--lib flag always** on cargo test (avoid keychain popups)
12. **Never push or tag** without explicit user permission

## PHASE 0: BASELINE VERIFICATION (30 min)

Before touching ANY code, verify the baseline is green.

### 0.1 Compile check
```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo check --workspace
cargo clippy --workspace -- -D warnings
```
If clippy fails: fix the lint FIRST, commit, then continue.

### 0.2 Test baseline
```bash
cargo test --workspace --lib 2>&1 | grep "test result"
```
Record the EXACT test count. Every phase must maintain or increase this number.
If tests fail: these are KNOWN failures from the model resilience WIP (gpt-5.2 refactor).
Record which tests fail. Do NOT fix them — they are a separate workstream.
Your changes must NOT add new failures.

### 0.3 Read key files
Read these files to understand current state:
- `docs/plans/2026-04-07-ai-rules-architecture.md` — THE PLAN you're implementing
- `docs/plans/2026-04-07-launch-plan-may5.md` — overall launch context
- `docs/plans/2026-04-07-zed-deep-integration-plan.md` — Zed specifics
- `dx/.claude/rules/architecture.md` — architecture rules to follow
- `tools/nika-cli/src/init.rs` — current nika init implementation
- `tools/nika-cli/src/install.rs` — current nika setup + fast_rule_update
- `tools/nika-cli/rules/` — current monolithic rule files
- `editors/` — current editor extensions
- `editors/shared/nika-keywords.json` — keyword database
- `editors/sync-editors.sh` — sync script

### 0.4 Git status
```bash
git status --short | wc -l
git log --oneline -5
```
Record current state. If there are uncommitted changes, DO NOT touch those files
unless they are part of your plan. Work alongside the existing WIP.

### PHASE 0 VERIFICATION
- [ ] cargo check passes
- [ ] clippy clean (or known-issue documented)
- [ ] test count recorded: ___
- [ ] all plan files read
- [ ] git state clean or WIP documented

---

## PHASE 1: SHARED CONTENT MODULES (2 hours)

Extract the monolithic `claude.md` (563 lines) into focused modules.

### 1.1 Create directory structure
```
tools/nika-cli/rules/shared/
```

### 1.2 Create identity.md (~15 lines)
Extract from claude.md the absolute essentials:
- Schema: nika/workflow@0.12
- Extension: .nika.yaml
- 5 verbs (1 line each)
- Commands: nika check, nika run
- MCP reference: "call nika_schema for full reference"

**Test**: The file must be under 20 lines. Count them.

### 1.3 Create verbs.md (~80 lines)
Extract the 5 verbs section from claude.md:
- Each verb: name, purpose, short form, ONE complete example
- No walls of text — code examples only

**Test**: Must contain exactly 5 verb sections. Each must have a ```yaml code block.

### 1.4 Create data-flow.md (~60 lines)
Extract data flow patterns:
- `with:` bindings + `{{with.alias}}`
- `depends_on:` ordering
- `$task_id` references + path access
- `{{with.data | transform}}` pipe syntax
- `for_each:` parallel loops
- `inputs:` and `context:`

**Test**: Must contain examples of with, depends_on, $ref, pipe transforms, for_each.

### 1.5 Create structured-output.md (~40 lines)
Extract structured output section:
- 5-layer defense explanation (1 line each)
- One complete example with schema
- Rule: prompt must be NATURAL, never mention JSON

**Test**: Must contain a ```yaml example with `structured:` block.

### 1.6 Create common-mistakes.md (~50 lines)
Extract the "Common Mistakes" table:
- Top 15 most impactful Wrong → Right pairs
- Focused on what AI assistants actually get wrong

**Test**: Must contain a markdown table with at least 15 rows.

### 1.7 Create providers.md (~30 lines)
Extract provider info:
- 16 providers listed with env var names
- Slash syntax: `model: groq/llama-3.3-70b`
- Auto-infer: `model: claude-sonnet-4-20250514` (no provider needed)

**Test**: Must list at least 9 providers (the cloud ones).

### 1.8 Create advanced.md (~60 lines)
Extract advanced features:
- `agent:` verb full form
- `on_error:` (ignore, retry_with_provider, fallback)
- `schedule:` field
- `when:` conditional
- Artifacts
- Vision/multimodal content

**Test**: Must contain examples of agent, on_error, and when.

### PHASE 1 VERIFICATION
```bash
# Count lines per module
for f in tools/nika-cli/rules/shared/*.md; do echo "$(wc -l < "$f") $f"; done

# Verify total content coverage
# The shared modules should cover the same topics as the original claude.md
# but in focused, smaller files

# Verify no module exceeds 100 lines
for f in tools/nika-cli/rules/shared/*.md; do
  lines=$(wc -l < "$f")
  if [ "$lines" -gt 100 ]; then echo "FAIL: $f has $lines lines (max 100)"; fi
done

# Tests still pass
cd tools && cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
```

**Commit**: `feat(rules): extract shared content modules from monolithic claude.md`

---

## PHASE 2: PER-TOOL ASSEMBLERS (2 hours)

Create assembler functions in init.rs that compose modules into tool-specific formats.

### 2.1 Write tests FIRST (TDD)

In `tools/nika-cli/src/init.rs` (or a new `tools/nika-cli/src/rules.rs` module):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembled_claude_contains_identity() {
        let claude = assemble_claude_rules();
        assert!(claude.contains("nika/workflow@0.12"));
        assert!(claude.contains("infer:"));
        assert!(claude.contains("exec:"));
        assert!(claude.contains("fetch:"));
        assert!(claude.contains("invoke:"));
        assert!(claude.contains("agent:"));
    }

    #[test]
    fn assembled_claude_under_600_lines() {
        let claude = assemble_claude_rules();
        let lines = claude.lines().count();
        assert!(lines < 600, "Claude rules too long: {lines} lines");
    }

    #[test]
    fn assembled_cursor_project_under_25_lines() {
        let mdc = assemble_cursor_project_mdc();
        let lines = mdc.lines().count();
        assert!(lines < 25, "Cursor project mdc too long: {lines} lines");
        assert!(mdc.contains("alwaysApply: true"));
    }

    #[test]
    fn assembled_cursor_syntax_has_globs() {
        let mdc = assemble_cursor_syntax_mdc();
        assert!(mdc.contains("globs:"));
        assert!(mdc.contains(".nika.yaml"));
        assert!(mdc.contains("infer:"));
    }

    #[test]
    fn assembled_cursor_reference_is_agent_requested() {
        let mdc = assemble_cursor_reference_mdc();
        // Agent Requested = no alwaysApply, no globs (or globs but not alwaysApply)
        assert!(!mdc.contains("alwaysApply: true"));
    }

    #[test]
    fn assembled_copilot_has_frontmatter() {
        let copilot = assemble_copilot_instructions();
        assert!(copilot.contains("applyTo:") || copilot.contains("nika"));
    }

    #[test]
    fn assembled_agents_md_is_cross_tool() {
        let agents = assemble_agents_md();
        assert!(agents.contains("nika/workflow@0.12"));
        assert!(agents.contains("nika check"));
        assert!(agents.contains("nika run"));
        // Should NOT contain tool-specific frontmatter
        assert!(!agents.contains("alwaysApply"));
        assert!(!agents.contains("globs:"));
    }

    #[test]
    fn all_assemblies_contain_five_verbs() {
        let outputs = vec![
            assemble_claude_rules(),
            assemble_cursor_syntax_mdc(),
            assemble_copilot_instructions(),
            assemble_agents_md(),
        ];
        for (i, output) in outputs.iter().enumerate() {
            for verb in ["infer:", "exec:", "fetch:", "invoke:", "agent:"] {
                assert!(output.contains(verb),
                    "Assembly {i} missing verb {verb}");
            }
        }
    }

    #[test]
    fn no_assembly_exceeds_token_budget() {
        // Rough estimate: 4 chars per token, 8000 token budget per rule file
        let max_chars = 32000;
        let assemblies = vec![
            ("claude", assemble_claude_rules()),
            ("cursor_project", assemble_cursor_project_mdc()),
            ("cursor_syntax", assemble_cursor_syntax_mdc()),
            ("cursor_reference", assemble_cursor_reference_mdc()),
            ("copilot", assemble_copilot_instructions()),
            ("agents", assemble_agents_md()),
        ];
        for (name, content) in assemblies {
            assert!(content.len() < max_chars,
                "{name} exceeds token budget: {} chars (max {max_chars})", content.len());
        }
    }
}
```

### 2.2 Run tests — watch them FAIL
```bash
cargo test -p nika-cli --lib -- rules::tests 2>&1
```
All tests should fail (functions don't exist yet).

### 2.3 Implement assembler functions

Create `tools/nika-cli/src/rules.rs`:
- `include_str!()` each shared module at compile time
- `assemble_claude_rules()` → identity + verbs + data_flow + structured + mistakes + advanced
- `assemble_cursor_project_mdc()` → MDC frontmatter (alwaysApply) + identity only
- `assemble_cursor_syntax_mdc()` → MDC frontmatter (globs) + verbs + data_flow
- `assemble_cursor_reference_mdc()` → MDC frontmatter (Agent Requested) + mistakes + providers
- `assemble_copilot_instructions()` → copilot frontmatter + identity + verbs + data_flow
- `assemble_agents_md()` → identity + verbs + data_flow + mistakes (no tool-specific frontmatter)
- `assemble_windsurf_rules()` → identity + verbs (under 6000 chars — Windsurf limit)
- `assemble_gemini_md()` → identity + verbs + data_flow (new)
- `assemble_roo_rules()` → identity + verbs + data_flow + mistakes

### 2.4 Run tests — watch them PASS
```bash
cargo test -p nika-cli --lib -- rules::tests 2>&1
```

### PHASE 2 VERIFICATION
```bash
cargo test -p nika-cli --lib -- rules 2>&1
cargo clippy -p nika-cli -- -D warnings
cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
```
Test count must be >= baseline + new tests.

**Commit**: `feat(rules): add per-tool assemblers composing shared modules`

---

## PHASE 3: NEW AI TOOL RULES (1 hour)

Add the 4 missing AI assistant rule files.

### 3.1 Create tools/nika-cli/rules/gemini/ directory
- `GEMINI.md` — assembled from shared modules
- Format: standard Markdown (Gemini CLI reads .gemini/GEMINI.md)

### 3.2 Create tools/nika-cli/rules/amazonq/ directory
- `nika.rule.md` — Amazon Q format with Purpose/Instructions/Priority sections
- Content from shared modules, reformatted

### 3.3 Create tools/nika-cli/rules/jetbrains/ directory
- `nika.md` — JetBrains AI format (standard Markdown, .aiassistant/rules/)

### 3.4 Create tools/nika-cli/rules/cline/ directory
- `clinerules` — Cline format (no extension, plain text)

### 3.5 Write coherence tests

```rust
#[test]
fn all_ai_tools_have_assemblers() {
    // Every AI tool must have an assembler function
    let _ = assemble_claude_rules();
    let _ = assemble_cursor_project_mdc();
    let _ = assemble_cursor_syntax_mdc();
    let _ = assemble_cursor_reference_mdc();
    let _ = assemble_copilot_instructions();
    let _ = assemble_windsurf_rules();
    let _ = assemble_roo_rules();
    let _ = assemble_gemini_md();
    let _ = assemble_amazonq_rules();
    let _ = assemble_jetbrains_rules();
    let _ = assemble_cline_rules();
    let _ = assemble_agents_md();
}

#[test]
fn all_assemblers_mention_schema_version() {
    let assemblers: Vec<(&str, String)> = vec![
        ("claude", assemble_claude_rules()),
        ("cursor_syntax", assemble_cursor_syntax_mdc()),
        ("copilot", assemble_copilot_instructions()),
        ("windsurf", assemble_windsurf_rules()),
        ("roo", assemble_roo_rules()),
        ("gemini", assemble_gemini_md()),
        ("amazonq", assemble_amazonq_rules()),
        ("jetbrains", assemble_jetbrains_rules()),
        ("cline", assemble_cline_rules()),
        ("agents", assemble_agents_md()),
    ];
    for (name, content) in assemblers {
        assert!(content.contains("nika/workflow@0.12"),
            "{name} assembly missing schema version");
    }
}

#[test]
fn windsurf_under_6000_chars() {
    let ws = assemble_windsurf_rules();
    assert!(ws.len() < 6000,
        "Windsurf rules exceed 6000 char limit: {} chars", ws.len());
}
```

### PHASE 3 VERIFICATION
```bash
cargo test -p nika-cli --lib -- rules 2>&1
cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
```

**Commit**: `feat(rules): add Gemini, Amazon Q, JetBrains AI, Cline rule assemblers`

---

## PHASE 4: UPDATE NIKA INIT (2 hours)

Wire the new assemblers into `nika init` with smart editor detection.

### 4.1 Read current init.rs thoroughly
```bash
wc -l tools/nika-cli/src/init.rs
```
Understand the current flow before modifying.

### 4.2 Write tests for smart detection

```rust
#[test]
fn init_generates_agents_md() {
    let dir = tempdir().unwrap();
    init_project_at(dir.path(), "test-project", &InitOptions::default()).unwrap();
    assert!(dir.path().join("AGENTS.md").exists());
    let content = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(content.contains("nika/workflow@0.12"));
}

#[test]
fn init_generates_mcp_json_with_nika_server() {
    let dir = tempdir().unwrap();
    init_project_at(dir.path(), "test-project", &InitOptions::default()).unwrap();
    let mcp = std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&mcp).unwrap();
    assert!(parsed["mcpServers"]["nika"].is_object(),
        ".mcp.json should have nika server pre-configured");
    assert_eq!(parsed["mcpServers"]["nika"]["command"], "nika");
}

#[test]
fn init_generates_cursor_multi_file() {
    let dir = tempdir().unwrap();
    init_project_at(dir.path(), "test-project", &InitOptions::default()).unwrap();
    // Should generate 3 cursor files instead of 1
    let rules_dir = dir.path().join(".cursor/rules");
    assert!(rules_dir.join("nika-project.mdc").exists());
    assert!(rules_dir.join("nika-syntax.mdc").exists());
    assert!(rules_dir.join("nika-reference.mdc").exists());
    
    // Project file must be small and alwaysApply
    let project = std::fs::read_to_string(rules_dir.join("nika-project.mdc")).unwrap();
    assert!(project.contains("alwaysApply: true"));
    assert!(project.lines().count() < 25);
}

#[test]
fn init_generates_claude_settings_with_hooks() {
    let dir = tempdir().unwrap();
    init_project_at(dir.path(), "test-project", &InitOptions::default()).unwrap();
    let settings_path = dir.path().join(".claude/settings.json");
    if settings_path.exists() {
        let settings: serde_json::Value = 
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        // Should have MCP permissions
        assert!(settings["permissions"]["allow"].as_array()
            .map(|a| a.iter().any(|v| v.as_str().unwrap_or("").contains("nika")))
            .unwrap_or(false));
    }
}

#[test]
fn init_generates_cursor_mcp_json() {
    let dir = tempdir().unwrap();
    init_project_at(dir.path(), "test-project", &InitOptions::default()).unwrap();
    let cursor_mcp = dir.path().join(".cursor/mcp.json");
    if cursor_mcp.exists() {
        let parsed: serde_json::Value = 
            serde_json::from_str(&std::fs::read_to_string(&cursor_mcp).unwrap()).unwrap();
        assert!(parsed["mcpServers"]["nika"].is_object());
    }
}
```

### 4.3 Implement the changes

Modify `init.rs`:
1. Replace monolithic rule embedding with calls to assembler functions
2. Pre-populate `.mcp.json` with nika MCP server entry
3. Generate 3 Cursor files instead of 1
4. Generate `.cursor/mcp.json` alongside `.cursor/rules/`
5. Generate `.claude/settings.json` with permissions + hooks
6. Add `.gemini/GEMINI.md` generation
7. Keep backward compatibility: if `.cursorrules` exists, don't overwrite

### 4.4 Update nika setup (install.rs)

Modify `install.rs`:
1. Deploy multi-file cursor rules to `~/.cursor/rules/`
2. Update xxhash tracking for per-file hashes (not per-tool)
3. Add Gemini deployment to `~/.gemini/`

### PHASE 4 VERIFICATION
```bash
cargo test -p nika-cli --lib -- init 2>&1
cargo test -p nika-cli --lib -- install 2>&1
cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
cargo clippy --workspace -- -D warnings
```

**Commit**: `feat(init): progressive discovery rules + MCP pre-config + smart detection`

---

## PHASE 5: DOCTOR AI ECOSYSTEM CHECKS (1.5 hours)

Add AI ecosystem verification to `nika doctor`.

### 5.1 Read current doctor.rs
```bash
wc -l tools/nika-cli/src/doctor.rs
```

### 5.2 Write tests FIRST

```rust
#[test]
fn doctor_detects_stale_rules() {
    // Create a temp project with old-version rules
    // Run doctor check
    // Should report "AI rules are outdated"
}

#[test]
fn doctor_detects_missing_mcp_config() {
    // Create a temp project with nika.toml but no .mcp.json
    // Run doctor check
    // Should report ".mcp.json missing nika MCP server"
}

#[test]
fn doctor_fix_creates_mcp_json() {
    // Create temp project without .mcp.json
    // Run doctor --fix
    // .mcp.json should now exist with nika server
}
```

### 5.3 Implement new doctor checks

Add `check_ai_ecosystem()` to doctor.rs:
1. **Rules freshness**: compare xxhash of deployed rules vs current version
2. **MCP config**: check .mcp.json has nika server entry
3. **AGENTS.md**: exists and contains current schema version
4. **Editor extensions**: detect installed editors, check for nika extension
5. **LSP binary**: verify `nika lsp --stdio` or `nika-lsp` available

Each check should have:
- Clear status (OK / WARN / FAIL)
- Actionable fix suggestion
- `--fix` auto-repair where possible

### PHASE 5 VERIFICATION
```bash
cargo test -p nika-cli --lib -- doctor 2>&1
cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
```

**Commit**: `feat(doctor): add AI ecosystem health checks (rules, MCP, editors)`

---

## PHASE 6: DAEMON RULE FRESHNESS (1 hour)

Add proactive rule update to daemon startup.

### 6.1 Read current daemon startup
```bash
grep -n "startup\|on_start\|init" tools/nika-daemon/src/lib.rs | head -20
```

### 6.2 Write tests

```rust
#[test]
fn daemon_detects_version_mismatch() {
    // Simulate stored rule version != binary version
    // Should trigger fast_rule_update
}
```

### 6.3 Implement

In daemon startup flow:
1. Read stored rule version from `machine.toml`
2. Compare with current binary version
3. If different: call `fast_rule_update()`
4. Log the update

### PHASE 6 VERIFICATION
```bash
cargo test -p nika-daemon --lib 2>&1
cargo test --workspace --lib 2>&1 | grep "test result" | tail -3
```

**Commit**: `feat(daemon): proactive AI rule freshness check on startup`

---

## PHASE 7: EDITOR SYNC IMPROVEMENTS (1 hour)

### 7.1 Update sync-editors.sh for new shared modules

The sync script currently parses the monolithic files. Update it to:
1. Read from `nika-keywords.json` (already generated)
2. Verify all 4 editor highlights have the same keywords
3. Verify Helix has full keyword coverage (currently partial)

### 7.2 Regenerate nika-keywords.json

```bash
python3 editors/shared/extract-keywords.py > editors/shared/nika-keywords.json
```

Verify the output is valid and complete.

### 7.3 Run sync check

```bash
./editors/sync-editors.sh --verbose
```

Fix any drift found.

### PHASE 7 VERIFICATION
```bash
./editors/sync-editors.sh
echo "Exit code: $?"
# Must be 0 (no drift)
```

**Commit**: `chore(editors): sync keywords and fix drift`

---

## PHASE 8: FULL INTEGRATION TEST (1 hour)

### 8.1 Full test suite
```bash
cd tools
cargo test --workspace --lib 2>&1 | grep "test result"
```
Count must be >= baseline + all new tests.

### 8.2 Clippy
```bash
cargo clippy --workspace -- -D warnings
```
Must be clean.

### 8.3 Format
```bash
cargo fmt --all --check
```
Must be clean. If not: `cargo fmt --all` then commit.

### 8.4 Editor sync
```bash
./editors/sync-editors.sh
```
Must be clean.

### 8.5 VS Code extension
```bash
cd editors/vscode
npm run compile
npm test
```
Must pass.

### 8.6 Smoke test: nika init in temp directory
```bash
TMPDIR=$(mktemp -d)
cd "$TMPDIR"
nika init test-smoke --no-interactive 2>&1 || true
ls -la
ls -la .claude/ 2>/dev/null
ls -la .cursor/rules/ 2>/dev/null
cat .mcp.json
cat AGENTS.md | head -5
cd -
rm -rf "$TMPDIR"
```

### 8.7 Architecture coherence check
```bash
# Verify the diamond pattern — no upward dependencies
# nika-core should NOT depend on nika-engine
grep "nika-engine" tools/nika-core/Cargo.toml && echo "FAIL: core depends on engine!" || echo "OK"
# nika-lsp-core should NOT depend on nika-engine
grep "nika-engine" tools/nika-lsp-core/Cargo.toml && echo "FAIL: lsp-core depends on engine!" || echo "OK"
```

### PHASE 8 VERIFICATION
All of the above must pass. If ANY fails, fix it before proceeding.

**Commit**: `style: cargo fmt + final coherence verification`

---

## PHASE 9: CODE REVIEW (1 hour)

Launch 3 code review agents in parallel:

### 9.1 General code review
Review all files created/modified in phases 1-8 for:
- Bug potential
- Missing error handling
- Convention violations
- Test coverage gaps

### 9.2 Rust quality review
Review all .rs files for:
- Idiomatic Rust patterns
- Unwrap/expect in non-test code
- Memory efficiency
- Clippy compliance

### 9.3 Architecture review
Review the overall changes for:
- Diamond pattern violations
- Unnecessary dependencies
- Scalability concerns
- DX consistency

Fix ALL issues found by reviews.

**Commit**: `fix: address code review findings`

---

## PHASE 10: DOCUMENTATION UPDATE (1 hour)

### 10.1 Update CHANGELOG.md
Add entries for all changes made in this session.

### 10.2 Update editors/README.md
Reflect the new multi-file architecture and AI assistant coverage.

### 10.3 Update AGENTS.md at project root
Regenerate from the new assembler (should be automatic from init changes).

### 10.4 Update dx/.claude/rules/architecture.md
Ensure the AI Rules Architecture section reflects what was actually implemented
(not just what was planned).

### 10.5 Verify all docs are consistent
```bash
# Schema version mentions should all say @0.12
grep -r "workflow@0\." docs/ editors/ tools/nika-cli/rules/ | grep -v "0.12" | grep -v target | grep -v node_modules
```

**Commit**: `docs: update CHANGELOG, editors README, architecture rules`

---

## PHASE 11: FINAL VERIFICATION (30 min)

### 11.1 Complete test suite
```bash
cargo test --workspace --lib 2>&1 | grep "test result"
```

### 11.2 Test count regression check
Compare with Phase 0 baseline. Must be HIGHER (new tests added).

### 11.3 Clippy + format
```bash
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

### 11.4 Editor sync
```bash
./editors/sync-editors.sh
```

### 11.5 Git log review
```bash
git log --oneline HEAD~20..HEAD
```
Verify all commits follow `type(scope): description` format.
Verify all have `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`.

### 11.6 Summary report
Write a summary of:
- How many tests were added
- How many files were created/modified
- What the test count went from → to
- Any issues discovered and how they were resolved
- What's left for the next session

---

## KEY FILES REFERENCE

### Files you'll CREATE:
- `tools/nika-cli/rules/shared/identity.md`
- `tools/nika-cli/rules/shared/verbs.md`
- `tools/nika-cli/rules/shared/data-flow.md`
- `tools/nika-cli/rules/shared/structured-output.md`
- `tools/nika-cli/rules/shared/common-mistakes.md`
- `tools/nika-cli/rules/shared/providers.md`
- `tools/nika-cli/rules/shared/advanced.md`
- `tools/nika-cli/src/rules.rs` (assembler module)

### Files you'll MODIFY:
- `tools/nika-cli/src/init.rs` (wire new assemblers)
- `tools/nika-cli/src/install.rs` (multi-file deployment)
- `tools/nika-cli/src/lib.rs` (add rules module)
- `tools/nika-cli/src/doctor.rs` (AI ecosystem checks)
- `tools/nika-daemon/src/lib.rs` (rule freshness)
- `editors/README.md`
- `tools/nika/CHANGELOG.md`
- `dx/.claude/rules/architecture.md`

### Files you'll READ (context):
- `tools/nika-cli/rules/claude.md` (source content to extract from)
- `tools/nika-cli/rules/cursor.mdc` (reference for MDC format)
- `editors/shared/nika-keywords.json` (keyword reference)
- `docs/plans/2026-04-07-ai-rules-architecture.md` (the plan)

## REMEMBER

- TDD: test first, fail, implement, pass
- 1 fix = 1 commit
- Co-author: Nika 🦋 only, NEVER Claude/Anthropic
- cargo test --workspace --lib after EVERY phase
- clippy clean after EVERY phase
- If something breaks, FIX IT before moving on
- Zero dead code — delete the old monolithic rule files after migration
- Work from tools/ directory for all cargo commands
```
