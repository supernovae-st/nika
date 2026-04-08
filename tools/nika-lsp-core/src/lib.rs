// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Protocol-agnostic LSP intelligence for Nika workflows.
//!
//! This crate provides the shared intelligence layer used by both the embedded
//! LSP (`nika lsp`) and the standalone LSP (`nika-lsp`). All handlers are pure
//! functions: `(text, offset, context) → Result` — no async, no server state.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │  nika-lsp-core                                   │
//! │                                                  │
//! │  analysis::context  → CursorContext (16 variants)│
//! │  handlers::*        → completion, hover, def, ...│
//! │  parse::*           → tree-sitter error recovery │
//! │  handler::*         → LspHandler trait           │
//! │  db::*              → WorldDatabase              │
//! │  position::*        → LineIndex, PositionIndex   │
//! └─────────────────────────────────────────────────┘
//!        ▲                          ▲
//!        │                          │
//!   nika (embedded)           nika-lsp (standalone)
//! ```
//!
//! # Key Types
//!
//! - [`analysis::context::CursorContext`] — 16-variant enum for cursor position semantics
//! - [`handler::LspHandler`] — trait for protocol-agnostic handler dispatch
//! - [`handler::DefaultHandler`] — default implementation wiring pure handlers
//! - [`parse::parse_and_extract`] — error-recovery parsing via tree-sitter
//! - [`parse::PartialWorkflow`] — structural info from broken YAML
//!
//! # Test Coverage
//!
//! 745+ tests across 10 test suites (lib + 9 E2E integration test files).

pub mod analysis;
pub mod db;
pub mod document;
pub mod handler;
pub mod handlers;
pub mod parse;
pub mod position;
