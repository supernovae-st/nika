#!/usr/bin/env bash
#
# Nika v0.3 Quick Demo
#
# Showcases the three v0.3 features:
#   1. for_each parallelism (v03-parallel-locales.yaml)
#   2. agent: verb with MCP tools (v03-agent-with-tools.yaml)
#   3. Resilience patterns (v03-resilience-demo.yaml)
#
# Prerequisites:
#   - Neo4j running: pnpm infra:up (from novanet-dev)
#   - Database seeded: pnpm infra:seed (from novanet-dev)
#   - ANTHROPIC_API_KEY set in environment
#
# Usage:
#   ./demo.sh           # Run all demos
#   ./demo.sh parallel  # Run parallel demo only
#   ./demo.sh agent     # Run agent demo only
#   ./demo.sh resilience # Run resilience demo only
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NIKA_DIR="${SCRIPT_DIR}/.."

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_status() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

check_prerequisites() {
    print_header "Checking Prerequisites"

    # Check ANTHROPIC_API_KEY
    if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
        print_error "ANTHROPIC_API_KEY not set"
        echo "  Export your API key: export ANTHROPIC_API_KEY=sk-ant-..."
        exit 1
    fi
    print_status "ANTHROPIC_API_KEY is set"

    # Check Neo4j (optional - workflows will fail gracefully)
    if nc -z localhost 7687 2>/dev/null; then
        print_status "Neo4j is running on port 7687"
    else
        print_warning "Neo4j not detected on port 7687"
        echo "  MCP tools will fail. Run: cd ../../../novanet-dev && pnpm infra:up"
    fi

    # Check Nika binary builds
    if cargo build --manifest-path "${NIKA_DIR}/Cargo.toml" --quiet 2>/dev/null; then
        print_status "Nika builds successfully"
    else
        print_error "Nika build failed"
        exit 1
    fi
}

run_demo() {
    local demo_name="$1"
    local demo_file="$2"
    local description="$3"

    print_header "${demo_name}: ${description}"

    echo "Workflow: ${demo_file}"
    echo ""

    # Validate first
    echo "Validating workflow..."
    if cargo run --manifest-path "${NIKA_DIR}/Cargo.toml" --quiet -- validate "${SCRIPT_DIR}/${demo_file}" 2>/dev/null; then
        print_status "Workflow is valid"
    else
        print_error "Workflow validation failed"
        return 1
    fi

    echo ""
    echo "Running workflow (this may take a few seconds)..."
    echo ""

    # Run the workflow
    if cargo run --manifest-path "${NIKA_DIR}/Cargo.toml" --quiet -- run "${SCRIPT_DIR}/${demo_file}"; then
        print_status "Demo completed successfully"
    else
        print_error "Demo failed"
        return 1
    fi
}

run_parallel_demo() {
    run_demo \
        "PARALLEL DEMO" \
        "v03-parallel-locales.yaml" \
        "for_each parallelism with 5 locales"
}

run_agent_demo() {
    run_demo \
        "AGENT DEMO" \
        "v03-agent-with-tools.yaml" \
        "Autonomous agent with MCP tool calling"
}

run_resilience_demo() {
    run_demo \
        "RESILIENCE DEMO" \
        "v03-resilience-demo.yaml" \
        "Retry, circuit breaker, rate limiting"
}

main() {
    print_header "Nika v0.3 Quick Demo"
    echo "This demo showcases the three major v0.3 features:"
    echo "  1. for_each parallelism (tokio::spawn JoinSet)"
    echo "  2. agent: verb with MCP tool calling"
    echo "  3. Resilience patterns (retry, circuit breaker, rate limiter)"
    echo ""

    check_prerequisites

    case "${1:-all}" in
        parallel)
            run_parallel_demo
            ;;
        agent)
            run_agent_demo
            ;;
        resilience)
            run_resilience_demo
            ;;
        all)
            run_parallel_demo
            echo ""
            run_agent_demo
            echo ""
            run_resilience_demo
            ;;
        *)
            echo "Usage: $0 [parallel|agent|resilience|all]"
            exit 1
            ;;
    esac

    print_header "Demo Complete"
    echo "Explore more examples in: ${SCRIPT_DIR}/"
    echo ""
    echo "Key v0.3 features demonstrated:"
    echo "  - for_each: Parallel iteration with concurrency control"
    echo "  - agent: Multi-turn autonomous execution with tools"
    echo "  - resilience: Exponential backoff, circuit breaker, rate limiting"
    echo ""
    echo "Run 'cargo run -- tui' for interactive workflow exploration."
}

main "$@"
