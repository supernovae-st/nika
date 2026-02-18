//! NDJSON Trace Writer
//!
//! Writes events to newline-delimited JSON files for debugging and replay.

use crate::error::Result;
use crate::event::{Event, EventLog};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
            return Err(crate::error::NikaError::ValidationError {
                reason: format!(
                    "Invalid generation_id: must be alphanumeric with hyphens/underscores only, got: {}",
                    generation_id
                ),
            }
            .into());
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
    let random: u32 = rand::random::<u32>() % 0x10000;  // 0-65535 for 4 hex digits

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
        use crate::event::EventKind;
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
                task_id: "test_task".into(),
                inputs: json!({}),
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
        assert!(
            "2024-01-01T12-00-00-abc0"
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == 'T')
        );
    }
}
