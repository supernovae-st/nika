//! Mock Runtime - For testing and demo purposes
//!
//! Generates fake events to demonstrate TUI functionality.

use async_trait::async_trait;
use std::path::Path;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use super::{FlowInfo, RuntimeBridge, RuntimeEvent, TaskInfo, WorkflowInfo};
use crate::tui::state::Paradigm;

/// Mock runtime that generates fake events for demo/testing
pub struct MockRuntime {
    tx: mpsc::Sender<RuntimeEvent>,
    workflow: Option<WorkflowInfo>,
}

impl MockRuntime {
    pub fn new() -> Self {
        let (tx, _rx) = mpsc::channel(100);
        Self { tx, workflow: None }
    }

    /// Generate a demo workflow
    fn demo_workflow() -> WorkflowInfo {
        WorkflowInfo {
            name: "demo-workflow".to_string(),
            path: "demo.nika.yaml".to_string(),
            task_count: 4,
            flow_count: 3,
            tasks: vec![
                TaskInfo {
                    id: "analyze".to_string(),
                    task_type: "nika/analyze".to_string(),
                    paradigm: Paradigm::Context,
                },
                TaskInfo {
                    id: "generate".to_string(),
                    task_type: "nika/generate".to_string(),
                    paradigm: Paradigm::Context,
                },
                TaskInfo {
                    id: "transform".to_string(),
                    task_type: "nika/transform".to_string(),
                    paradigm: Paradigm::Pure,
                },
                TaskInfo {
                    id: "review".to_string(),
                    task_type: "nika/code".to_string(),
                    paradigm: Paradigm::Isolated,
                },
            ],
            flows: vec![
                FlowInfo {
                    source: "analyze".to_string(),
                    target: "generate".to_string(),
                },
                FlowInfo {
                    source: "generate".to_string(),
                    target: "transform".to_string(),
                },
                FlowInfo {
                    source: "transform".to_string(),
                    target: "review".to_string(),
                },
            ],
        }
    }

    /// Start generating mock events
    async fn generate_events(tx: mpsc::Sender<RuntimeEvent>, workflow: WorkflowInfo) {
        // Initial connections
        let _ = tx
            .send(RuntimeEvent::McpConnected {
                server: "filesystem".to_string(),
                tools: vec!["read".to_string(), "write".to_string()],
            })
            .await;

        time::sleep(Duration::from_millis(200)).await;

        let _ = tx
            .send(RuntimeEvent::McpConnected {
                server: "github".to_string(),
                tools: vec!["search".to_string(), "pr".to_string()],
            })
            .await;

        time::sleep(Duration::from_millis(200)).await;

        let _ = tx
            .send(RuntimeEvent::SkillLoaded {
                name: "code-review".to_string(),
            })
            .await;

        time::sleep(Duration::from_millis(100)).await;

        // Workflow start
        let _ = tx
            .send(RuntimeEvent::WorkflowStarted {
                id: "wf-001".to_string(),
                name: workflow.name.clone(),
            })
            .await;

        time::sleep(Duration::from_millis(300)).await;

        // Process each task
        let mut total_tokens: u32 = 0;
        for (i, task) in workflow.tasks.iter().enumerate() {
            // Task start
            let _ = tx
                .send(RuntimeEvent::TaskStarted {
                    id: task.id.clone(),
                    paradigm: task.paradigm,
                })
                .await;

            // Spawn agent for context/isolated tasks
            if task.paradigm == Paradigm::Context || task.paradigm == Paradigm::Isolated {
                let _ = tx
                    .send(RuntimeEvent::AgentSpawned {
                        id: format!("agent-{}", i),
                        name: task.id.clone(),
                        paradigm: task.paradigm,
                    })
                    .await;

                time::sleep(Duration::from_millis(200)).await;

                // Thinking
                let _ = tx
                    .send(RuntimeEvent::AgentThinking {
                        id: format!("agent-{}", i),
                    })
                    .await;

                time::sleep(Duration::from_millis(500)).await;

                // Some messages
                let _ = tx
                    .send(RuntimeEvent::AgentMessage {
                        id: format!("agent-{}", i),
                        content: format!("Processing {} task...", task.id),
                    })
                    .await;

                time::sleep(Duration::from_millis(300)).await;

                // Tool use
                let _ = tx
                    .send(RuntimeEvent::AgentToolUse {
                        id: format!("agent-{}", i),
                        tool: "read".to_string(),
                    })
                    .await;

                time::sleep(Duration::from_millis(400)).await;

                // Token usage
                let tokens = 1500 + (i as u32 * 500);
                total_tokens += tokens;
                let _ = tx
                    .send(RuntimeEvent::TokensUsed {
                        input: tokens / 3,
                        output: tokens * 2 / 3,
                        total: total_tokens,
                        cost: total_tokens as f64 * 0.00001,
                    })
                    .await;

                // Agent terminated
                let _ = tx
                    .send(RuntimeEvent::AgentTerminated {
                        id: format!("agent-{}", i),
                    })
                    .await;
            }

            // Progress updates
            for p in [25, 50, 75, 100] {
                let _ = tx
                    .send(RuntimeEvent::TaskProgress {
                        id: task.id.clone(),
                        progress: p as f32,
                    })
                    .await;
                time::sleep(Duration::from_millis(150)).await;
            }

            // Task complete
            let _ = tx
                .send(RuntimeEvent::TaskCompleted {
                    id: task.id.clone(),
                    output: Some(format!("Output from {}", task.id)),
                })
                .await;

            time::sleep(Duration::from_millis(200)).await;
        }

        // Context summarization event (simulate)
        let _ = tx
            .send(RuntimeEvent::ContextSummarized {
                before: total_tokens,
                after: total_tokens / 3,
            })
            .await;

        time::sleep(Duration::from_millis(300)).await;

        // Workflow complete
        let _ = tx
            .send(RuntimeEvent::WorkflowCompleted {
                duration_ms: 5000,
                tasks_completed: workflow.task_count,
                total_tokens,
            })
            .await;
    }
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeBridge for MockRuntime {
    async fn load_workflow(&self, path: &Path) -> anyhow::Result<WorkflowInfo> {
        // For mock, just return demo workflow
        let mut workflow = Self::demo_workflow();
        workflow.path = path.to_string_lossy().to_string();
        Ok(workflow)
    }

    async fn start(&self) -> anyhow::Result<()> {
        let tx = self.tx.clone();
        let workflow = self.workflow.clone().unwrap_or_else(Self::demo_workflow);

        // Spawn event generation in background
        tokio::spawn(async move {
            Self::generate_events(tx, workflow).await;
        });

        Ok(())
    }

    async fn pause(&self) -> anyhow::Result<()> {
        let _ = self.tx.send(RuntimeEvent::WorkflowPaused).await;
        Ok(())
    }

    async fn resume(&self) -> anyhow::Result<()> {
        let _ = self.tx.send(RuntimeEvent::WorkflowResumed).await;
        Ok(())
    }

    async fn abort(&self) -> anyhow::Result<()> {
        let _ = self
            .tx
            .send(RuntimeEvent::WorkflowError {
                error: "Aborted by user".to_string(),
            })
            .await;
        Ok(())
    }

    fn events(&self) -> Box<dyn Stream<Item = RuntimeEvent> + Send + Unpin> {
        // This is a bit awkward - in real impl we'd use a broadcast channel
        // For now, create a new channel for each call
        let (_tx, rx) = mpsc::channel(100);

        // Clone our sender to forward events
        let main_tx = self.tx.clone();
        tokio::spawn(async move {
            // In a real impl, we'd subscribe to events
            // For mock, we just forward from our main channel
            drop(main_tx);
        });

        Box::new(ReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_runtime_load() {
        let runtime = MockRuntime::new();
        let info = runtime.load_workflow(Path::new("test.nika.yaml")).await;
        assert!(info.is_ok());
        let info = info.unwrap();
        assert_eq!(info.task_count, 4);
    }

    #[test]
    fn test_demo_workflow() {
        let workflow = MockRuntime::demo_workflow();
        assert_eq!(workflow.tasks.len(), 4);
        assert_eq!(workflow.flows.len(), 3);
    }
}
