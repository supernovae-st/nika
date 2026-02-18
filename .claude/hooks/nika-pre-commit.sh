#!/bin/bash
# Hook: PreCommit (for Claude Code commit workflow)
# Purpose: BLOCKING gate - ensures quality before commit
# Exit 0 = allow commit, Exit 1 = block commit with message
#
# Enhanced with:
# - cargo-nextest: Faster parallel test execution
# - cargo-audit: Security vulnerability scanning
# - cargo-machete: Unused dependencies detection
# - cargo-tarpaulin: Code coverage analysis
# - cargo-geiger: Unsafe code detection
#
# Configuration: Reads thresholds from .nika-rust.toml if present

set -e

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
SPEC_FILE="$PROJECT_DIR/spec/SPEC.md"
ACTION_RS="$PROJECT_DIR/src/ast/action.rs"
CONFIG_FILE="$PROJECT_DIR/.nika-rust.toml"

# Track failures
FAILURES=""
WARNINGS=""
INFOS=""

# Read TOML configuration if available
read_toml_config() {
  local section="$1"
  local key="$2"
  local default="$3"

  if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "$default"
    return
  fi

  # Parse TOML section-based format
  # Extract value from [section] -> key = value
  value=$(awk -v section="$section" -v key="$key" '
    /^\[/ { in_section = 0 }
    $0 == "[" section "]" { in_section = 1; next }
    in_section && $1 == key {
      gsub(/.*=\s*/, "")
      gsub(/["# ]/, "")
      print
      exit
    }
  ' "$CONFIG_FILE" 2>/dev/null || echo "$default")

  echo "${value:-$default}"
}

# Read thresholds from config
COVERAGE_THRESHOLD=$(read_toml_config "coverage" "threshold" "80")
UNSAFE_THRESHOLD=$(read_toml_config "geiger" "max_unsafe_project" "10")

add_failure() {
  FAILURES+="- $1\n"
}

add_warning() {
  WARNINGS+="- $1\n"
}

add_info() {
  INFOS+="- $1\n"
}

# Check if tools are available
has_nextest() { command -v cargo-nextest &>/dev/null; }
has_audit() { command -v cargo-audit &>/dev/null; }
has_machete() { command -v cargo-machete &>/dev/null; }
has_tarpaulin() { command -v cargo-tarpaulin &>/dev/null; }
has_geiger() { command -v cargo-geiger &>/dev/null; }

echo "🔍 Nika Pre-Commit Validation..."
echo ""

# ============================================================================
# 1. RUST COMPILATION
# ============================================================================

echo "▶ cargo check..."
if ! cargo check --quiet 2>/dev/null; then
  add_failure "cargo check failed - code doesn't compile"
else
  echo "  ✓ Compiles"
fi

# ============================================================================
# 2. RUST FORMATTING
# ============================================================================

echo "▶ cargo fmt..."
if ! cargo fmt --check --quiet 2>/dev/null; then
  add_warning "cargo fmt: code not formatted (run 'cargo fmt')"
else
  echo "  ✓ Formatted"
fi

# ============================================================================
# 3. CLIPPY LINTS
# ============================================================================

echo "▶ cargo clippy..."
clippy_output=$(cargo clippy --message-format=short 2>&1 || true)
clippy_warnings=$(echo "$clippy_output" | grep -c "warning:" 2>/dev/null | tr -d '\n' || echo "0")
clippy_errors=$(echo "$clippy_output" | grep -c "error:" 2>/dev/null | tr -d '\n' || echo "0")

# Ensure numeric values
clippy_warnings=${clippy_warnings:-0}
clippy_errors=${clippy_errors:-0}

if [[ "$clippy_errors" -gt 0 ]]; then
  add_failure "clippy: $clippy_errors errors"
elif [[ "$clippy_warnings" -gt 5 ]]; then
  add_warning "clippy: $clippy_warnings warnings (consider fixing)"
else
  echo "  ✓ Clippy clean ($clippy_warnings warnings)"
fi

# ============================================================================
# 4. TESTS (with nextest fallback)
# ============================================================================

if has_nextest; then
  echo "▶ cargo nextest run --lib..."
  if ! cargo nextest run --lib --no-fail-fast 2>/dev/null; then
    add_failure "Tests failed (nextest)"
  else
    echo "  ✓ Tests pass (nextest - parallel)"
  fi
else
  echo "▶ cargo test --lib..."
  if ! cargo test --lib --quiet 2>/dev/null; then
    add_failure "Tests failed"
  else
    echo "  ✓ Tests pass"
  fi
  add_info "Install cargo-nextest for faster parallel tests: cargo install cargo-nextest"
fi

# ============================================================================
# 5. SCHEMA ALIGNMENT
# ============================================================================

echo "▶ Schema alignment..."
spec_schema=$(grep -o 'workflow@[0-9.]*' "$SPEC_FILE" 2>/dev/null | head -1 || echo "unknown")
code_schema=$(grep -ro 'workflow@[0-9.]*' "$PROJECT_DIR/src/" 2>/dev/null | head -1 | cut -d: -f2 || echo "unknown")

if [[ "$spec_schema" != "$code_schema" ]]; then
  add_failure "Schema mismatch: spec=$spec_schema, code=$code_schema"
else
  echo "  ✓ Schema: $spec_schema"
fi

# ============================================================================
# 6. ACTION COUNT
# ============================================================================

echo "▶ Action alignment..."
spec_actions=$(grep -cE "^### (infer|exec|fetch)" "$SPEC_FILE" 2>/dev/null || echo "0")
if [[ -f "$ACTION_RS" ]]; then
  code_actions=$(grep -cE "(Infer|Exec|Fetch)\s*\{" "$ACTION_RS" 2>/dev/null || echo "0")
else
  code_actions="0"
fi

if [[ "$spec_actions" != "$code_actions" ]]; then
  add_failure "Action count: spec=$spec_actions, code=$code_actions"
else
  echo "  ✓ Actions: $spec_actions"
fi

# ============================================================================
# 7. ERROR CODES SYNC
# ============================================================================

echo "▶ Error codes..."
spec_errors=$(grep -c "NIKA-[0-9]*" "$SPEC_FILE" 2>/dev/null || echo "0")
code_errors=$(grep -c "NIKA-[0-9]*" "$PROJECT_DIR/src/error.rs" 2>/dev/null || echo "0")

error_diff=$((code_errors - spec_errors))
if [[ ${error_diff#-} -gt 5 ]]; then
  add_warning "Error code drift: spec=$spec_errors, code=$code_errors"
else
  echo "  ✓ Error codes: spec=$spec_errors, code=$code_errors"
fi

# ============================================================================
# 8. SECURITY AUDIT
# ============================================================================

if has_audit; then
  echo "▶ cargo audit..."
  audit_output=$(cargo audit --quiet 2>&1 || true)
  audit_vulns=$(echo "$audit_output" | grep -c "Vulnerability" 2>/dev/null | tr -d '\n' || echo "0")
  audit_vulns=${audit_vulns:-0}

  if [[ "$audit_vulns" -gt 0 ]]; then
    add_warning "Security: $audit_vulns vulnerabilities found (run 'cargo audit' for details)"
  else
    echo "  ✓ No known vulnerabilities"
  fi
else
  add_info "Install cargo-audit for security scanning: cargo install cargo-audit"
fi

# ============================================================================
# 9. UNUSED DEPENDENCIES
# ============================================================================

if has_machete; then
  echo "▶ cargo machete..."
  machete_output=$(cargo machete --skip-target-dir 2>&1 || true)

  # Check for success message (no unused deps)
  if echo "$machete_output" | grep -q "didn't find any unused"; then
    echo "  ✓ No unused dependencies"
  else
    # Count actual unused crates (lines starting with spaces after "unused")
    unused_deps=$(echo "$machete_output" | grep -E "^\s+\w" | wc -l | tr -d ' ' || echo "0")
    if [[ "$unused_deps" -gt 0 ]]; then
      add_warning "Unused dependencies: $unused_deps (run 'cargo machete' for details)"
    else
      echo "  ✓ No unused dependencies"
    fi
  fi
else
  add_info "Install cargo-machete for unused deps check: cargo install cargo-machete"
fi

# ============================================================================
# 10. CODE COVERAGE (tarpaulin)
# ============================================================================

if has_tarpaulin; then
  echo "▶ cargo tarpaulin..."
  coverage_output=$(cargo tarpaulin --lib --out Stdout --skip-clean 2>&1 || true)

  # Extract coverage percentage (last occurrence of X.XX% pattern)
  coverage=$(echo "$coverage_output" | grep -oE '[0-9]+\.[0-9]+%' | tail -1 | tr -d '%' || echo "0")
  coverage=${coverage:-0}

  if (( $(echo "$coverage < $COVERAGE_THRESHOLD" | bc -l) )); then
    add_warning "Coverage: ${coverage}% (below ${COVERAGE_THRESHOLD}% threshold)"
  else
    echo "  ✓ Coverage: ${coverage}%"
  fi
else
  add_info "Install cargo-tarpaulin for coverage: cargo install cargo-tarpaulin"
fi

# ============================================================================
# 11. UNSAFE CODE ANALYSIS (geiger)
# ============================================================================

if has_geiger; then
  echo "▶ cargo geiger..."
  geiger_output=$(cargo geiger --output-format Ratio 2>&1 || true)

  # Extract unsafe counts - this outputs a ratio like "15/100"
  unsafe_count=$(echo "$geiger_output" | grep -E "^[0-9]+/[0-9]+" | cut -d'/' -f1 | head -1 || echo "0")
  unsafe_count=${unsafe_count:-0}

  if [[ "$unsafe_count" -gt "$UNSAFE_THRESHOLD" ]]; then
    add_warning "Unsafe code: $unsafe_count usage(s) detected (threshold: $UNSAFE_THRESHOLD)"
  else
    echo "  ✓ Unsafe code: $unsafe_count usage(s)"
  fi
else
  add_info "Install cargo-geiger for unsafe analysis: cargo install cargo-geiger"
fi

# ============================================================================
# RESULT
# ============================================================================

echo ""

if [[ -n "$FAILURES" ]]; then
  echo "❌ **PRE-COMMIT FAILED**"
  echo ""
  echo "**Blocking issues:**"
  echo -e "$FAILURES"

  if [[ -n "$WARNINGS" ]]; then
    echo "**Warnings:**"
    echo -e "$WARNINGS"
  fi

  echo ""
  echo "Fix these issues before committing."
  echo "Run \`/nika-sync\` for detailed report."
  exit 1
fi

if [[ -n "$WARNINGS" ]]; then
  echo "⚠️ **COMMIT ALLOWED (with warnings)**"
  echo ""
  echo "**Warnings:**"
  echo -e "$WARNINGS"
  if [[ -n "$INFOS" ]]; then
    echo "**Suggestions:**"
    echo -e "$INFOS"
  fi
  echo ""
  echo "Consider addressing these before pushing."
  exit 0
fi

echo "✅ **PRE-COMMIT PASSED**"
echo "All checks passed. Ready to commit."
if [[ -n "$INFOS" ]]; then
  echo ""
  echo "**Suggestions:**"
  echo -e "$INFOS"
fi
exit 0
