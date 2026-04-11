// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Module - Directed Acyclic Graph structure
//!
//! Contains the DAG representation and validation:
//! - `flow`: Dag built from depends_on edges (HashMap-based)
//! - `stable`: StableDag wrapper for petgraph::StableGraph (TUI)
//! - `validate`: DAG validation for with: bindings
//!
//! The DAG represents task dependencies and execution order.
//! Dag is immutable after construction (architectural decision #2).

pub mod flow;
mod stable;
mod validate;

// Re-export public types
pub use flow::Dag;
pub use stable::{DagEdge, StableDag};
pub use validate::{validate_bindings, validate_with_bindings};
