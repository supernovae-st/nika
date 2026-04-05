# Doctor + Init UX Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the new-user journey so `nika doctor` gives honest, actionable, scoped diagnostics, `nika init` offers editor/AI setup, and the LSP check actually works.

**Architecture:** All changes in `tools/nika-cli/src/doctor.rs` (diagnostic logic), `tools/nika-cli/src/init_wizard.rs` (wizard flow), and `tools/nika-engine/src/display/summary.rs` (rendering). No new crates or dependencies.

**Tech Stack:** Rust, cliclack (wizard UI), colored (terminal colors), std::process::Command (LSP probe)

---

## Batch 1: Doctor — Sectioned Output + Scope Labels

### Task 1: Add section field to DiagnosticCheck + sectioned rendering

Currently all 15 checks render as a flat list. New users can't tell what's critical vs optional.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (struct + output_doctor_text)

**Step 1: Add section field to DiagnosticCheck**

In `doctor.rs`, add a `section` field to the struct and update constructors:

```rust
#[derive(Debug, Clone)]
struct DiagnosticCheck {
    section: &'static str,
    name: &'static str,
    status: DiagnosticStatus,
    message: String,
    suggestion: Option<String>,
}

impl DiagnosticCheck {
    fn pass(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            section: "",
            name,
            status: DiagnosticStatus::Pass,
            message: message.into(),
            suggestion: None,
        }
    }

    fn warn(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            section: "",
            name,
            status: DiagnosticStatus::Warn,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn fail(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            section: "",
            name,
            status: DiagnosticStatus::Fail,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn in_section(mut self, section: &'static str) -> Self {
        self.section = section;
        self
    }

    fn icon(&self) -> &'static str {
        match self.status {
            DiagnosticStatus::Pass => "✓",
            DiagnosticStatus::Warn => "⚠",
            DiagnosticStatus::Fail => "✗",
        }
    }
}
```

**Step 2: Assign sections to all checks in handle_doctor_command**

Replace the check registration in `handle_doctor_command` to assign sections. Use these 4 sections:

```rust
// Section: Core
checks.extend(check_nika_directory().into_iter().map(|c| c.in_section("Core")));
checks.push(check_config_file().in_section("Core"));
checks.extend(check_api_keys().into_iter().map(|c| c.in_section("Core")));
checks.push(DiagnosticCheck::pass("Version", format!("nika {}", env!("CARGO_PKG_VERSION"))).in_section("Core"));
checks.push(check_workflow_files().in_section("Core"));

// Section: Editor & LSP
checks.push(check_lsp_available().in_section("Editor & LSP"));
checks.extend(check_editor_integration().into_iter().map(|c| c.in_section("Editor & LSP")));

// Section: AI Integration
checks.extend(check_ai_rules().into_iter().map(|c| c.in_section("AI Integration")));
checks.push(check_agent_skills().in_section("AI Integration"));
checks.push(check_agents_md().in_section("AI Integration"));

// Section: Environment
checks.extend(check_trace_directory().into_iter().map(|c| c.in_section("Environment")));
checks.push(check_rust_version().in_section("Environment"));
checks.push(check_npx().in_section("Environment"));
checks.push(check_git_hook().in_section("Environment"));
if full {
    checks.push(check_mcp_connectivity().await.in_section("Environment"));
}
```

**Step 3: Update output_doctor_text to render sections**

```rust
fn output_doctor_text(checks: &[DiagnosticCheck], quiet: bool) {
    if !quiet {
        nika_engine::display::print_doctor_header(env!("CARGO_PKG_VERSION"));
    }

    let mut pass_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;
    let mut current_section = "";

    for check in checks {
        // Print section header when section changes
        if check.section != current_section && !check.section.is_empty() {
            current_section = check.section;
            println!();
            println!("  {}", current_section.bold().underline());
        }

        let icon = match check.status {
            DiagnosticStatus::Pass => check.icon().green(),
            DiagnosticStatus::Warn => check.icon().yellow(),
            DiagnosticStatus::Fail => check.icon().red(),
        };

        println!("  {} {} {}", icon, check.name.bold(), check.message);

        if let Some(ref suggestion) = check.suggestion {
            println!("    {} {}", "→".cyan(), suggestion.dimmed());
        }

        match check.status {
            DiagnosticStatus::Pass => pass_count += 1,
            DiagnosticStatus::Warn => warn_count += 1,
            DiagnosticStatus::Fail => fail_count += 1,
        }
    }

    // Next steps footer
    if warn_count > 0 || fail_count > 0 {
        println!();
        println!("  {}", "Next steps".bold());
        println!("  {} Run: {} to fix editor + AI integration", "→".cyan(), "nika setup".bold());
        println!("  {} Run: {} for detailed checks", "→".cyan(), "nika doctor --full".bold());
    }

    if !quiet {
        nika_engine::display::print_doctor_summary(pass_count, warn_count, fail_count);
    }
}
```

**Step 4: Update JSON output to include section**

In `output_doctor_json`, add section field:

```rust
serde_json::json!({
    "section": c.section,
    "name": c.name,
    // ... rest unchanged
})
```

**Step 5: Run tests**

```bash
cargo test -p nika-cli --lib -- doctor --nocapture
cargo check -p nika-cli
```

**Step 6: Commit**

```bash
git add tools/nika-cli/src/doctor.rs
git commit -m "fix(doctor): add sectioned output (Core, Editor & LSP, AI, Environment)

Groups checks into 4 sections with headers. Adds 'Next steps' footer
pointing users to nika setup. Includes section field in JSON output.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Add scope labels to AI Rules + Agent Skills checks

Users can't tell user-level from project-level. Fix check_ai_rules and check_agent_skills.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (check_ai_rules, check_agent_skills)

**Step 1: Update check_ai_rules with scope labels**

Replace check_ai_rules:

```rust
fn check_ai_rules() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Project-level rules (scope: project)
    let project_rules: &[(&str, &str)] = &[
        ("Cursor", ".cursor/rules/nika-workflows.mdc"),
        ("Copilot", ".github/copilot/nika.instructions.md"),
        ("Windsurf", ".windsurf/rules/nika.md"),
        ("Roo Code", ".roo/rules/nika.md"),
    ];

    for (tool, path) in project_rules {
        if std::path::Path::new(path).exists() {
            checks.push(DiagnosticCheck::pass(
                "AI Rules",
                format!("{tool} rules [project] ({path})"),
            ));
        }
    }

    // User-level rules (scope: user)
    let home = dirs::home_dir().unwrap_or_default();
    let claude_path = home.join(".claude/rules/nika.md");
    if claude_path.exists() {
        checks.push(DiagnosticCheck::pass(
            "AI Rules",
            format!("Claude Code rules [user] (~/.claude/rules/nika.md)"),
        ));
    } else if which::which("claude").is_ok() {
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            "Claude Code detected but no Nika rules [user]",
            "Run: nika setup ai → installs rules at ~/.claude/rules/nika.md",
        ));
    }

    if checks.is_empty() {
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            "No AI coding tool rules found",
            "Run: nika setup ai (installs user-level rules for detected tools)",
        ));
    }

    checks
}
```

**Step 2: Update check_agent_skills with scope labels**

Replace check_agent_skills:

```rust
fn check_agent_skills() -> DiagnosticCheck {
    let home = dirs::home_dir().unwrap_or_default();

    let user_skills = home.join(".agents/skills");
    let has_user = user_skills.join("nika-workflow-syntax").exists()
        || user_skills.join("nika-create").exists();

    let has_project = std::path::Path::new("skills/nika-workflow-syntax").exists()
        || std::path::Path::new(".agents/skills/nika-workflow-syntax").exists();

    match (has_user, has_project) {
        (true, true) => DiagnosticCheck::pass(
            "Agent Skills",
            format!(
                "Nika skills installed [user] (~/.agents/skills/) + [project]"
            ),
        ),
        (true, false) => DiagnosticCheck::pass(
            "Agent Skills",
            format!(
                "Nika skills installed [user] (~/.agents/skills/)"
            ),
        ),
        (false, true) => DiagnosticCheck::warn(
            "Agent Skills",
            "Nika skills found [project] only — AI agents (Claude Code) need [user] scope",
            "Run: nika setup ai → installs to ~/.agents/skills/ (visible to all AI tools)",
        ),
        (false, false) => DiagnosticCheck::warn(
            "Agent Skills",
            "No Nika Agent Skills installed",
            "Run: nika setup ai → installs to ~/.agents/skills/ (visible to all AI tools)",
        ),
    }
}
```

**Step 3: Run tests + commit**

```bash
cargo test -p nika-cli --lib -- doctor --nocapture
cargo check -p nika-cli
git add tools/nika-cli/src/doctor.rs
git commit -m "fix(doctor): add [user]/[project] scope labels to AI checks

AI Rules and Agent Skills now clearly show which scope they're installed at.
Warns when project-level skills exist but user-level are missing (invisible
to AI agents like Claude Code).

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 2: Doctor — Real LSP + PATH Check

### Task 3: Replace fake LSP check with real probe

The current check just tests `cfg!(feature = "lsp")`. Replace with actual `nika lsp --help` probe + PATH verification.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (check_lsp_available)

**Step 1: Replace check_lsp_available**

```rust
fn check_lsp_available() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // 1. Check if feature is compiled in
    if !cfg!(feature = "lsp") {
        checks.push(DiagnosticCheck::fail(
            "LSP",
            "Language server not compiled in",
            "Reinstall with: cargo install nika --features lsp, or: brew reinstall nika",
        ));
        return checks;
    }

    // 2. Check that nika binary is findable in PATH
    match which::which("nika") {
        Ok(path) => {
            checks.push(DiagnosticCheck::pass(
                "LSP",
                format!("nika binary in PATH ({})", path.display()),
            ));
        }
        Err(_) => {
            checks.push(DiagnosticCheck::fail(
                "LSP",
                "nika binary not found in PATH — editors cannot start LSP",
                "Add nika to PATH: export PATH=\"$HOME/.cargo/bin:$PATH\" (or reinstall via brew)",
            ));
            return checks;
        }
    }

    // 3. Probe: can `nika lsp` actually start? (quick version check)
    match std::process::Command::new("nika")
        .args(["lsp", "--help"])
        .output()
    {
        Ok(output) if output.status.success() => {
            checks.push(DiagnosticCheck::pass(
                "LSP",
                "Language server responds (nika lsp --help OK)",
            ));
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // --help may not exist, try just checking the feature is there
            if stderr.contains("lsp") || stderr.contains("LSP") {
                checks.push(DiagnosticCheck::pass(
                    "LSP",
                    "Language server compiled in (nika lsp)",
                ));
            } else {
                checks.push(DiagnosticCheck::warn(
                    "LSP",
                    format!("nika lsp returned error: {}", stderr.lines().next().unwrap_or("unknown")),
                    "Try: nika lsp --help to diagnose",
                ));
            }
        }
        Err(e) => {
            checks.push(DiagnosticCheck::fail(
                "LSP",
                format!("Cannot execute nika binary: {e}"),
                "Check file permissions and PATH",
            ));
        }
    }

    checks
}
```

**Step 2: Update handle_doctor_command** — change from `checks.push(check_lsp_available())` to `checks.extend(check_lsp_available().into_iter().map(|c| c.in_section("Editor & LSP")))`.

**Step 3: Run tests + commit**

```bash
cargo test -p nika-cli --lib -- doctor --nocapture
cargo check -p nika-cli
git add tools/nika-cli/src/doctor.rs
git commit -m "fix(doctor): real LSP check — PATH probe + nika lsp execution test

Replaces cfg!() compile-time check with runtime verification:
1. Checks nika binary is in PATH (needed by editors)
2. Probes nika lsp --help to verify LSP actually works
Fails loudly if PATH is missing instead of false-positive pass.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4: Implement real MCP connectivity check

Replace the stub with actual config parsing + connection test.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (check_mcp_connectivity)

**Step 1: Replace stub**

```rust
async fn check_mcp_connectivity() -> DiagnosticCheck {
    // Check if config has MCP servers defined
    let nika_dir = match crate::config::find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            return DiagnosticCheck::warn(
                "MCP",
                "Cannot check MCP — no .nika/ directory",
                "Run: nika init first",
            );
        }
    };

    let config_path = nika_dir.join("config.toml");
    if !config_path.exists() {
        return DiagnosticCheck::warn(
            "MCP",
            "No config.toml — cannot check MCP servers",
            "Run: nika init",
        );
    }

    // Check if npx is available (most MCP servers need it)
    let has_npx = std::process::Command::new("npx")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_npx {
        return DiagnosticCheck::warn(
            "MCP",
            "npx not available — most MCP servers require it",
            "Install Node.js: https://nodejs.org",
        );
    }

    // Look for MCP definitions in workflow files
    let has_mcp_workflows = fs::read_dir(".")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|s| s.ends_with(".nika.yaml"))
                })
                .any(|e| {
                    fs::read_to_string(e.path())
                        .map(|c| c.contains("mcp:"))
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false);

    if has_mcp_workflows {
        DiagnosticCheck::pass(
            "MCP",
            "MCP-enabled workflows found, npx available",
        )
    } else {
        DiagnosticCheck::pass(
            "MCP",
            "npx available (no MCP workflows in current directory)",
        )
    }
}
```

**Step 2: Run tests + commit**

```bash
cargo test -p nika-cli --lib -- doctor --nocapture
cargo check -p nika-cli
git add tools/nika-cli/src/doctor.rs
git commit -m "fix(doctor): replace MCP connectivity stub with real check

Checks npx availability and scans for MCP-enabled workflows. No longer
returns a fake PASS. Reports actual MCP readiness.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 3: Init Wizard — Editor Setup Integration

### Task 5: Add editor/AI setup question to init wizard

After mode selection, ask "Configure your editor?" and optionally run `nika setup editors` + `nika setup ai`.

**Files:**
- Modify: `tools/nika-cli/src/init_wizard.rs` (WizardResult + wizard flow)
- Modify: `tools/nika-cli/src/init.rs` (consume new WizardResult fields)

**Step 1: Add fields to WizardResult**

```rust
pub struct WizardResult {
    pub mode: InitMode,
    pub permission: String,
    pub course_dest: Option<String>,
    pub detected_providers: Vec<DetectedProvider>,
    pub migrate_keys: bool,
    /// Whether to run `nika setup editors` after init
    pub setup_editors: bool,
    /// Whether to run `nika setup ai` after init
    pub setup_ai: bool,
}
```

**Step 2: Add wizard step between keychain and outro**

After the keychain migration question (step 5), add:

```rust
// ── Step 6: Editor & AI setup ────────────────────────────────────
let setup_editors = cliclack::confirm("Install editor extension? (VS Code / Cursor)")
    .initial_value(true)
    .interact()
    .map_err(NikaError::IoError)?;

let setup_ai = cliclack::confirm("Install AI coding rules? (Claude Code, Cursor, Copilot)")
    .initial_value(true)
    .interact()
    .map_err(NikaError::IoError)?;
```

And include in the WizardResult:

```rust
Ok(WizardResult {
    mode,
    permission,
    course_dest,
    detected_providers: detected,
    migrate_keys,
    setup_editors,
    setup_ai,
})
```

**Step 3: Update non-interactive path**

In the `yes` fast path, add:

```rust
setup_editors: false,
setup_ai: false,
```

**Step 4: Consume in init.rs**

In the init handler (where WizardResult is consumed), after generating files, add:

```rust
if result.setup_editors {
    if let Err(e) = crate::setup::handle_setup_command(Some(crate::setup::SetupAction::Editors)).await {
        eprintln!("  {} Editor setup: {}", "⚠".yellow(), e);
    }
}

if result.setup_ai {
    if let Err(e) = crate::setup::handle_setup_command(Some(crate::setup::SetupAction::Ai)).await {
        eprintln!("  {} AI setup: {}", "⚠".yellow(), e);
    }
}
```

**Step 5: Run tests + commit**

```bash
cargo test -p nika-cli --lib -- init --nocapture
cargo check -p nika-cli
git add tools/nika-cli/src/init_wizard.rs tools/nika-cli/src/init.rs
git commit -m "feat(init): add editor + AI setup questions to init wizard

After mode/permission selection, wizard now asks:
- Install editor extension? (VS Code/Cursor) → runs nika setup editors
- Install AI coding rules? (Claude/Cursor/Copilot) → runs nika setup ai
Both default to yes. Skipped in --yes mode.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 4: Doctor Next Steps + Final Verification

### Task 6: Add recommended order to doctor footer

**Files:**
- Modify: `tools/nika-engine/src/display/summary.rs` (print_doctor_summary)

**Step 1: Add next_steps parameter**

Update `print_doctor_summary` signature:

```rust
pub fn print_doctor_summary(pass_count: usize, warn_count: usize, fail_count: usize, show_next_steps: bool) {
```

After the current summary line, add:

```rust
if show_next_steps && (warn_count > 0 || fail_count > 0) {
    println!();
    println!("  {}", "Recommended order:".bold());
    println!("    {} {} → initialize project", "1.".bold(), "nika init".cyan());
    println!("    {} {} → configure editors + AI tools", "2.".bold(), "nika setup".cyan());
    println!("    {} {} → verify everything works", "3.".bold(), "nika doctor".cyan());
}
println!();
```

**Step 2: Update caller in doctor.rs**

Change line 765 from:
```rust
nika_engine::display::print_doctor_summary(pass_count, warn_count, fail_count);
```
to:
```rust
nika_engine::display::print_doctor_summary(pass_count, warn_count, fail_count, true);
```

**Step 3: Update any other callers of print_doctor_summary**

Search for other callers and add the `false` parameter if they don't want next steps.

**Step 4: Run tests + commit**

```bash
cargo test -p nika-engine --lib -- display --nocapture
cargo test -p nika-cli --lib -- doctor --nocapture
cargo check --workspace
git add tools/nika-engine/src/display/summary.rs tools/nika-cli/src/doctor.rs
git commit -m "fix(doctor): add recommended order to summary footer

When warnings or failures exist, shows:
  1. nika init → initialize project
  2. nika setup → configure editors + AI tools
  3. nika doctor → verify everything works

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 7: Full workspace test + clippy

**Step 1: Run tests**

```bash
cargo test -p nika-cli --lib
cargo test -p nika-engine --lib -- display
cargo clippy -p nika-cli -- -D warnings
```

**Step 2: Fix any issues**

**Step 3: Commit fixes if needed**

```bash
git add -A
git commit -m "fix: clippy + test fixes for doctor/init UX batch

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Summary

| Task | File | Change |
|------|------|--------|
| T1 | doctor.rs | Section field + sectioned rendering + Next steps footer |
| T2 | doctor.rs | [user]/[project] scope labels on AI Rules + Agent Skills |
| T3 | doctor.rs | Real LSP check: PATH probe + nika lsp execution test |
| T4 | doctor.rs | Real MCP check: npx + workflow scan |
| T5 | init_wizard.rs + init.rs | Editor/AI setup questions in wizard |
| T6 | summary.rs + doctor.rs | Recommended order in doctor footer |
| T7 | — | Full test + clippy verification |
