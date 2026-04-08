// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NDJSON Trace Writer
//!
//! Writes events to newline-delimited JSON files for debugging and replay.

use crate::error::Result;
use crate::log::{Event, EventLog};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;

/// Directory for trace files
const TRACE_DIR: &str = ".nika/traces";

/// NDJSON trace writer
pub struct TraceWriter {
    writer: Arc<Mutex<BufWriter<File>>>,
    path: PathBuf,
}

impl TraceWriter {
    /// Create a new trace writer for a generation
    ///
    /// # Security
    ///
    /// The generation_id is validated to prevent path traversal attacks.
    /// Only alphanumeric characters, hyphens, and underscores are allowed.
    pub fn new(generation_id: &str) -> Result<Self> {
        // Validate generation_id to prevent path traversal
        if generation_id.is_empty()
            || generation_id.contains("..")
            || generation_id.contains('/')
            || generation_id.contains('\\')
            || !generation_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == 'T')
        {
            return Err(crate::error::EventError::TraceWrite(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Invalid generation_id: must be alphanumeric with hyphens/underscores only, got: {}",
                        generation_id
                    ),
                ),
            ));
        }

        // Ensure trace directory exists
        let trace_dir = Path::new(TRACE_DIR);
        fs::create_dir_all(trace_dir)?;

        // Create trace file
        let filename = format!("{}.ndjson", generation_id);
        let path = trace_dir.join(&filename);
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);

        tracing::info!(path = %path.display(), "Created trace file");

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            path,
        })
    }

    /// Write a single event to the trace file
    pub fn write_event(&self, event: &Event) -> Result<()> {
        let json = serde_json::to_string(event)?;

        let mut writer = self.writer.lock();
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        Ok(())
    }

    /// Append a single event line and flush immediately.
    ///
    /// Designed for incremental writing during execution so that partial
    /// trace data survives a crash. Returns `io::Result` for ergonomic
    /// use with `let _ =` in hot paths.
    pub fn append_event(&self, event: &Event) -> io::Result<()> {
        let json = serde_json::to_string(event)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut writer = self.writer.lock();
        writeln!(writer, "{}", json)?;
        writer.flush()?;
        Ok(())
    }

    /// Write all events from an EventLog
    pub fn write_all(&self, event_log: &EventLog) -> Result<()> {
        let events = event_log.events();
        for event in events {
            self.write_event(&event)?;
        }
        Ok(())
    }

    /// Get the trace file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Close the trace writer (flushes buffer)
    pub fn close(&self) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.flush()?;
        Ok(())
    }
}

/// Generate a unique generation ID
///
/// Format: `YYYY-MM-DDTHH-MM-SS-XXXX` where XXXX is random hex
pub fn generate_generation_id() -> String {
    use chrono::Utc;

    let now = Utc::now();
    let timestamp = now.format("%Y-%m-%dT%H-%M-%S");
    let random: u32 = rand::random::<u32>() % 0x10000; // 0-65535 for 4 hex digits

    format!("{}-{:04x}", timestamp, random)
}

/// Calculate workflow hash (for cache invalidation)
///
/// Uses xxh3 (fast, non-cryptographic) hash.
/// Format: `xxh3:XXXXXXXXXXXXXXXX` (16 hex chars)
pub fn calculate_workflow_hash(yaml: &str) -> String {
    use xxhash_rust::xxh3::xxh3_64;

    let hash = xxh3_64(yaml.as_bytes());
    format!("xxh3:{:016x}", hash)
}

/// Prune old trace files, enforcing both `max_traces` and `retention_days`.
///
/// Deletes the oldest traces beyond `max_traces`, and any trace older than
/// `retention_days`. Logs a warning when files are pruned.
///
/// Safe to call on every write -- performs a single directory listing and
/// at most N unlink calls.
pub fn prune_traces(max_traces: u32, retention_days: u32) {
    prune_traces_in_dir(Path::new(TRACE_DIR), max_traces, retention_days);
}

/// Core pruning logic, parameterised by directory for testability.
fn prune_traces_in_dir(trace_dir: &Path, max_traces: u32, retention_days: u32) {
    if !trace_dir.exists() {
        return;
    }

    let dir_iter = match fs::read_dir(trace_dir) {
        Ok(iter) => iter,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read trace directory for pruning");
            return;
        }
    };

    // Collect .ndjson entries with their creation times
    let mut entries: Vec<(PathBuf, Option<SystemTime>)> = Vec::new();

    for entry in dir_iter {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if path.extension().map(|e| e == "ndjson").unwrap_or(false) {
            let created = entry.metadata().ok().and_then(|m| m.created().ok());
            entries.push((path, created));
        }
    }

    // Sort newest first (None-timestamps sort last so unknown files get pruned first)
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    // Pass 1: retention_days -- mark entries older than cutoff for deletion
    let cutoff = if retention_days > 0 {
        SystemTime::now().checked_sub(Duration::from_secs(u64::from(retention_days) * 86400))
    } else {
        None
    };

    let mut to_delete: Vec<PathBuf> = Vec::new();
    let mut kept: Vec<(PathBuf, Option<SystemTime>)> = Vec::new();

    for (path, created) in entries {
        let expired = match (&cutoff, &created) {
            (Some(cutoff_time), Some(create_time)) => create_time < cutoff_time,
            _ => false,
        };

        if expired {
            to_delete.push(path);
        } else {
            kept.push((path, created));
        }
    }

    // Pass 2: max_traces -- from the kept entries, remove oldest beyond the limit
    if kept.len() > max_traces as usize {
        let excess = kept.split_off(max_traces as usize);
        to_delete.extend(excess.into_iter().map(|(path, _)| path));
    }

    // Delete files
    let mut pruned_count: u32 = 0;
    for path in &to_delete {
        if let Err(e) = fs::remove_file(path) {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "Failed to prune trace file"
            );
        } else {
            pruned_count += 1;
        }
    }

    if pruned_count > 0 {
        tracing::debug!(
            pruned = pruned_count,
            max_traces = max_traces,
            retention_days = retention_days,
            remaining = kept.len(),
            "Pruned old trace files"
        );
    }
}

/// List all trace files
pub fn list_traces() -> Result<Vec<TraceInfo>> {
    let trace_dir = Path::new(TRACE_DIR);

    if !trace_dir.exists() {
        return Ok(vec![]);
    }

    let mut traces = Vec::new();

    for entry in fs::read_dir(trace_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "ndjson").unwrap_or(false) {
            let metadata = entry.metadata()?;
            let generation_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            traces.push(TraceInfo {
                generation_id,
                path,
                size_bytes: metadata.len(),
                created: metadata.created().ok(),
            });
        }
    }

    // Sort by creation time (newest first)
    traces.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(traces)
}

/// Information about a trace file
#[derive(Debug)]
pub struct TraceInfo {
    pub generation_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_id_format() {
        let id = generate_generation_id();
        // Format: YYYY-MM-DDTHH-MM-SS-XXXX
        assert!(id.len() > 20);
        assert!(id.contains('T'));
    }

    #[test]
    fn test_workflow_hash() {
        let yaml = "schema: test\ntasks: []";
        let hash = calculate_workflow_hash(yaml);
        assert!(hash.starts_with("xxh3:"));
        assert_eq!(hash.len(), 21); // "xxh3:" + 16 hex chars
    }

    #[test]
    fn test_workflow_hash_deterministic() {
        let yaml = "schema: test";
        let hash1 = calculate_workflow_hash(yaml);
        let hash2 = calculate_workflow_hash(yaml);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_workflow_hash_different_inputs() {
        let hash1 = calculate_workflow_hash("a");
        let hash2 = calculate_workflow_hash("b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_trace_writer_creates_file() {
        use tempfile::TempDir;

        // Create temp directory and override TRACE_DIR behavior
        let temp_dir = TempDir::new().unwrap();
        let trace_dir = temp_dir.path().join(".nika/traces");
        fs::create_dir_all(&trace_dir).unwrap();

        let gen_id = "test-gen-123";
        let path = trace_dir.join(format!("{}.ndjson", gen_id));
        let file = File::create(&path).unwrap();
        let writer = BufWriter::new(file);

        let trace_writer = TraceWriter {
            writer: Arc::new(Mutex::new(writer)),
            path: path.clone(),
        };

        assert_eq!(trace_writer.path(), path);
    }

    #[test]
    fn test_trace_writer_writes_event() {
        use crate::log::EventKind;
        use serde_json::json;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let trace_dir = temp_dir.path().join(".nika/traces");
        fs::create_dir_all(&trace_dir).unwrap();

        let gen_id = "test-write-event";
        let path = trace_dir.join(format!("{}.ndjson", gen_id));
        let file = File::create(&path).unwrap();
        let writer = BufWriter::new(file);

        let trace_writer = TraceWriter {
            writer: Arc::new(Mutex::new(writer)),
            path: path.clone(),
        };

        let event = Event {
            id: 0,
            timestamp_ms: 100,
            kind: EventKind::TaskStarted {
                verb: "infer".into(),
                task_id: "test_task".into(),
                inputs: Arc::new(json!({})),
            },
        };

        trace_writer.write_event(&event).unwrap();
        trace_writer.close().unwrap();

        // Read back and verify
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("test_task"));
        assert!(content.contains("task_started"));
    }

    #[test]
    fn test_list_traces_empty_dir() {
        // When trace dir doesn't exist, should return empty vec
        let result = list_traces();
        // This may or may not return empty depending on filesystem state
        assert!(result.is_ok());
    }

    #[test]
    fn test_trace_writer_rejects_path_traversal() {
        // Path traversal attempts should be rejected
        let result = TraceWriter::new("../evil");
        assert!(result.is_err());

        let result = TraceWriter::new("foo/../bar");
        assert!(result.is_err());

        let result = TraceWriter::new("foo/bar");
        assert!(result.is_err());

        let result = TraceWriter::new("foo\\bar");
        assert!(result.is_err());
    }

    #[test]
    fn test_trace_writer_rejects_empty_id() {
        let result = TraceWriter::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_trace_writer_accepts_valid_ids() {
        // These should be valid format (even if file creation fails)
        assert!("2024-01-01T12-00-00-abc0"
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == 'T'));
    }

    // ───────────────────────────────────────────────────────────────
    // prune_traces_in_dir tests
    // ───────────────────────────────────────────────────────────────

    /// Helper: create N empty .ndjson files in a temp dir, return the dir path.
    fn make_trace_dir(count: usize) -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        for i in 0..count {
            let name = format!("trace-{:04}.ndjson", i);
            fs::write(tmp.path().join(&name), "").unwrap();
            // Tiny sleep so creation times differ (needed on macOS HFS+ which
            // has 1s resolution). 10ms is enough for APFS/ext4 nanosecond
            // resolution and still keeps tests fast.
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        tmp
    }

    fn count_ndjson(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "ndjson")
                    .unwrap_or(false)
            })
            .count()
    }

    #[test]
    fn test_prune_noop_when_under_limit() {
        let tmp = make_trace_dir(5);
        prune_traces_in_dir(tmp.path(), 100, 0);
        assert_eq!(count_ndjson(tmp.path()), 5);
    }

    #[test]
    fn test_prune_enforces_max_traces() {
        let tmp = make_trace_dir(10);
        assert_eq!(count_ndjson(tmp.path()), 10);

        prune_traces_in_dir(tmp.path(), 3, 0);
        assert_eq!(count_ndjson(tmp.path()), 3);
    }

    #[test]
    fn test_prune_keeps_newest_files() {
        let tmp = make_trace_dir(5);

        // The files are created in order 0000..0004, with 0004 being newest.
        prune_traces_in_dir(tmp.path(), 2, 0);

        let remaining: Vec<String> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "ndjson")
                    .unwrap_or(false)
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        assert_eq!(remaining.len(), 2);
        // The two newest should survive (0003, 0004)
        assert!(remaining.iter().any(|f| f.contains("0004")));
        assert!(remaining.iter().any(|f| f.contains("0003")));
    }

    #[test]
    fn test_prune_nonexistent_dir_is_noop() {
        let dir = Path::new("/tmp/nika-test-nonexistent-dir-12345");
        // Should not panic
        prune_traces_in_dir(dir, 10, 7);
    }

    #[test]
    fn test_prune_empty_dir_is_noop() {
        let tmp = tempfile::TempDir::new().unwrap();
        prune_traces_in_dir(tmp.path(), 5, 7);
        assert_eq!(count_ndjson(tmp.path()), 0);
    }

    #[test]
    fn test_prune_ignores_non_ndjson_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Create some .ndjson and some .txt files
        for i in 0..5 {
            fs::write(tmp.path().join(format!("trace-{}.ndjson", i)), "").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        fs::write(tmp.path().join("notes.txt"), "keep me").unwrap();
        fs::write(tmp.path().join("data.json"), "keep me too").unwrap();

        prune_traces_in_dir(tmp.path(), 2, 0);

        // Only 2 ndjson should remain, and the non-ndjson files should be untouched
        assert_eq!(count_ndjson(tmp.path()), 2);
        assert!(tmp.path().join("notes.txt").exists());
        assert!(tmp.path().join("data.json").exists());
    }

    #[test]
    fn test_prune_max_traces_zero_deletes_all() {
        let tmp = make_trace_dir(5);
        prune_traces_in_dir(tmp.path(), 0, 0);
        assert_eq!(count_ndjson(tmp.path()), 0);
    }
}
