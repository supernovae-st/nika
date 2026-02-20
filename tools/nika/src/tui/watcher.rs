//! File Watcher Module
//!
//! Watches for changes to `.nika.yaml` workflow files and notifies the TUI.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  FileWatcher                                                     │
//! │  ├── notify::RecommendedWatcher (cross-platform)                │
//! │  └── tokio::sync::mpsc::Sender (async channel)                  │
//! │                                                                  │
//! │  Directory → Watcher → Filter → Event → Channel → BrowserView   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc as async_mpsc;

/// File change event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEvent {
    /// A workflow file was created
    Created(PathBuf),
    /// A workflow file was modified
    Modified(PathBuf),
    /// A workflow file was removed
    Removed(PathBuf),
    /// A workflow file was renamed (old path, new path)
    Renamed(PathBuf, PathBuf),
}

impl FileEvent {
    /// Get the path affected by this event
    pub fn path(&self) -> &Path {
        match self {
            FileEvent::Created(p) | FileEvent::Modified(p) | FileEvent::Removed(p) => p,
            FileEvent::Renamed(_, new) => new,
        }
    }

    /// Check if this event affects a workflow file
    pub fn is_workflow_file(&self) -> bool {
        let path = self.path();
        path.extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".nika"))
    }
}

/// File watcher for detecting workflow file changes
pub struct FileWatcher {
    /// The underlying notify watcher
    _watcher: RecommendedWatcher,
    /// Receiver for file events
    event_rx: async_mpsc::Receiver<FileEvent>,
    /// Root directory being watched
    root: PathBuf,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory
    ///
    /// # Arguments
    ///
    /// * `root` - Directory to watch (recursively)
    ///
    /// # Returns
    ///
    /// A `FileWatcher` instance or an error if the watcher couldn't be created
    pub fn new(root: PathBuf) -> Result<Self, notify::Error> {
        // Create async channel for file events (bounded to prevent memory issues)
        let (event_tx, event_rx) = async_mpsc::channel(100);

        // Create sync channel for notify (it uses sync callbacks)
        let (sync_tx, sync_rx) = mpsc::channel::<Event>();

        // Create the watcher with a reasonable debounce interval
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                if let Ok(event) = result {
                    // Send to sync channel, ignore errors (receiver may be gone)
                    let _ = sync_tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )?;

        // Start watching the root directory recursively
        watcher.watch(&root, RecursiveMode::Recursive)?;

        // Spawn a blocking task to convert sync events to async events
        let root_clone = root.clone();
        tokio::spawn(async move {
            loop {
                // Check for events with a timeout to not block forever
                match sync_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(event) => {
                        // Convert notify event to our FileEvent
                        if let Some(file_event) = Self::convert_event(&event, &root_clone) {
                            // Only send workflow file events
                            if file_event.is_workflow_file() {
                                // Send to async channel, break if receiver is gone
                                if event_tx.send(file_event).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // Check if channel is still open
                        if event_tx.is_closed() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            _watcher: watcher,
            event_rx,
            root,
        })
    }

    /// Convert a notify event to our FileEvent type
    fn convert_event(event: &Event, _root: &Path) -> Option<FileEvent> {
        // Only process events with paths
        if event.paths.is_empty() {
            return None;
        }

        let path = event.paths[0].clone();

        match &event.kind {
            EventKind::Create(_) => Some(FileEvent::Created(path)),
            EventKind::Modify(_) => Some(FileEvent::Modified(path)),
            EventKind::Remove(_) => Some(FileEvent::Removed(path)),
            EventKind::Other => None,
            EventKind::Access(_) => None, // Ignore access events
            EventKind::Any => None,
        }
    }

    /// Try to receive a file event without blocking
    ///
    /// Returns `Some(FileEvent)` if an event is available, `None` otherwise
    pub fn try_recv(&mut self) -> Option<FileEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Receive a file event, waiting up to the specified timeout
    ///
    /// Returns `Some(FileEvent)` if an event is received, `None` on timeout
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Option<FileEvent> {
        tokio::time::timeout(timeout, self.event_rx.recv())
            .await
            .ok()
            .flatten()
    }

    /// Get the root directory being watched
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_event_is_workflow_file() {
        // Valid workflow files
        assert!(FileEvent::Created(PathBuf::from("test.nika.yaml")).is_workflow_file());
        assert!(FileEvent::Modified(PathBuf::from("workflow.nika.yml")).is_workflow_file());
        assert!(FileEvent::Removed(PathBuf::from("/path/to/example.nika.yaml")).is_workflow_file());

        // Invalid files (not .nika.yaml)
        assert!(!FileEvent::Created(PathBuf::from("test.yaml")).is_workflow_file());
        assert!(!FileEvent::Modified(PathBuf::from("workflow.yml")).is_workflow_file());
        assert!(!FileEvent::Removed(PathBuf::from("config.toml")).is_workflow_file());
    }

    #[test]
    fn test_file_event_path() {
        let path = PathBuf::from("test.nika.yaml");

        assert_eq!(FileEvent::Created(path.clone()).path(), &path);
        assert_eq!(FileEvent::Modified(path.clone()).path(), &path);
        assert_eq!(FileEvent::Removed(path.clone()).path(), &path);

        let old_path = PathBuf::from("old.nika.yaml");
        let new_path = PathBuf::from("new.nika.yaml");
        assert_eq!(
            FileEvent::Renamed(old_path, new_path.clone()).path(),
            &new_path
        );
    }

    #[test]
    fn test_file_event_equality() {
        let path = PathBuf::from("test.nika.yaml");

        assert_eq!(
            FileEvent::Created(path.clone()),
            FileEvent::Created(path.clone())
        );
        assert_ne!(
            FileEvent::Created(path.clone()),
            FileEvent::Modified(path.clone())
        );
    }
}
