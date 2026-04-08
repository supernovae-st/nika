// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source file tracking and span management.
//!
//! This module provides the foundation for error reporting with precise
//! source locations. It's used by the raw AST phase to track where every
//! parsed value came from.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  SourceRegistry                                                     │
//! │  ├── SourceFile 0 (main.nika.yaml)                                 │
//! │  │   ├── content: "schema: ..."                                    │
//! │  │   └── line_starts: [0, 29, 44, ...]                             │
//! │  ├── SourceFile 1 (included.nika.yaml)                             │
//! │  └── ...                                                           │
//! └─────────────────────────────────────────────────────────────────────┘
//!
//! Span { file: FileId(0), start: 29, end: 44 }
//!   → "workflow: test"
//!   → workflow.yaml:2:1
//! ```
//!
//! # Example
//!
//! ```
//! use nika_core::source::{SourceRegistry, Span, FileId, Spanned};
//!
//! let mut registry = SourceRegistry::new();
//! let file_id = registry.add_file("workflow.yaml", "schema: \"nika/workflow@0.12\"\nworkflow: test".to_string());
//!
//! // Create a span pointing to "workflow"
//! let span = Span::new(file_id, 29, 37);
//!
//! // Get location string
//! assert_eq!(registry.format_location(span), "workflow.yaml:2:1");
//!
//! // Get text at span
//! assert_eq!(registry.text_at(span), "workflow");
//! ```

mod registry;
mod span;

pub use registry::{SourceFile, SourceRegistry, SourceSnippet};
pub use span::{ByteOffset, FileId, Span, Spanned};
