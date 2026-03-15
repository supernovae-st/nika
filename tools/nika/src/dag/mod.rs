//! DAG Module - Directed Acyclic Graph structure
//!
//! Contains the DAG representation and validation:
//! - `flow`: Dag built from workflow flows (HashMap-based, legacy)
//! - `indexed`: IndexedDag with Vec adjacency and Kahn's algorithm
//! - `stable`: StableDag wrapper for petgraph::StableGraph (TUI)
//! - `validate`: DAG validation for with: bindings
//!
//! The DAG represents task dependencies and execution order.
//! Dag is immutable after construction (architectural decision #2).

mod flow;
pub mod indexed;
mod stable;
mod validate;

// Re-export public types
pub use flow::Dag;
pub use indexed::IndexedDag;
pub use stable::{DagEdge, StableDag};
pub use validate::{validate_bindings, validate_with_bindings};
