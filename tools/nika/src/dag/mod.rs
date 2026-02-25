//! DAG Module - Directed Acyclic Graph structure (v0.1)
//!
//! Contains the DAG representation and validation:
//! - `flow`: FlowGraph built from workflow flows
//! - `stable`: StableFlowGraph wrapper for petgraph::StableGraph (v0.9.0)
//! - `validate`: DAG validation for use: bindings
//!
//! The DAG represents task dependencies and execution order.
//! FlowGraph is immutable after construction (architectural decision #2).

mod flow;
mod stable;
mod validate;

// Re-export public types
pub use flow::FlowGraph;
pub use stable::{FlowEdge, StableFlowGraph};
pub use validate::validate_use_wiring;
