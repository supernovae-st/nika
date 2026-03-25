# DX Polish Follow-up — 5 UX Improvements

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the 5 remaining UX friction points found by the editor install review.

**Architecture:** 3 parallel batches. Batch A: VS Code TypeScript changes. Batch B: Rust init_ai.rs template changes. Batch C: model name unification across all files.

**Baseline:** 8,236 tests passing, 0 failures.

---

## Task 1: VS Code — Add menu entries for Run/Check commands

**Files:**
- Modify: `editors/vscode/package.json`

Add `menus` contribution so Run and Check buttons appear in the editor title bar and context menu for .nika.yaml files. Use `when` clause with `resourceFilename` regex.

---

## Task 2: VS Code — Proactive "nika not found" check at activation

**Files:**
- Modify: `editors/vscode/src/extension.ts`

Add a binary check at activation time using `execFile` (not exec — avoid shell injection). Show a warning banner if nika is not found, with an "Install Instructions" button linking to the GitHub repo.

---

## Task 3: Init — Generate .vscode/settings.json with nika language association

**Files:**
- Modify: `tools/nika-cli/src/init_ai.rs`

Add a VSCODE_SETTINGS constant with the nika language ID association and generate it via write_if_absent_with_dir in generate_ai_files().

---

## Task 4: Cursor — Generate architecture and security rules (parity)

**Files:**
- Modify: `tools/nika-cli/src/init_ai.rs`

Add architecture and security Cursor rules distilled from the Claude rules. Generate them alongside existing syntax+patterns rules.

---

## Task 5: Model name consistency — standardize on claude-sonnet-4-20250514

**Files:**
- Modify: `.claude/rules/nika-workflows.md`

User-facing docs should use the full versioned name for clarity.

---
