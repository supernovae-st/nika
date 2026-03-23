# LSP Migration: References + Document Links + Folding Ranges

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate 3 fully-implemented LSP features from nika-engine embedded LSP to nika-lsp-core (protocol-agnostic) + wire in nika-lsp backend.

**Architecture:** Extract pure text-scanning logic from nika-engine handlers, create protocol-agnostic types in nika-lsp-core, wire to tower-lsp in nika-lsp/backend.rs. No AST dependency — all 3 work on raw text.

**Tech Stack:** Rust, nika-lsp-core (pure handlers), nika-lsp (tower-lsp wiring)

---

## Task 1: References handler (find all usages of a task ID)

**Source:** `nika-engine/src/lsp/handlers/references.rs` (828 lines)
**Target:** `nika-lsp-core/src/handlers/references.rs` (~170 lines)

Create protocol-agnostic version with:
- `find_task_at_offset(text: &str, offset: u32) -> Option<String>` — finds task ID at cursor
- `find_task_references(text: &str, task_id: &str) -> Vec<ReferenceEntry>` — all usages
- `ReferenceEntry { start_offset: u32, end_offset: u32 }`
- Scans: `- id:` definitions, `depends_on:` (inline/scalar/multiline), `with:` bindings (`$task_id`), templates (`{{with.alias}}` resolved through alias map)

Register in mod.rs + handler.rs trait. Add e2e tests. Wire in backend.rs with `references_provider` capability.

## Task 2: Document Links handler (clickable paths)

**Source:** `nika-engine/src/lsp/handlers/document_links.rs` (453 lines)
**Target:** `nika-lsp-core/src/handlers/document_links.rs` (~165 lines)

Create protocol-agnostic version with:
- `document_links(text: &str) -> Vec<LinkEntry>`
- `LinkEntry { start_offset: u32, end_offset: u32, target: String, tooltip: String }`
- Tracks section context (Context/Skills/None)
- Detects: `url:` HTTP links, `path:`/`include:` file paths, section file references

Register in mod.rs + handler.rs. Add e2e tests. Wire in backend.rs with `document_link_provider`.

## Task 3: Folding Ranges handler (collapsible blocks)

**Source:** `nika-engine/src/lsp/handlers/folding_ranges.rs` (585 lines)
**Target:** `nika-lsp-core/src/handlers/folding_ranges.rs` (~220 lines)

Create protocol-agnostic version with:
- `folding_ranges(text: &str) -> Vec<FoldEntry>`
- `FoldEntry { start_line: u32, end_line: u32, kind: FoldKind }`
- `enum FoldKind { Region, Comment }`
- 3-pass: indent-based blocks, multi-line strings (`|`/`>`), comment groups

Register in mod.rs + handler.rs. Add e2e tests. Wire in backend.rs with `folding_range_provider`.

## Task 4: Wire all 3 in backend.rs + final verification

Add 3 capabilities to ServerCapabilities, 3 handler methods, cargo test + clippy.
