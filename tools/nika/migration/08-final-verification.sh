#!/usr/bin/env bash
# Migration Step 8: Final Verification
# Comprehensive testing of the entire workspace

set -Eeuo pipefail
shopt -s inherit_errexit

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly MIGRATION_LOG="$PROJECT_ROOT/migration/migration.log"
readonly CHECKPOINT_DIR="$PROJECT_ROOT/migration/checkpoints"

readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly BLUE='\033[0;34m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "$MIGRATION_LOG"; }

# === Verification checks ===
verify_workspace_structure() {
  log_info "Verifying workspace structure..."

  local expected_crates=(
    "nika-core"
    "nika-runtime"
    "nika-provider"
    "nika-mcp"
    "nika-tui"
    "nika-cli"
  )

  for crate in "${expected_crates[@]}"; do
    if [[ ! -d "$PROJECT_ROOT/crates/$crate" ]]; then
      log_error "Missing crate: $crate"
      return 1
    fi

    if [[ ! -f "$PROJECT_ROOT/crates/$crate/Cargo.toml" ]]; then
      log_error "Missing Cargo.toml in: $crate"
      return 1
    fi

    if [[ ! -f "$PROJECT_ROOT/crates/$crate/src/lib.rs" ]] && [[ "$crate" != "nika-cli" ]]; then
      log_error "Missing lib.rs in: $crate"
      return 1
    fi
  done

  log_success "Workspace structure verified"
}

verify_all_builds() {
  log_info "Verifying all crates build..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build --workspace --all-targets 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Workspace build failed"
    return 1
  fi

  log_success "All crates build successfully"
}

verify_all_tests() {
  log_info "Running all tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test --workspace --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Some tests failed"
    return 1
  fi

  log_success "All tests pass"
}

verify_clippy() {
  log_info "Running clippy on workspace..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Clippy warnings found"
    return 1
  fi

  log_success "Zero clippy warnings"
}

verify_docs() {
  log_info "Verifying documentation builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo doc --workspace --no-deps 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Documentation build failed"
    return 1
  fi

  log_success "Documentation builds successfully"
}

verify_binary() {
  log_info "Testing nika binary..."

  cd "$PROJECT_ROOT" || exit 1

  # Test --version
  if ! cargo run -p nika-cli -- --version 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Binary --version failed"
    return 1
  fi

  # Test --help
  if ! cargo run -p nika-cli -- --help 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Binary --help failed"
    return 1
  fi

  log_success "Binary works correctly"
}

count_tests() {
  log_info "Counting tests in workspace..."

  cd "$PROJECT_ROOT" || exit 1

  local test_count
  test_count=$(cargo test --workspace --lib -- --list 2>&1 | grep -c "test " || true)

  log_info "Total tests: $test_count"

  if [[ $test_count -lt 4000 ]]; then
    log_warn "Test count is lower than expected baseline (4,433)"
    log_warn "Some tests may have been lost during migration"
  fi

  echo "$test_count" > "$CHECKPOINT_DIR/final-test-count.txt"

  log_success "Test count recorded"
}

compare_with_baseline() {
  log_info "Comparing with baseline..."

  local baseline_checkpoint="$CHECKPOINT_DIR/00-initial-state"

  if [[ ! -f "$baseline_checkpoint/test-count.txt" ]]; then
    log_warn "No baseline test count found"
    return 0
  fi

  local baseline_count
  baseline_count=$(cat "$baseline_checkpoint/test-count.txt" | grep -oP '\d+(?= passed)' || echo "0")

  local current_count
  current_count=$(cat "$CHECKPOINT_DIR/final-test-count.txt")

  log_info "Baseline tests: $baseline_count"
  log_info "Current tests: $current_count"

  if [[ $current_count -lt $baseline_count ]]; then
    log_warn "Lost $(( baseline_count - current_count )) tests during migration"
  else
    log_success "All tests preserved or increased"
  fi
}

verify_imports() {
  log_info "Checking for remaining crate:: imports..."

  cd "$PROJECT_ROOT" || exit 1

  local bad_imports=0

  # Check for old crate:: imports that should have been rewritten
  if grep -r "use crate::ast" crates/nika-runtime crates/nika-provider crates/nika-mcp crates/nika-tui crates/nika-cli 2>/dev/null; then
    log_error "Found unrewritten 'use crate::ast' imports"
    bad_imports=1
  fi

  if grep -r "use crate::runtime" crates/nika-core crates/nika-provider crates/nika-mcp crates/nika-tui crates/nika-cli 2>/dev/null; then
    log_error "Found unrewritten 'use crate::runtime' imports"
    bad_imports=1
  fi

  if [[ $bad_imports -eq 1 ]]; then
    log_error "Import rewriting incomplete"
    return 1
  fi

  log_success "All imports correctly rewritten"
}

generate_migration_report() {
  log_info "Generating migration report..."

  local report_path="$PROJECT_ROOT/migration/MIGRATION_REPORT.md"

  cat > "$report_path" <<EOF
# Nika v0.28 Migration Report

**Date:** $(date)
**Status:** Complete

## Workspace Structure

\`\`\`
nika/
├── Cargo.toml (workspace)
└── crates/
    ├── nika-core/      (AST, DAG, error)
    ├── nika-runtime/   (executor, binding, event)
    ├── nika-provider/  (rig, native)
    ├── nika-mcp/       (MCP client)
    ├── nika-tui/       (TUI views)
    └── nika-cli/       (binary)
\`\`\`

## Migration Steps

1. ✅ Setup and preparation
2. ✅ Move nika-core modules
3. ✅ Move nika-runtime modules
4. ✅ Move nika-provider modules
5. ✅ Move nika-mcp modules
6. ✅ Move nika-tui modules
7. ✅ Build nika-cli binary
8. ✅ Final verification

## Verification Results

- **Build:** $(cargo build --workspace 2>&1 >/dev/null && echo "✅ Pass" || echo "❌ Fail")
- **Tests:** $(cargo test --workspace --lib 2>&1 >/dev/null && echo "✅ Pass" || echo "❌ Fail")
- **Clippy:** $(cargo clippy --workspace -- -D warnings 2>&1 >/dev/null && echo "✅ Pass" || echo "❌ Fail")
- **Docs:** $(cargo doc --workspace --no-deps 2>&1 >/dev/null && echo "✅ Pass" || echo "❌ Fail")

## Test Count

- **Baseline:** $(cat "$CHECKPOINT_DIR/00-initial-state/test-count.txt" 2>/dev/null | grep -oP '\d+(?= passed)' || echo "N/A")
- **Final:** $(cat "$CHECKPOINT_DIR/final-test-count.txt" 2>/dev/null || echo "N/A")

## Commits

\`\`\`
$(git log --oneline --grep="refactor(v0.28)" | head -n 10)
\`\`\`

## Next Steps

1. Update CHANGELOG.md with v0.28 release notes
2. Update README.md with new workspace structure
3. Update CLAUDE.md documentation
4. Tag release: \`git tag v0.28.0\`
5. Push to remote: \`git push origin main --tags\`

EOF

  log_success "Migration report generated: $report_path"
}

create_final_checkpoint() {
  local checkpoint_name="08-final-state"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates -type f -name "*.rs" | wc -l > "$checkpoint_path/rust-file-count.txt"

  log_success "Final checkpoint created: $checkpoint_path"
}

# === Main execution ===
main() {
  log_info "Starting Step 8: Final Verification..."

  verify_workspace_structure
  verify_all_builds
  verify_all_tests
  verify_clippy
  verify_docs
  verify_binary
  count_tests
  compare_with_baseline
  verify_imports
  generate_migration_report
  create_final_checkpoint

  log_success "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_success "  MIGRATION COMPLETE! 🦋"
  log_success "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_info ""
  log_info "Workspace restructure successful!"
  log_info "Report: migration/MIGRATION_REPORT.md"
  log_info ""
  log_info "Next steps:"
  log_info "  1. Review migration report"
  log_info "  2. Update documentation"
  log_info "  3. Tag release: git tag v0.28.0"
  log_info ""
}

main "$@"
