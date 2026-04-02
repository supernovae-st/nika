#!/usr/bin/env bash
# detect-workspace.sh — SessionStart hook for Nika plugin
#
# Detects the Nika workspace state on session start:
# - Checks if nika binary is installed
# - Counts .nika.yaml workflow files in the workspace
# - Detects .nika/ project directory
# - Reports course installation status
# - Suggests next actions
#
# Exit codes:
#   0 — Always (informational only, never blocks)
#
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 SuperNovae Studio

set -euo pipefail

# ─── Binary Detection ─────────────────────────────────────────────────────

NIKA_VERSION="not installed"
if command -v nika &>/dev/null; then
  NIKA_VERSION=$(nika --version 2>/dev/null | head -1 || echo "installed (version unknown)")
fi

# ─── Workspace Detection ──────────────────────────────────────────────────

# Count .nika.yaml files (limit search depth for performance)
YAML_COUNT=$(find . -name '*.nika.yaml' -maxdepth 5 2>/dev/null | wc -l | tr -d ' ')

# Check for .nika project directory
NIKA_DIR="not found"
if [ -d ".nika" ]; then
  NIKA_DIR="found"
fi

# Check for course
COURSE_STATUS="not installed"
if [ -f ".nika/course-progress.toml" ]; then
  COURSE_STATUS="installed"
  # Count completed exercises if possible
  COMPLETED=$(grep -c 'status.*=.*"completed"' .nika/course-progress.toml 2>/dev/null || echo "0")
  if [ "$COMPLETED" -gt 0 ]; then
    COURSE_STATUS="installed ($COMPLETED completed)"
  fi
fi

# Check for MCP config (.mcp.json preferred, .nika/mcp.yaml fallback)
MCP_STATUS="no config"
if [ -f ".mcp.json" ]; then
  SERVER_COUNT=$(grep -c '"command"' .mcp.json 2>/dev/null || echo "0")
  MCP_STATUS="$SERVER_COUNT server(s) (.mcp.json)"
elif [ -f ".nika/mcp.yaml" ]; then
  SERVER_COUNT=$(grep -c '^  [a-z]' .nika/mcp.yaml 2>/dev/null || echo "0")
  MCP_STATUS="$SERVER_COUNT server(s) (.nika/mcp.yaml)"
fi

# ─── Report ────────────────────────────────────────────────────────────────

echo "Nika plugin active"
echo "  Binary:    $NIKA_VERSION"
echo "  Workflows: $YAML_COUNT .nika.yaml file(s)"
echo "  Project:   .nika/ $NIKA_DIR"
echo "  Course:    $COURSE_STATUS"
echo "  MCP:       $MCP_STATUS"

# ─── Suggestions ───────────────────────────────────────────────────────────

if [ "$NIKA_VERSION" = "not installed" ]; then
  echo ""
  echo "Tip: Install Nika with 'cargo install nika' or 'brew install nika'"
  echo "     Use /nika-setup for guided installation"
elif [ "$NIKA_DIR" = "not found" ] && [ "$YAML_COUNT" -eq 0 ]; then
  echo ""
  echo "Tip: Initialize a Nika project with 'nika init'"
  echo "     Use /nika-wizard to create your first workflow"
elif [ "$YAML_COUNT" -gt 0 ]; then
  echo ""
  echo "Available commands:"
  echo "  /nika-wizard        Create a new workflow"
  echo "  /nika-doctor        Run diagnostics"
  echo "  /nika-course-tutor  Course help (if installed)"
  echo "  /workflow-architect  Design complex workflows"
  echo "  /workflow-debugger   Debug failing workflows"
fi
