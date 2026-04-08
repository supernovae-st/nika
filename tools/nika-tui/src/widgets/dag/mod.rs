// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Visualization Subsystem
//!
//! Renders directed acyclic graphs of workflow tasks with nodes, edges,
//! bindings, and data previews using Sugiyama-style hierarchical layout.
//!
//! # Public API
//!
//! - [`DagAscii`] — Complete DAG widget (compose with nodes + deps)
//! - [`NodeBox`] — Individual node renderer
//! - [`NodeBoxData`] — Node data (id, verb, status, etc.)
//! - [`NodeBoxMode`] — Display mode (minimal/expanded)

mod ascii;
mod edge;
mod layout;
mod node_box;
mod node_data;

// Public API: only expose what views need
pub use ascii::DagAscii;
pub use node_box::NodeBox;
pub use node_data::{NodeBoxData, NodeBoxMode};
