//! TUI Application State
//!
//! Manages the main application loop and state.

use std::path::Path;

use crate::error::{NikaError, Result};

/// Main TUI application
pub struct App {
    /// Path to the workflow being observed
    workflow_path: std::path::PathBuf,
}

impl App {
    /// Create a new TUI application for the given workflow
    pub fn new(workflow_path: &Path) -> Result<Self> {
        if !workflow_path.exists() {
            return Err(NikaError::WorkflowNotFound {
                path: workflow_path.display().to_string(),
            });
        }

        Ok(Self {
            workflow_path: workflow_path.to_path_buf(),
        })
    }

    /// Run the TUI application
    pub async fn run(self) -> Result<()> {
        // TODO: Implement TUI event loop
        tracing::info!("TUI started for workflow: {}", self.workflow_path.display());
        Ok(())
    }
}
