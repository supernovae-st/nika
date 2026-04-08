// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat-specific widgets
//!
//! Widgets used exclusively by ChatView — DAG visualization and task queue.

mod dag_panel;
mod edge_line;
mod node_box;
mod task_queue;

pub use dag_panel::{ChatDagPanel, DagEdgeData, DagNodeData};
pub use node_box::{ChatNodeKind, ChatNodeState};
pub use task_queue::{ChatTaskQueue, ChatTaskQueueItem, ChatTaskState, ChatTaskVerb};
