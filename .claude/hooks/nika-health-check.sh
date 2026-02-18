#!/bin/bash
# Hook: SessionStart
# Purpose: Comprehensive health check at startup (fast, ~2s)
# Exit 0 + stdout = context injected
# stderr = visible in user terminal

set -e

# Visual feedback to user terminal (stderr)
echo "" >&2

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
SPEC_FILE="$PROJECT_DIR/spec/SPEC.md"
CLAUDE_MD="$PROJECT_DIR/CLAUDE.md"
ACTION_RS="$PROJECT_DIR/src/ast/action.rs"

# Quick existence checks
if [[ ! -f "$SPEC_FILE" ]]; then
  echo "<nika-health status=\"error\">"
  echo "spec/SPEC.md NOT FOUND - critical file missing"
  echo "</nika-health>"
  exit 0
fi

# ============================================================================
# GATHER METRICS (fast, no cargo commands)
# ============================================================================

# Schema versions
spec_schema=$(grep -o 'workflow@[0-9.]*' "$SPEC_FILE" 2>/dev/null | head -1 || echo "?")
code_schema=$(grep -ro 'workflow@[0-9.]*' "$PROJECT_DIR/src/" 2>/dev/null | head -1 | cut -d: -f2 || echo "?")

# Action counts
spec_actions=$(grep -cE "^### (infer|exec|fetch)" "$SPEC_FILE" 2>/dev/null || echo "0")
if [[ -f "$ACTION_RS" ]]; then
  code_actions=$(grep -cE "(Infer|Exec|Fetch)\s*\{" "$ACTION_RS" 2>/dev/null || echo "0")
else
  code_actions="?"
fi

# Versions
spec_version=$(grep -oE "0\.[0-9]+" "$SPEC_FILE" 2>/dev/null | head -1 || echo "?")
claude_version=$(grep -oE "Version \| [0-9.]+" "$CLAUDE_MD" 2>/dev/null | grep -oE "[0-9.]+" || echo "?")

# Error codes
spec_errors=$(grep -c "NIKA-[0-9]*" "$SPEC_FILE" 2>/dev/null || echo "0")
code_errors=$(grep -c "NIKA-[0-9]*" "$PROJECT_DIR/src/error.rs" 2>/dev/null || echo "0")

# ============================================================================
# DETERMINE STATUS
# ============================================================================

issues=""
warnings=""

# Schema match
if [[ "$spec_schema" != "$code_schema" && "$code_schema" != "?" ]]; then
  issues+="Schema mismatch: spec=$spec_schema, code=$code_schema\n"
fi

# Action count match
if [[ "$spec_actions" != "$code_actions" && "$code_actions" != "?" ]]; then
  issues+="Action count: spec=$spec_actions, code=$code_actions\n"
fi

# Version match
if [[ "$spec_version" != "$claude_version" && "$claude_version" != "?" ]]; then
  warnings+="CLAUDE.md version drift: $claude_version vs spec $spec_version\n"
fi

# Error code drift (warning if diff > 3)
error_diff=$((code_errors - spec_errors))
if [[ ${error_diff#-} -gt 3 ]]; then
  warnings+="Error codes: spec=$spec_errors, code=$code_errors (diff=${error_diff#-})\n"
fi

# ============================================================================
# OUTPUT STATUS
# ============================================================================

if [[ -n "$issues" ]]; then
  status="BROKEN"
  icon="🔴"
elif [[ -n "$warnings" ]]; then
  status="DRIFT"
  icon="🟡"
else
  status="ALIGNED"
  icon="🟢"
fi

# Visual feedback to user terminal (stderr)
echo "$icon Nika v$spec_version | $status | Schema: $spec_schema | Actions: $spec_actions" >&2

echo "<nika-health status=\"$status\">"
echo "$icon **Nika v$spec_version** | Schema: $spec_schema | Actions: $spec_actions"
echo ""

if [[ -n "$issues" ]]; then
  echo "**ISSUES:**"
  echo -e "$issues"
fi

if [[ -n "$warnings" ]]; then
  echo "**Warnings:**"
  echo -e "$warnings"
fi

if [[ "$status" == "ALIGNED" ]]; then
  echo "Spec-Code alignment: OK"
fi

echo ""
echo "Run \`/nika-sync\` for detailed validation."
echo "</nika-health>"

# Save status cache
mkdir -p "$PROJECT_DIR/.claude"
echo "{\"status\": \"$status\", \"timestamp\": \"$(date +%Y-%m-%dT%H:%M:%S)\", \"schema\": \"$spec_schema\", \"actions\": $spec_actions}" > "$PROJECT_DIR/.claude/.nika-status"

# ============================================================================
# PRELOAD DEVELOPMENT CONTEXT
# ============================================================================

echo ""
echo "<dev-context project=\"nika\" version=\"0.1\">"

# ─────────────────────────────────────────────────────────────────────────────
# NIKA SPEC ESSENTIALS
# ─────────────────────────────────────────────────────────────────────────────
echo "## Nika v0.1 Essentials"
echo ""
echo "**Schema:** \`nika/workflow@0.1\`"
echo "**Source of Truth:** \`spec/SPEC.md\`"
echo ""
echo "### Actions (3 only)"
echo "| Action | Purpose | Key Fields |"
echo "|--------|---------|------------|"
echo "| \`infer:\` | LLM call | \`prompt\`, \`provider?\`, \`model?\` |"
echo "| \`exec:\` | Shell cmd | \`command\` |"
echo "| \`fetch:\` | HTTP req | \`url\`, \`method?\`, \`headers?\`, \`body?\` |"
echo ""
echo "### Data Flow"
echo "\`\`\`"
echo "Task A → DataStore → use: block → {{use.alias}} → Task B"
echo "\`\`\`"
echo ""
echo "### Anti-Hallucination (v0.1 does NOT have)"
echo "- ❌ \`agent:\` action (use \`infer:\`)"
echo "- ❌ \`invoke:\` action"
echo "- ❌ SHAKA system"
echo "- ❌ scope presets (minimal, default, debug, full)"
echo "- ❌ manifest files (single workflow file only)"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
# RUST SKILLS & AGENTS (spn-rust)
# ─────────────────────────────────────────────────────────────────────────────
echo "## Rust Development (spn-rust)"
echo ""
echo "### Skills"
echo "| Skill | When to Use |"
echo "|-------|-------------|"
echo "| \`spn-rust:rust-core\` | Ownership, borrowing, thiserror/anyhow, type-state, traits |"
echo "| \`spn-rust:rust-async\` | Tokio, spawn, channels (mpsc/oneshot/broadcast), DashMap |"
echo "| \`spn-rust:rust-agentic\` | LLM agents, tool calling, DAG workflows, RAG |"
echo "| \`spn-rust:rust-ai\` | ONNX (ort), Candle ML, MCP (rmcp), tiktoken |"
echo ""
echo "### Agents"
echo "| Agent | Specialty |"
echo "|-------|-----------|"
echo "| \`spn-rust:rust-pro\` | General Rust dev & code review |"
echo "| \`spn-rust:rust-async-expert\` | Tokio/async (Nika uses Tokio) |"
echo "| \`spn-rust:rust-perf\` | Performance profiling |"
echo "| \`spn-rust:rust-security\` | Security audit |"
echo "| \`spn-rust:rust-architect\` | System design |"
echo ""
echo "### Commands"
echo "\`/rust-new\` \`/rust-bench\` \`/rust-audit\` \`/rust-tokio\` \`/rust-perf\`"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
# NIKA TOOLS (local skills, agents, commands)
# ─────────────────────────────────────────────────────────────────────────────
echo "## Nika DX Tools"
echo ""
echo "### Commands"
echo "| Command | Purpose |"
echo "|---------|---------|"
echo "| \`/nika-sync\` | Full spec-code-docs alignment validation |"
echo "| \`/nika-deep-verify\` | Launch 6 parallel Haiku agents for comprehensive check |"
echo ""
echo "### Agents (use with Task tool)"
echo "| Agent | Purpose |"
echo "|-------|---------|"
echo "| \`nika-sync\` | Validate alignment + Rust quality |"
echo "| \`verify-spec\` | Validate spec/SPEC.md structure |"
echo "| \`verify-code\` | Validate src/ implements spec |"
echo "| \`verify-docs\` | Validate CLAUDE.md + README accuracy |"
echo "| \`verify-rust-conventions\` | Rust best practices check |"
echo "| \`verify-logic\` | Business logic consistency |"
echo "| \`verify-claude-structure\` | .claude/ directory validation |"
echo ""
echo "### Skills"
echo "| Skill | Purpose |"
echo "|-------|---------|"
echo "| \`nika-spec\` | Routes to spec/SPEC.md sections |"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
# NIKA DEPENDENCIES (from Cargo.toml)
# ─────────────────────────────────────────────────────────────────────────────
echo "## Nika Dependencies"
echo "\`\`\`toml"
echo "# Async"
echo "tokio = { version = \"1.48\", features = [\"rt-multi-thread\", \"macros\", \"sync\"] }"
echo "async-trait = \"0.1\""
echo ""
echo "# Errors"
echo "thiserror = \"1.0\"  # Library errors"
echo "anyhow = \"1.0\"     # App errors with context"
echo ""
echo "# Performance"
echo "dashmap = \"6.1\"       # Thread-safe HashMap"
echo "parking_lot = \"0.12\"  # 2-3x faster RwLock"
echo "smallvec = \"1.13\"     # Stack-allocated vectors"
echo "rustc-hash = \"2.1\"    # FxHashMap for small keys"
echo ""
echo "# Serialization"
echo "serde = { version = \"1.0\", features = [\"derive\", \"rc\"] }"
echo "serde_yaml = \"0.9\""
echo "serde_json = \"1.0\""
echo "\`\`\`"
echo ""

# ─────────────────────────────────────────────────────────────────────────────
# QUICK PATTERNS
# ─────────────────────────────────────────────────────────────────────────────
echo "## Quick Patterns"
echo ""
echo "**Error Handling:**"
echo "\`\`\`rust"
echo "// thiserror for library"
echo "#[derive(Error, Debug)]"
echo "pub enum NikaError {"
echo "    #[error(\"NIKA-{code}: {message}\")]"
echo "    Validation { code: u16, message: String },"
echo "    #[error(transparent)]"
echo "    Io(#[from] std::io::Error),"
echo "}"
echo ""
echo "// anyhow for app with context"
echo "fs::read(path).context(\"failed to read workflow\")?;"
echo "\`\`\`"
echo ""
echo "**Tokio + DashMap:**"
echo "\`\`\`rust"
echo "let store: Arc<DashMap<Arc<str>, TaskResult>> = Arc::new(DashMap::new());"
echo "let handle = tokio::spawn(async move {"
echo "    store.insert(task_id.into(), result);"
echo "});"
echo "let (a, b) = tokio::join!(task_a(), task_b());"
echo "\`\`\`"
echo ""
echo "**For deep patterns:** \`Skill(\"spn-rust:rust-core\")\` or \`Skill(\"spn-rust:rust-async\")\`"
echo "</dev-context>"

exit 0
