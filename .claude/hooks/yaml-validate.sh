#!/bin/bash
# Hook: yaml-validate
# Purpose: Validate Nika workflow YAML files before edit/write operations
# Exit 0 = allow, Exit 1 = block with feedback
#
# Validates:
# 1. yamllint - YAML style and syntax
# 2. JSON Schema - Structure validation (via npx ajv-cli)
# 3. Rust semantic - Workflow semantics (cargo run -- validate)

set -e

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"
NIKA_DIR="$PROJECT_DIR/tools/nika"
SCHEMA_FILE="$NIKA_DIR/schemas/nika-workflow.schema.json"
YAMLLINT_CONFIG="$NIKA_DIR/.yamllint.yaml"

# Get the file being edited (from hook context)
FILE="${1:-}"

# Skip if not a nika workflow file
if [[ -z "$FILE" ]] || [[ ! "$FILE" =~ \.nika\.yaml$ ]] && [[ ! "$FILE" =~ examples/.*\.yaml$ ]]; then
  exit 0
fi

# If relative path, make absolute
if [[ ! "$FILE" = /* ]]; then
  FILE="$PROJECT_DIR/$FILE"
fi

# Skip if file doesn't exist
if [[ ! -f "$FILE" ]]; then
  exit 0
fi

echo "🔍 Validating Nika workflow: $(basename "$FILE")"

ERRORS=""

# ============================================================================
# 1. YAMLLINT (style and syntax)
# ============================================================================

if command -v yamllint &>/dev/null; then
  if [[ -f "$YAMLLINT_CONFIG" ]]; then
    yamllint_output=$(yamllint -c "$YAMLLINT_CONFIG" "$FILE" 2>&1 || true)
  else
    yamllint_output=$(yamllint "$FILE" 2>&1 || true)
  fi

  if echo "$yamllint_output" | grep -qE "error|warning"; then
    yamllint_errors=$(echo "$yamllint_output" | grep -c "error" || echo "0")
    yamllint_warnings=$(echo "$yamllint_output" | grep -c "warning" || echo "0")

    if [[ "$yamllint_errors" -gt 0 ]]; then
      ERRORS+="yamllint: $yamllint_errors errors\n"
      echo "  ❌ yamllint: $yamllint_errors errors"
      echo "$yamllint_output" | grep "error" | head -5
    elif [[ "$yamllint_warnings" -gt 3 ]]; then
      echo "  ⚠️  yamllint: $yamllint_warnings warnings (non-blocking)"
    else
      echo "  ✓ yamllint passed"
    fi
  else
    echo "  ✓ yamllint passed"
  fi
else
  echo "  ⚠️  yamllint not installed (pip install yamllint)"
fi

# ============================================================================
# 2. JSON SCHEMA VALIDATION (structure)
# ============================================================================

if [[ -f "$SCHEMA_FILE" ]]; then
  if command -v npx &>/dev/null; then
    # Use ajv-cli for JSON Schema validation
    schema_output=$(npx ajv-cli validate -s "$SCHEMA_FILE" -d "$FILE" --strict=false 2>&1 || true)

    if echo "$schema_output" | grep -qi "invalid\|error"; then
      ERRORS+="JSON Schema validation failed\n"
      echo "  ❌ JSON Schema: validation failed"
      echo "$schema_output" | head -5
    else
      echo "  ✓ JSON Schema passed"
    fi
  else
    echo "  ⚠️  npx not available (install Node.js for schema validation)"
  fi
else
  echo "  ⚠️  Schema file not found: $SCHEMA_FILE"
fi

# ============================================================================
# 3. RUST SEMANTIC VALIDATION (deep validation)
# ============================================================================

if [[ -f "$NIKA_DIR/Cargo.toml" ]]; then
  pushd "$NIKA_DIR" >/dev/null 2>&1 || true

  # Run Nika's built-in validator
  validate_output=$(cargo run --quiet -- validate "$FILE" 2>&1 || true)

  if echo "$validate_output" | grep -qiE "NIKA-[0-9]+|error|failed"; then
    ERRORS+="Rust semantic validation failed\n"
    echo "  ❌ Semantic: validation failed"
    echo "$validate_output" | head -5
  else
    echo "  ✓ Semantic validation passed"
  fi

  popd >/dev/null 2>&1 || true
fi

# ============================================================================
# RESULT
# ============================================================================

echo ""

if [[ -n "$ERRORS" ]]; then
  echo "❌ **YAML VALIDATION FAILED**"
  echo ""
  echo "Issues found:"
  echo -e "$ERRORS"
  echo ""
  echo "Fix these issues before saving."
  echo "Use /nika-yaml skill for correct syntax reference."
  exit 1
fi

echo "✅ YAML validation passed"
exit 0
