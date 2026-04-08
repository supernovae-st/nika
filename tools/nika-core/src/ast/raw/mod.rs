// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw AST - Phase 1 of the Two-Phase IR Architecture.
//!
//! This module contains AST types that are directly parsed from YAML with
//! full span tracking. All string values are preserved as-is, and all nodes
//! carry their source location for precise error reporting.
//!
//! # Two-Phase Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  YAML Source    │  workflow.nika.yaml
//! └────────┬────────┘
//!          │ marked_yaml
//!          ▼
//! ┌─────────────────┐
//! │  raw::Workflow  │  ← THIS MODULE
//! │  ├── Spanned<T> │  All nodes have line:col
//! │  └── strings    │  Unresolved references
//! └────────┬────────┘
//!          │ analyze()
//!          ▼
//! ┌─────────────────┐
//! │ analyzed::Workflow │  (see ast/analyzed/)
//! │  ├── TaskId(u32)   │  Interned identifiers
//! │  └── resolved refs │  All references validated
//! └─────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use nika::ast::raw;
//! use nika::source::SourceRegistry;
//!
//! let mut sources = SourceRegistry::new();
//! let file_id = sources.add_file("workflow.yaml", content);
//!
//! // Phase 1: Parse to raw AST
//! let raw_workflow = raw::parse(&content, file_id)?;
//!
//! // Phase 2: Analyze to resolved AST
//! let analyzed = analyze(&sources, raw_workflow)?;
//! ```
//!
//! # Design Principles
//!
//! 1. **Every node has a span** - For precise error locations
//! 2. **Strings are unresolved** - Task IDs are strings, not interned
//! 3. **No validation** - Structure only, semantics checked in Phase 2
//! 4. **Preserves YAML order** - Uses indexmap for key ordering

mod action;
mod mcp;
mod parser;
mod task;
mod workflow;

pub use action::{
    RawAgentAction, RawExecAction, RawFetchAction, RawInferAction, RawInvokeAction, RawTaskAction,
};
pub use mcp::{RawMcpConfig, RawMcpServer};
pub use parser::{parse, ParseError, ParseErrorKind};
pub use task::{RawForEach, RawOutputConfig, RawRetryConfig, RawTask};
pub use workflow::{RawContextConfig, RawIncludeSpec, RawPkgConfig, RawWorkflow};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure() {
        // Basic smoke test that all types are accessible
        let _ = RawWorkflow::default();
        let _ = RawTask::default();
        let _ = RawMcpConfig::default();
    }
}
