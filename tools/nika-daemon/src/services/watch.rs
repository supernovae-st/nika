// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Watch service — file watcher with debounce.
//!
//! Watches a directory for `.nika.yaml` file changes, debounces rapid edits,
//! and optionally submits jobs for modified workflows.
//!
//! Uses `notify` crate for cross-platform filesystem events
//! (FSEvents on macOS, inotify on Linux). Manual debounce via tokio
//! to avoid API compatibility issues with notify-debouncer-full.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, trace, warn};

use crate::error::{DaemonError, DaemonResult};

/// Default debounce duration (500ms — accommodates macOS FSEvents batching).
const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// Watch event: a workflow file changed.
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub kind: WatchEventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WatchEventKind {
    Modified,
    Created,
    Removed,
}

/// Configuration for the watch service.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub dir: PathBuf,
    pub patterns: Vec<String>,
    pub debounce: Duration,
    pub recursive: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("."),
            patterns: vec!["*.nika.yaml".to_string()],
            debounce: DEFAULT_DEBOUNCE,
            recursive: true,
        }
    }
}

/// The watch service — wraps notify watcher with tokio channel bridge.
pub struct WatchService {
    config: WatchConfig,
    event_rx: mpsc::Receiver<WatchEvent>,
    shutdown_rx: watch::Receiver<bool>,
    _watcher: RecommendedWatcher,
}

impl WatchService {
    /// Start watching a directory.
    pub fn start(config: WatchConfig, shutdown_rx: watch::Receiver<bool>) -> DaemonResult<Self> {
        let (raw_tx, mut raw_rx) = mpsc::channel::<Event>(512);
        let (event_tx, event_rx) = mpsc::channel::<WatchEvent>(256);

        let glob_set = build_glob_set(&config.patterns)?;
        let debounce = config.debounce;

        // Create watcher with channel bridge (notify → tokio mpsc)
        let tx = raw_tx;
        let mut watcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    // try_send: drop events rather than blocking the notify thread on a full channel
                    if tx.try_send(event).is_err() {
                        warn!("watch event dropped — raw channel full");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "watch error");
                }
            })
            .map_err(|e| DaemonError::Lifecycle(format!("create watcher: {e}")))?;

        let mode = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher
            .watch(config.dir.as_ref(), mode)
            .map_err(|e| DaemonError::Lifecycle(format!("watch {}: {e}", config.dir.display())))?;

        info!(
            dir = %config.dir.display(),
            patterns = ?config.patterns,
            "watching for changes"
        );

        // Spawn debounce + filter task
        tokio::spawn(async move {
            let mut last_seen = std::collections::HashMap::<PathBuf, tokio::time::Instant>::new();
            let mut prune_counter = 0u32;

            while let Some(event) = raw_rx.recv().await {
                // H5 fix: prune stale entries every 100 events to prevent unbounded growth
                prune_counter += 1;
                if prune_counter % 100 == 0 {
                    let now = tokio::time::Instant::now();
                    last_seen.retain(|_, t| now.duration_since(*t) < debounce * 10);
                }
                for path in event.paths {
                    // Filter by glob
                    if let Some(filename) = path.file_name() {
                        if !glob_set.is_match(filename) {
                            trace!(path = %path.display(), "skipped (no glob match)");
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Debounce: skip if we saw this path recently
                    let now = tokio::time::Instant::now();
                    if let Some(last) = last_seen.get(&path) {
                        if now.duration_since(*last) < debounce {
                            trace!(path = %path.display(), "debounced");
                            continue;
                        }
                    }
                    last_seen.insert(path.clone(), now);

                    let kind = match event.kind {
                        notify::EventKind::Create(_) => WatchEventKind::Created,
                        notify::EventKind::Remove(_) => WatchEventKind::Removed,
                        _ => WatchEventKind::Modified,
                    };

                    debug!(path = %path.display(), ?kind, "watch event");
                    if event_tx
                        .send(WatchEvent {
                            path: path.clone(),
                            kind,
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });

        Ok(Self {
            config,
            event_rx,
            shutdown_rx,
            _watcher: watcher,
        })
    }

    /// Receive the next watch event.
    pub async fn next_event(&mut self) -> Option<WatchEvent> {
        tokio::select! {
            biased;
            _ = self.shutdown_rx.changed() => {
                if *self.shutdown_rx.borrow() {
                    return None;
                }
                self.event_rx.recv().await
            }
            event = self.event_rx.recv() => event,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.config.dir
    }

    pub fn patterns(&self) -> &[String] {
        &self.config.patterns
    }
}

/// Maximum number of glob patterns per watch (prevents ReDoS on GlobSet compile).
const MAX_GLOB_PATTERNS: usize = 32;

fn build_glob_set(patterns: &[String]) -> DaemonResult<globset::GlobSet> {
    if patterns.len() > MAX_GLOB_PATTERNS {
        return Err(DaemonError::Protocol(format!(
            "too many glob patterns: {} > {MAX_GLOB_PATTERNS} limit",
            patterns.len()
        )));
    }
    let mut builder = globset::GlobSet::builder();
    for pattern in patterns {
        let glob = globset::Glob::new(pattern)
            .map_err(|e| DaemonError::Protocol(format!("invalid glob {pattern}: {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| DaemonError::Protocol(format!("build glob set: {e}")))
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = WatchConfig::default();
        assert_eq!(config.patterns, vec!["*.nika.yaml"]);
        assert_eq!(config.debounce, Duration::from_millis(500));
        assert!(config.recursive);
    }

    #[test]
    fn glob_set_matches_nika_yaml() {
        let set = build_glob_set(&["*.nika.yaml".to_string()]).unwrap();
        assert!(set.is_match("test.nika.yaml"));
        assert!(set.is_match("my-workflow.nika.yaml"));
        assert!(!set.is_match("test.yaml"));
        assert!(!set.is_match("test.nika.yml"));
    }

    #[test]
    fn glob_set_multiple_patterns() {
        let set = build_glob_set(&["*.nika.yaml".to_string(), "*.nika.yml".to_string()]).unwrap();
        assert!(set.is_match("test.nika.yaml"));
        assert!(set.is_match("test.nika.yml"));
        assert!(!set.is_match("test.yaml"));
    }

    #[test]
    fn glob_set_invalid_pattern_errors() {
        let result = build_glob_set(&["[invalid".to_string()]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn watch_detects_file_change() {
        let dir = tempfile::tempdir().unwrap();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = WatchConfig {
            dir: dir.path().to_path_buf(),
            patterns: vec!["*.nika.yaml".to_string()],
            debounce: Duration::from_millis(50),
            recursive: false,
        };

        let mut svc = WatchService::start(config, shutdown_rx).unwrap();

        // Create a .nika.yaml file
        tokio::fs::write(
            dir.path().join("test.nika.yaml"),
            "schema: 'nika/workflow@0.12'",
        )
        .await
        .unwrap();

        // Wait for event
        let event = tokio::time::timeout(Duration::from_secs(3), svc.next_event())
            .await
            .unwrap();

        assert!(event.is_some());
        let event = event.unwrap();
        assert!(event.path.ends_with("test.nika.yaml"));
    }

    #[tokio::test]
    async fn watch_ignores_non_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = WatchConfig {
            dir: dir.path().to_path_buf(),
            patterns: vec!["*.nika.yaml".to_string()],
            debounce: Duration::from_millis(50),
            recursive: false,
        };

        let mut svc = WatchService::start(config, shutdown_rx).unwrap();

        tokio::fs::write(dir.path().join("readme.md"), "# Hello")
            .await
            .unwrap();

        let result = tokio::time::timeout(Duration::from_millis(500), svc.next_event()).await;
        assert!(result.is_err()); // Timeout = no event
    }

    #[tokio::test]
    async fn watch_shutdown_stops_service() {
        let dir = tempfile::tempdir().unwrap();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = WatchConfig {
            dir: dir.path().to_path_buf(),
            ..WatchConfig::default()
        };

        let mut svc = WatchService::start(config, shutdown_rx).unwrap();

        shutdown_tx.send(true).unwrap();

        let event = tokio::time::timeout(Duration::from_millis(500), svc.next_event())
            .await
            .unwrap();

        assert!(event.is_none());
    }
}
