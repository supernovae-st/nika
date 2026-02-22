//! DAG Module - Directed Acyclic Graph structure (v0.1)
//!
//! Contains the DAG representation and validation:
//! - `flow`: FlowGraph built from workflow flows
//! - `validate`: DAG validation for use: bindings
//!
//! The DAG represents task dependencies and execution order.
//! FlowGraph is immutable after construction (architectural decision #2).

mod flow;
mod validate;

// Re-export public types
pub use flow::FlowGraph;
pub use validate::validate_use_wiring;
