//! DAG Module - Directed Acyclic Graph structure
//!
//! Contains the DAG representation and validation:
//! - `flow`: Dag built from workflow flows
//! - `stable`: StableDag wrapper for petgraph::StableGraph
//! - `validate`: DAG validation for use: bindings
//!
//! The DAG represents task dependencies and execution order.
//! Dag is immutable after construction (architectural decision #2).

mod flow;
mod stable;
mod validate;

// Re-export public types
pub use flow::Dag;
pub use stable::{DagEdge, StableDag};
pub use validate::{validate_use_wiring, validate_with_bindings};
