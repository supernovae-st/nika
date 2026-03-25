//! Chat-specific widgets
//!
//! Widgets used exclusively by ChatView — DAG visualization and task queue.
//! Some widget API methods are staged for future features (selection, animation).

// Widget APIs include getters/builder methods staged for future features
#[allow(dead_code)]
mod dag_panel;
#[allow(dead_code)]
mod edge_line;
#[allow(dead_code)]
mod node_box;
#[allow(dead_code)]
mod task_queue;

pub use dag_panel::{ChatDagPanel, DagEdgeData, DagNodeData};
pub use node_box::{ChatNodeKind, ChatNodeState};
pub use task_queue::{ChatTaskQueue, ChatTaskQueueItem, ChatTaskState, ChatTaskVerb};
