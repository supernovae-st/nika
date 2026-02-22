#!/bin/bash
# Nika Validation Library
# Shared functions for alignment checks
# Source this file: source "$CLAUDE_PROJECT_DIR/.claude/scripts/nika-validate.sh"

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Project paths
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
SPEC_FILE="$PROJECT_DIR/spec/SPEC.md"
CLAUDE_MD="$PROJECT_DIR/CLAUDE.md"
ACTION_RS="$PROJECT_DIR/src/ast/action.rs"
ERROR_RS="$PROJECT_DIR/src/error.rs"
STATUS_FILE="$PROJECT_DIR/.claude/.nika-status"

# ============================================================================
# VALIDATION FUNCTIONS
# ============================================================================

# Check if spec file exists
check_spec_exists() {
  [[ -f "$SPEC_FILE" ]]
}

# Get schema version from spec
get_spec_schema() {
  grep -o 'workflow@[0-9.]*' "$SPEC_FILE" 2>/dev/null | head -1 || echo "unknown"
}

# Get schema version from code
get_code_schema() {
  grep -ro 'workflow@[0-9.]*' "$PROJECT_DIR/src/" 2>/dev/null | head -1 | cut -d: -f2 || echo "unknown"
}

# Count actions in spec (### headers for infer, exec, fetch)
get_spec_action_count() {
  grep -cE "^### (infer|exec|fetch)" "$SPEC_FILE" 2>/dev/null || echo "0"
}

# Count actions in code (enum variants)
get_code_action_count() {
  if [[ -f "$ACTION_RS" ]]; then
    # Count Infer, Exec, Fetch in TaskAction enum
    grep -cE "(Infer|Exec|Fetch)\s*\{" "$ACTION_RS" 2>/dev/null || echo "0"
  else
    echo "0"
  fi
}

# Count error codes in spec
get_spec_error_count() {
  grep -c "NIKA-[0-9]*" "$SPEC_FILE" 2>/dev/null || echo "0"
}

# Count error codes in code
get_code_error_count() {
  if [[ -f "$ERROR_RS" ]]; then
    grep -c "NIKA-[0-9]*" "$ERROR_RS" 2>/dev/null || echo "0"
  else
    echo "0"
  fi
}

# Get version from CLAUDE.md
get_claude_md_version() {
  grep -oE "Version \| [0-9.]+" "$CLAUDE_MD" 2>/dev/null | grep -oE "[0-9.]+" || echo "unknown"
}

# Get version from spec
get_spec_version() {
  grep -oE "\*\*0\.[0-9]+\*\*|Version.*0\.[0-9]+" "$SPEC_FILE" 2>/dev/null | grep -oE "0\.[0-9]+" | head -1 || echo "unknown"
}

# ============================================================================
# .CLAUDE/ STRUCTURE ALIGNMENT
# ============================================================================

CLAUDE_DIR="$PROJECT_DIR/.claude"
SETTINGS_JSON="$CLAUDE_DIR/settings.json"

# Required directories in .claude/
REQUIRED_DIRS=("hooks" "scripts" "commands" "skills" "agents")

# Check .claude/ directory structure
check_claude_structure() {
  local missing=0
  local dirs_status=""

  for dir in "${REQUIRED_DIRS[@]}"; do
    if [[ -d "$CLAUDE_DIR/$dir" ]]; then
      dirs_status+="$dir:OK "
    else
      dirs_status+="$dir:MISSING "
      ((missing++))
    fi
  done

  if [[ $missing -eq 0 ]]; then
    echo "OK"
  else
    echo "MISSING:$missing ($dirs_status)"
  fi
}

# Check settings.json hooks point to existing files
check_hooks_exist() {
  if [[ ! -f "$SETTINGS_JSON" ]]; then
    echo "NO_SETTINGS"
    return
  fi

  local missing=0
  local total=0

  # Extract hook paths from settings.json (handles escaped quotes in paths)
  # Format: "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/file.sh"
  local hooks=$(grep -E '"command":.*\.sh' "$SETTINGS_JSON" 2>/dev/null | \
                grep -oE '/\.claude/hooks/[^"]+\.sh' | \
                sed 's|^|'"$PROJECT_DIR"'|')

  while IFS= read -r hook_path; do
    [[ -z "$hook_path" ]] && continue
    ((total++))
    if [[ ! -f "$hook_path" ]]; then
      ((missing++))
    fi
  done <<< "$hooks"

  if [[ $total -eq 0 ]]; then
    echo "NONE"
  elif [[ $missing -eq 0 ]]; then
    echo "OK:$total"
  else
    echo "MISSING:$missing/$total"
  fi
}

# Check hooks are executable
check_hooks_executable() {
  local non_exec=0
  local total=0

  for hook in "$CLAUDE_DIR/hooks"/*.sh; do
    [[ ! -f "$hook" ]] && continue
    ((total++))
    if [[ ! -x "$hook" ]]; then
      ((non_exec++))
    fi
  done

  if [[ $non_exec -eq 0 ]]; then
    echo "OK:$total"
  else
    echo "NOT_EXEC:$non_exec/$total"
  fi
}

# Validate skills have required structure
check_skills_valid() {
  local invalid=0
  local total=0

  for skill_dir in "$CLAUDE_DIR/skills"/*/; do
    [[ ! -d "$skill_dir" ]] && continue
    ((total++))

    # Check SKILL.md exists
    if [[ ! -f "${skill_dir}SKILL.md" ]]; then
      ((invalid++))
      continue
    fi

    # Check required fields in SKILL.md (name and description)
    local skill_file="${skill_dir}SKILL.md"
    if ! grep -qE "^name:" "$skill_file" 2>/dev/null; then
      ((invalid++))
      continue
    fi
    if ! grep -qE "^description:" "$skill_file" 2>/dev/null; then
      ((invalid++))
    fi
  done

  if [[ $total -eq 0 ]]; then
    echo "NONE"
  elif [[ $invalid -eq 0 ]]; then
    echo "OK:$total"
  else
    echo "INVALID:$invalid/$total"
  fi
}

# Validate agents have required structure
check_agents_valid() {
  local invalid=0
  local total=0

  for agent in "$CLAUDE_DIR/agents"/*.md; do
    [[ ! -f "$agent" ]] && continue
    ((total++))

    # Check required fields (name and description in frontmatter or heading)
    if ! grep -qE "^#|^name:" "$agent" 2>/dev/null; then
      ((invalid++))
    fi
  done

  if [[ $total -eq 0 ]]; then
    echo "NONE"
  elif [[ $invalid -eq 0 ]]; then
    echo "OK:$total"
  else
    echo "INVALID:$invalid/$total"
  fi
}

# Validate commands have required structure
check_commands_valid() {
  local invalid=0
  local total=0

  for cmd in "$CLAUDE_DIR/commands"/*.md; do
    [[ ! -f "$cmd" ]] && continue
    ((total++))

    # Check required fields
    if ! grep -qE "^#|^name:" "$cmd" 2>/dev/null; then
      ((invalid++))
    fi
  done

  if [[ $total -eq 0 ]]; then
    echo "NONE"
  elif [[ $invalid -eq 0 ]]; then
    echo "OK:$total"
  else
    echo "INVALID:$invalid/$total"
  fi
}

# Get counts for .claude/ components
get_claude_counts() {
  local hooks=$(find "$CLAUDE_DIR/hooks" -name "*.sh" 2>/dev/null | wc -l | tr -d ' ')
  local scripts=$(find "$CLAUDE_DIR/scripts" -name "*.sh" 2>/dev/null | wc -l | tr -d ' ')
  local skills=$(find "$CLAUDE_DIR/skills" -maxdepth 1 -type d 2>/dev/null | tail -n +2 | wc -l | tr -d ' ')
  local agents=$(find "$CLAUDE_DIR/agents" -name "*.md" 2>/dev/null | wc -l | tr -d ' ')
  local commands=$(find "$CLAUDE_DIR/commands" -name "*.md" 2>/dev/null | wc -l | tr -d ' ')

  echo "hooks:$hooks scripts:$scripts skills:$skills agents:$agents commands:$commands"
}

# Full .claude/ alignment check
check_claude_alignment() {
  local issues=0

  # Structure
  local structure=$(check_claude_structure)
  [[ "$structure" != "OK" ]] && ((issues++))

  # Hooks exist
  local hooks_exist=$(check_hooks_exist)
  [[ "$hooks_exist" != OK:* ]] && ((issues++))

  # Hooks executable
  local hooks_exec=$(check_hooks_executable)
  [[ "$hooks_exec" != OK:* ]] && ((issues++))

  # Skills valid
  local skills=$(check_skills_valid)
  [[ "$skills" != OK:* && "$skills" != "NONE" ]] && ((issues++))

  # Agents valid
  local agents=$(check_agents_valid)
  [[ "$agents" != OK:* && "$agents" != "NONE" ]] && ((issues++))

  # Commands valid
  local commands=$(check_commands_valid)
  [[ "$commands" != OK:* && "$commands" != "NONE" ]] && ((issues++))

  if [[ $issues -eq 0 ]]; then
    echo "ALIGNED"
  else
    echo "ISSUES:$issues"
  fi
}

# ============================================================================
# RUST QUALITY CHECKS
# ============================================================================

# Check if enhanced tools are available
has_nextest() { command -v cargo-nextest &>/dev/null; }
has_audit() { command -v cargo-audit &>/dev/null; }
has_machete() { command -v cargo-machete &>/dev/null; }
has_tarpaulin() { command -v cargo-tarpaulin &>/dev/null; }
has_geiger() { command -v cargo-geiger &>/dev/null; }
# bacon installs as 'bacon' not 'cargo-bacon'
has_bacon() { command -v bacon &>/dev/null; }
has_expand() { command -v cargo-expand &>/dev/null; }

# Run cargo check (returns 0 if OK, 1 if errors)
rust_check() {
  cd "$PROJECT_DIR"
  cargo check 2>&1
}

# Run cargo clippy (returns warnings count)
rust_clippy_warnings() {
  cd "$PROJECT_DIR"
  cargo clippy --message-format=short 2>&1 | grep -c "warning:" | tr -d '\n' || echo "0"
}

# Run cargo test (returns 0 if OK, 1 if failures)
# Uses nextest if available for faster parallel execution
rust_test() {
  cd "$PROJECT_DIR"
  if has_nextest; then
    cargo nextest run --lib 2>&1
  else
    cargo test --lib 2>&1
  fi
}

# Check for unused code
rust_unused_count() {
  cd "$PROJECT_DIR"
  cargo check 2>&1 | grep -c "unused" | tr -d '\n' || echo "0"
}

# Security audit (returns vulnerability count)
rust_security_vulns() {
  cd "$PROJECT_DIR"
  if has_audit; then
    cargo audit --quiet 2>&1 | grep -c "Vulnerability" | tr -d '\n' || echo "0"
  else
    echo "N/A"
  fi
}

# Unused dependencies check
rust_unused_deps() {
  cd "$PROJECT_DIR"
  if has_machete; then
    local output=$(cargo machete --skip-target-dir 2>&1 || true)
    if echo "$output" | grep -q "didn't find any unused"; then
      echo "0"
    else
      echo "$output" | grep -E "^\s+\w" | wc -l | tr -d ' ' || echo "0"
    fi
  else
    echo "N/A"
  fi
}

# Get code coverage percentage
rust_coverage() {
  cd "$PROJECT_DIR"
  if has_tarpaulin; then
    local output=$(cargo tarpaulin --lib --out Stdout --skip-clean 2>&1 || true)
    echo "$output" | grep -oE '[0-9]+\.[0-9]+%' | tail -1 | tr -d '%' || echo "N/A"
  else
    echo "N/A"
  fi
}

# Get unsafe code count in project
rust_unsafe_count() {
  cd "$PROJECT_DIR"
  if has_geiger; then
    local output=$(cargo geiger --output-format Ratio 2>&1 || true)
    echo "$output" | grep -E "^[0-9]+/[0-9]+" | cut -d'/' -f1 | head -1 || echo "N/A"
  else
    echo "N/A"
  fi
}

# Load config from .nika-rust.toml (basic parsing)
# Sets global variables: COVERAGE_THRESHOLD, COVERAGE_TARGET, MAX_UNSAFE
load_config() {
  local config_file="$PROJECT_DIR/.nika-rust.toml"
  if [[ -f "$config_file" ]]; then
    # Parse values (basic TOML parsing - looking for key = value lines)
    local threshold=$(grep "^threshold = " "$config_file" | grep -oE '[0-9]+' | head -1)
    local target=$(grep "^target = " "$config_file" | grep -oE '[0-9]+' | head -1)
    local max_unsafe=$(grep "^max_unsafe_project = " "$config_file" | grep -oE '[0-9]+' | head -1)

    # Set globals with defaults if not found
    COVERAGE_THRESHOLD=${threshold:-80}
    COVERAGE_TARGET=${target:-90}
    MAX_UNSAFE=${max_unsafe:-10}
    echo "loaded"
  else
    COVERAGE_THRESHOLD=80
    COVERAGE_TARGET=90
    MAX_UNSAFE=10
    echo "defaults"
  fi
}

# Check tool versions
check_tool_versions() {
  echo "### Tool Versions"
  if command -v rustc &>/dev/null; then
    echo "- rustc: $(rustc --version | cut -d' ' -f2)"
  fi
  if command -v cargo &>/dev/null; then
    echo "- cargo: $(cargo --version | cut -d' ' -f2)"
  fi
  if has_nextest; then
    echo "- cargo-nextest: installed"
  fi
  if has_audit; then
    echo "- cargo-audit: installed"
  fi
  if has_machete; then
    echo "- cargo-machete: installed"
  fi
  if has_tarpaulin; then
    echo "- cargo-tarpaulin: installed"
  fi
  if has_geiger; then
    echo "- cargo-geiger: installed"
  fi
  if has_bacon; then
    echo "- bacon: installed"
  fi
  if has_expand; then
    echo "- cargo-expand: installed"
  fi
}

# ============================================================================
# STATUS REPORTING
# ============================================================================

# Generate quick status (for SessionStart)
quick_status() {
  local issues=0
  local warnings=0

  # Schema match
  local spec_schema=$(get_spec_schema)
  local code_schema=$(get_code_schema)
  if [[ "$spec_schema" != "$code_schema" ]]; then
    ((issues++))
  fi

  # Action count match
  local spec_actions=$(get_spec_action_count)
  local code_actions=$(get_code_action_count)
  if [[ "$spec_actions" != "$code_actions" ]]; then
    ((issues++))
  fi

  # CLAUDE.md version match
  local claude_ver=$(get_claude_md_version)
  local spec_ver=$(get_spec_version)
  if [[ "$claude_ver" != "$spec_ver" ]]; then
    ((warnings++))
  fi

  # Error codes (warning if mismatch > 2)
  local spec_errors=$(get_spec_error_count)
  local code_errors=$(get_code_error_count)
  local error_diff=$((code_errors - spec_errors))
  if [[ ${error_diff#-} -gt 2 ]]; then
    ((warnings++))
  fi

  # .claude/ structure alignment
  local claude_align=$(check_claude_alignment)
  if [[ "$claude_align" != "ALIGNED" ]]; then
    ((warnings++))
  fi

  # Return status
  if [[ $issues -gt 0 ]]; then
    echo "BROKEN"
  elif [[ $warnings -gt 0 ]]; then
    echo "DRIFT"
  else
    echo "ALIGNED"
  fi
}

# Generate detailed report
full_report() {
  echo "## Nika Sync Report"
  echo ""
  echo "**Timestamp:** $(date '+%Y-%m-%d %H:%M:%S')"
  echo ""

  # Load configuration (sets global variables and returns status)
  load_config >/dev/null 2>&1
  local config_status
  if [[ -f "$PROJECT_DIR/.nika-rust.toml" ]]; then
    config_status="loaded"
  else
    config_status="defaults"
  fi

  # Schema
  local spec_schema=$(get_spec_schema)
  local code_schema=$(get_code_schema)
  if [[ "$spec_schema" == "$code_schema" ]]; then
    echo "- [x] Schema: $spec_schema"
  else
    echo "- [ ] Schema: spec=$spec_schema, code=$code_schema **MISMATCH**"
  fi

  # Actions
  local spec_actions=$(get_spec_action_count)
  local code_actions=$(get_code_action_count)
  if [[ "$spec_actions" == "$code_actions" ]]; then
    echo "- [x] Actions: $spec_actions"
  else
    echo "- [ ] Actions: spec=$spec_actions, code=$code_actions **MISMATCH**"
  fi

  # Version
  local claude_ver=$(get_claude_md_version)
  local spec_ver=$(get_spec_version)
  if [[ "$claude_ver" == "$spec_ver" ]]; then
    echo "- [x] Version: $spec_ver"
  else
    echo "- [ ] Version: CLAUDE.md=$claude_ver, spec=$spec_ver **DRIFT**"
  fi

  # Error codes
  local spec_errors=$(get_spec_error_count)
  local code_errors=$(get_code_error_count)
  echo "- [x] Error codes: spec=$spec_errors, code=$code_errors"

  echo ""
  echo "### .claude/ Structure Alignment"

  # Directory structure
  local structure=$(check_claude_structure)
  if [[ "$structure" == "OK" ]]; then
    echo "- [x] Directories: all required present (hooks, scripts, commands, skills, agents)"
  else
    echo "- [ ] Directories: $structure **MISSING**"
  fi

  # Hooks exist (declared in settings.json)
  local hooks_exist=$(check_hooks_exist)
  if [[ "$hooks_exist" == OK:* ]]; then
    local hook_count=$(echo "$hooks_exist" | cut -d: -f2)
    echo "- [x] Hooks declared: $hook_count (all exist)"
  elif [[ "$hooks_exist" == "NO_SETTINGS" ]]; then
    echo "- [ ] Hooks: no settings.json found"
  else
    echo "- [ ] Hooks declared: $hooks_exist **MISSING FILES**"
  fi

  # Hooks executable
  local hooks_exec=$(check_hooks_executable)
  if [[ "$hooks_exec" == OK:* ]]; then
    local exec_count=$(echo "$hooks_exec" | cut -d: -f2)
    echo "- [x] Hooks executable: $exec_count"
  else
    echo "- [ ] Hooks executable: $hooks_exec **FIX: chmod +x**"
  fi

  # Skills validation
  local skills_valid=$(check_skills_valid)
  if [[ "$skills_valid" == OK:* ]]; then
    local skill_count=$(echo "$skills_valid" | cut -d: -f2)
    echo "- [x] Skills: $skill_count valid"
  elif [[ "$skills_valid" == "NONE" ]]; then
    echo "- [x] Skills: none defined"
  else
    echo "- [ ] Skills: $skills_valid **INVALID STRUCTURE**"
  fi

  # Agents validation
  local agents_valid=$(check_agents_valid)
  if [[ "$agents_valid" == OK:* ]]; then
    local agent_count=$(echo "$agents_valid" | cut -d: -f2)
    echo "- [x] Agents: $agent_count valid"
  elif [[ "$agents_valid" == "NONE" ]]; then
    echo "- [x] Agents: none defined"
  else
    echo "- [ ] Agents: $agents_valid **INVALID STRUCTURE**"
  fi

  # Commands validation
  local commands_valid=$(check_commands_valid)
  if [[ "$commands_valid" == OK:* ]]; then
    local cmd_count=$(echo "$commands_valid" | cut -d: -f2)
    echo "- [x] Commands: $cmd_count valid"
  elif [[ "$commands_valid" == "NONE" ]]; then
    echo "- [x] Commands: none defined"
  else
    echo "- [ ] Commands: $commands_valid **INVALID STRUCTURE**"
  fi

  # Component counts summary
  local counts=$(get_claude_counts)
  echo ""
  echo "  **Inventory:** $counts"

  echo ""
  echo "### Configuration"
  if [[ "$config_status" == "loaded" ]]; then
    echo "- [x] Config: loaded from .nika-rust.toml"
    echo "  - Coverage threshold: ${COVERAGE_THRESHOLD}%"
    echo "  - Coverage target: ${COVERAGE_TARGET}%"
    echo "  - Max unsafe blocks: ${MAX_UNSAFE}"
  else
    echo "- [ ] Config: using defaults (create .nika-rust.toml to customize)"
    echo "  - Coverage threshold: ${COVERAGE_THRESHOLD}%"
    echo "  - Coverage target: ${COVERAGE_TARGET}%"
    echo "  - Max unsafe blocks: ${MAX_UNSAFE}"
  fi

  echo ""
  echo "### Rust Quality"

  # Clippy warnings
  local clippy_warns=$(rust_clippy_warnings 2>/dev/null || echo "?")
  if [[ "$clippy_warns" == "0" ]]; then
    echo "- [x] Clippy: clean"
  else
    echo "- [ ] Clippy: $clippy_warns warnings"
  fi

  # Unused code
  local unused=$(rust_unused_count 2>/dev/null || echo "?")
  if [[ "$unused" == "0" ]]; then
    echo "- [x] Unused code: none"
  else
    echo "- [ ] Unused code: $unused items"
  fi

  # Security audit
  local vulns=$(rust_security_vulns 2>/dev/null || echo "?")
  if [[ "$vulns" == "N/A" ]]; then
    echo "- [ ] Security: cargo-audit not installed"
  elif [[ "$vulns" == "0" ]]; then
    echo "- [x] Security: no vulnerabilities"
  else
    echo "- [ ] Security: $vulns vulnerabilities"
  fi

  # Unused dependencies
  local unused_deps=$(rust_unused_deps 2>/dev/null || echo "?")
  if [[ "$unused_deps" == "N/A" ]]; then
    echo "- [ ] Dependencies: cargo-machete not installed"
  elif [[ "$unused_deps" == "0" ]]; then
    echo "- [x] Dependencies: no unused"
  else
    echo "- [ ] Dependencies: $unused_deps unused"
  fi

  echo ""
  echo "### Code Coverage"
  local coverage=$(rust_coverage 2>/dev/null || echo "?")
  if [[ "$coverage" == "N/A" ]]; then
    echo "- [ ] Coverage: cargo-tarpaulin not installed"
  else
    # Compare with thresholds
    local coverage_int=$(echo "$coverage" | cut -d'.' -f1)
    if [[ "$coverage_int" -ge "$COVERAGE_TARGET" ]]; then
      echo "- [x] Coverage: ${coverage}% (target: ${COVERAGE_TARGET}%)"
    elif [[ "$coverage_int" -ge "$COVERAGE_THRESHOLD" ]]; then
      echo "- [x] Coverage: ${coverage}% (threshold: ${COVERAGE_THRESHOLD}%, target: ${COVERAGE_TARGET}%)"
    else
      echo "- [ ] Coverage: ${coverage}% **BELOW THRESHOLD** (min: ${COVERAGE_THRESHOLD}%)"
    fi
  fi

  echo ""
  echo "### Unsafe Analysis"
  local unsafe_count=$(rust_unsafe_count 2>/dev/null || echo "?")
  if [[ "$unsafe_count" == "N/A" ]]; then
    echo "- [ ] Unsafe code: cargo-geiger not installed"
  elif [[ "$unsafe_count" == "0" ]]; then
    echo "- [x] Unsafe code: none (100% safe Rust)"
  else
    if [[ "$unsafe_count" -le "$MAX_UNSAFE" ]]; then
      echo "- [x] Unsafe code: $unsafe_count blocks (max: $MAX_UNSAFE)"
    else
      echo "- [ ] Unsafe code: $unsafe_count blocks **EXCEEDS LIMIT** (max: $MAX_UNSAFE)"
    fi
  fi

  echo ""
  check_tool_versions

  echo ""
  echo "### Enhanced Tools"
  if has_nextest; then
    echo "- [x] cargo-nextest: installed (parallel tests)"
  else
    echo "- [ ] cargo-nextest: not installed"
  fi
  if has_audit; then
    echo "- [x] cargo-audit: installed (security)"
  else
    echo "- [ ] cargo-audit: not installed"
  fi
  if has_machete; then
    echo "- [x] cargo-machete: installed (unused deps)"
  else
    echo "- [ ] cargo-machete: not installed"
  fi
  if has_tarpaulin; then
    echo "- [x] cargo-tarpaulin: installed (coverage)"
  else
    echo "- [ ] cargo-tarpaulin: not installed"
  fi
  if has_geiger; then
    echo "- [x] cargo-geiger: installed (unsafe detection)"
  else
    echo "- [ ] cargo-geiger: not installed"
  fi
  if has_bacon; then
    echo "- [x] bacon: installed (watch mode)"
  else
    echo "- [ ] bacon: not installed"
  fi
  if has_expand; then
    echo "- [x] cargo-expand: installed (macro expansion)"
  else
    echo "- [ ] cargo-expand: not installed"
  fi
}

# Save status to cache
save_status() {
  local status=$1
  mkdir -p "$(dirname "$STATUS_FILE")"
  echo "{\"status\": \"$status\", \"timestamp\": \"$(date +%Y-%m-%dT%H:%M:%S)\"}" > "$STATUS_FILE"
}

# Load cached status
load_status() {
  if [[ -f "$STATUS_FILE" ]]; then
    cat "$STATUS_FILE"
  else
    echo "{\"status\": \"unknown\", \"timestamp\": \"never\"}"
  fi
}

# ============================================================================
# MAIN (if run directly)
# ============================================================================

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  case "${1:-status}" in
    status)
      quick_status
      ;;
    report)
      full_report
      ;;
    schema)
      echo "spec: $(get_spec_schema)"
      echo "code: $(get_code_schema)"
      ;;
    actions)
      echo "spec: $(get_spec_action_count)"
      echo "code: $(get_code_action_count)"
      ;;
    claude)
      echo "## .claude/ Structure Check"
      echo ""
      echo "Structure: $(check_claude_structure)"
      echo "Hooks exist: $(check_hooks_exist)"
      echo "Hooks executable: $(check_hooks_executable)"
      echo "Skills: $(check_skills_valid)"
      echo "Agents: $(check_agents_valid)"
      echo "Commands: $(check_commands_valid)"
      echo ""
      echo "Counts: $(get_claude_counts)"
      echo ""
      echo "Overall: $(check_claude_alignment)"
      ;;
    *)
      echo "Usage: nika-validate.sh [status|report|schema|actions|claude]"
      ;;
  esac
fi
