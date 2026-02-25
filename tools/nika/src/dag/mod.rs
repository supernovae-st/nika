//! DAG Module - Directed Acyclic Graph structure (v0.1)
//!
//! Contains the DAG representation and validation:
//! - `flow`: Dag built from workflow flows
//! - `stable`: StableDag wrapper for petgraph::StableGraph (v0.9.0)
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
pub use validate::validate_use_wiring;
