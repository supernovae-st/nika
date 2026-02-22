#!/usr/bin/env bash
# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  publish-all.sh — Publish Nika crates to crates.io                            ║
# ║  Handles dependency order and crates.io index propagation delays              ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

set -euo pipefail

# ═══════════════════════════════════════════════════════════════════════════════
# Configuration
# ═══════════════════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Crates in dependency order (leaf → root)
# nika-core ← nika-mcp ← nika-provider ← nika-runtime ← nika-tui ← nika (cli)
CRATES=(
  "nika-core"
  "nika-mcp"
  "nika-provider"
  "nika-runtime"
  "nika-tui"
  "nika-cli"  # Published as "nika" on crates.io
)

# Wait time between publishes (crates.io index propagation)
WAIT_SECONDS=30

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# ═══════════════════════════════════════════════════════════════════════════════
# Helper Functions
# ═══════════════════════════════════════════════════════════════════════════════

log_info() {
  echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
  echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
  echo -e "\n${CYAN}${BOLD}══════════════════════════════════════════════════════════════${NC}"
  echo -e "${CYAN}${BOLD}  $1${NC}"
  echo -e "${CYAN}${BOLD}══════════════════════════════════════════════════════════════${NC}\n"
}

countdown() {
  local seconds=$1
  local message=$2

  for ((i=seconds; i>0; i--)); do
    printf "\r${YELLOW}%s${NC} %d seconds remaining..." "$message" "$i"
    sleep 1
  done
  printf "\r%-60s\n" ""  # Clear the line
}

get_version() {
  grep -m1 'version = "' "$WORKSPACE_ROOT/Cargo.toml" | sed 's/.*version = "\([^"]*\)".*/\1/'
}

check_cargo_login() {
  if ! cargo login --help &>/dev/null; then
    log_error "cargo is not installed or not in PATH"
    exit 1
  fi

  # Check if we have a valid token by attempting a dry-run publish
  # (This is a heuristic - the actual check happens during publish)
  if [[ ! -f "$HOME/.cargo/credentials.toml" ]] && [[ ! -f "$HOME/.cargo/credentials" ]]; then
    log_warn "No cargo credentials file found. You may need to run: cargo login"
  fi
}

check_clean_git() {
  cd "$WORKSPACE_ROOT"

  if [[ -n "$(git status --porcelain)" ]]; then
    log_error "Working directory is not clean. Commit or stash changes first."
    git status --short
    exit 1
  fi
}

check_on_main() {
  cd "$WORKSPACE_ROOT"

  local branch
  branch=$(git rev-parse --abbrev-ref HEAD)

  if [[ "$branch" != "main" ]] && [[ "$branch" != "master" ]]; then
    log_warn "Not on main/master branch (current: $branch)"
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
      exit 1
    fi
  fi
}

run_tests() {
  log_step "Running test suite"
  cd "$WORKSPACE_ROOT"

  if ! cargo test --workspace; then
    log_error "Tests failed. Fix tests before publishing."
    exit 1
  fi

  log_success "All tests passed"
}

verify_version_bump() {
  local version
  version=$(get_version)

  log_step "Verifying version: v$version"

  # Check if tag already exists
  cd "$WORKSPACE_ROOT"
  if git rev-parse "v$version" &>/dev/null; then
    log_error "Tag v$version already exists. Bump version in Cargo.toml first."
    exit 1
  fi

  # Check if any crate is already published at this version
  for crate in "${CRATES[@]}"; do
    local crate_name="$crate"
    # nika-cli is published as "nika"
    if [[ "$crate" == "nika-cli" ]]; then
      crate_name="nika"
    fi

    # Query crates.io API
    local published_version
    published_version=$(curl -s "https://crates.io/api/v1/crates/$crate_name" | \
      grep -o '"max_version":"[^"]*"' | head -1 | sed 's/.*:"\([^"]*\)".*/\1/' 2>/dev/null || echo "")

    if [[ "$published_version" == "$version" ]]; then
      log_error "$crate_name v$version is already published on crates.io"
      exit 1
    fi

    if [[ -n "$published_version" ]]; then
      log_info "$crate_name: current=$published_version, publishing=$version"
    else
      log_info "$crate_name: new crate, publishing=$version"
    fi
  done

  log_success "Version v$version is ready for publishing"
}

publish_crate() {
  local crate=$1
  local crate_dir="$WORKSPACE_ROOT/crates/$crate"

  log_info "Publishing $crate..."

  cd "$crate_dir"

  # Dry run first
  if ! cargo publish --dry-run 2>/dev/null; then
    log_error "Dry run failed for $crate"
    exit 1
  fi

  # Actual publish
  if ! cargo publish; then
    log_error "Failed to publish $crate"
    exit 1
  fi

  log_success "$crate published successfully"
}

create_git_tag() {
  local version
  version=$(get_version)

  log_step "Creating git tag v$version"

  cd "$WORKSPACE_ROOT"

  git tag -a "v$version" -m "Release v$version"
  log_success "Created tag v$version"

  read -p "Push tag to origin? [Y/n] " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Nn]$ ]]; then
    git push origin "v$version"
    log_success "Pushed tag v$version to origin"
  fi
}

# ═══════════════════════════════════════════════════════════════════════════════
# Main Script
# ═══════════════════════════════════════════════════════════════════════════════

main() {
  local dry_run=false
  local skip_tests=false
  local skip_tag=false

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case $1 in
      --dry-run)
        dry_run=true
        shift
        ;;
      --skip-tests)
        skip_tests=true
        shift
        ;;
      --skip-tag)
        skip_tag=true
        shift
        ;;
      --help|-h)
        echo "Usage: $0 [OPTIONS]"
        echo ""
        echo "Options:"
        echo "  --dry-run      Run all checks but don't actually publish"
        echo "  --skip-tests   Skip running tests (use with caution)"
        echo "  --skip-tag     Don't create git tag after publishing"
        echo "  --help, -h     Show this help message"
        exit 0
        ;;
      *)
        log_error "Unknown option: $1"
        exit 1
        ;;
    esac
  done

  local version
  version=$(get_version)

  echo -e "${BOLD}"
  echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
  echo "║                                                                               ║"
  echo "║   NIKA CRATES.IO PUBLISH SCRIPT                                               ║"
  echo "║   Version: v$version                                                          ║"
  echo "║                                                                               ║"
  echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
  echo -e "${NC}"

  if [[ "$dry_run" == true ]]; then
    log_warn "DRY RUN MODE - No actual publishing will occur"
  fi

  # Pre-flight checks
  log_step "Pre-flight checks"

  check_cargo_login
  log_success "Cargo credentials check passed"

  check_clean_git
  log_success "Git working directory is clean"

  check_on_main
  log_success "Branch check passed"

  verify_version_bump

  if [[ "$skip_tests" != true ]]; then
    run_tests
  else
    log_warn "Skipping tests (--skip-tests)"
  fi

  # Publish crates in order
  log_step "Publishing crates to crates.io"

  local total=${#CRATES[@]}
  local current=0

  for crate in "${CRATES[@]}"; do
    ((current++))

    echo -e "\n${BOLD}[$current/$total] Publishing $crate${NC}"

    if [[ "$dry_run" == true ]]; then
      log_info "[DRY RUN] Would publish $crate"
    else
      publish_crate "$crate"

      # Wait between publishes (except for the last one)
      if [[ $current -lt $total ]]; then
        countdown $WAIT_SECONDS "Waiting for crates.io index to update..."
      fi
    fi
  done

  # Create git tag
  if [[ "$skip_tag" != true ]] && [[ "$dry_run" != true ]]; then
    create_git_tag
  elif [[ "$dry_run" == true ]]; then
    log_info "[DRY RUN] Would create tag v$version"
  else
    log_warn "Skipping git tag (--skip-tag)"
  fi

  # Summary
  echo ""
  log_step "PUBLISH COMPLETE"

  if [[ "$dry_run" == true ]]; then
    log_info "Dry run completed successfully. Run without --dry-run to publish."
  else
    log_success "All ${#CRATES[@]} crates published to crates.io"
    log_success "Tag v$version created"
    echo ""
    echo "View on crates.io:"
    for crate in "${CRATES[@]}"; do
      local crate_name="$crate"
      if [[ "$crate" == "nika-cli" ]]; then
        crate_name="nika"
      fi
      echo "  https://crates.io/crates/$crate_name"
    done
  fi
}

main "$@"
