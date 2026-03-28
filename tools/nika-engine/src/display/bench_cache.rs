//! Bench cache — persist benchmark results for cross-session comparison.
//!
//! Results are stored in `.nika/bench-cache/<workflow_hash>.json`, keyed by
//! workflow content hash so cache invalidates when the workflow changes.
//! Used by Level 4 (Smart Routing) to bootstrap provider selection and by
//! Level 5 (Auto-Optimization) to skip redundant benchmark runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Cached benchmark results for a single workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchCacheEntry {
    /// Hash of the workflow YAML content (16 hex chars, DefaultHasher).
    /// NOTE: DefaultHasher is NOT stable across Rust versions. Cache may
    /// invalidate on toolchain upgrade — this is acceptable for bench cache.
    pub workflow_hash: String,
    /// ISO-8601 timestamp of when the bench was run.
    pub timestamp: String,
    /// Number of iterations used.
    pub iterations: usize,
    /// Per-provider aggregated results.
    pub results: HashMap<String, CachedProviderResult>,
}

/// Aggregated stats for one provider, suitable for cross-session comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedProviderResult {
    pub model: String,
    pub avg_duration_ms: u64,
    pub avg_cost_usd: f64,
    /// Average quality score (0.0–1.0), if evaluated.
    pub avg_quality: Option<f64>,
    pub ttft_p50_ms: u64,
    pub ttft_p90_ms: u64,
    pub avg_input_tokens: u64,
    pub avg_output_tokens: u64,
    pub avg_tokens_per_sec: f64,
}

/// Compute the cache key for a workflow YAML string.
///
/// Uses `DefaultHasher` (SipHash) — not cryptographic, not stable across
/// Rust versions. This is acceptable: bench cache is ephemeral and will
/// simply miss (re-benchmark) after a toolchain upgrade.
pub fn workflow_hash(yaml_content: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    yaml_content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Get the bench cache directory path (`.nika/bench-cache/`).
pub fn cache_dir(base_path: &Path) -> PathBuf {
    base_path.join(".nika").join("bench-cache")
}

/// Get the cache file path for a given workflow hash.
pub fn cache_file(base_path: &Path, hash: &str) -> PathBuf {
    cache_dir(base_path).join(format!("{}.json", hash))
}

/// Read cached bench results for a workflow, if they exist and are valid JSON.
pub fn read_cache(base_path: &Path, hash: &str) -> Option<BenchCacheEntry> {
    let path = cache_file(base_path, hash);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write bench results to the cache file.
///
/// Creates the `.nika/bench-cache/` directory if it doesn't exist.
pub fn write_cache(base_path: &Path, entry: &BenchCacheEntry) -> Result<PathBuf, std::io::Error> {
    let dir = cache_dir(base_path);
    std::fs::create_dir_all(&dir)?;

    let path = cache_file(base_path, &entry.workflow_hash);
    let json = serde_json::to_string_pretty(entry)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_hash_deterministic() {
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: test\n    infer: \"hello\"";
        let h1 = workflow_hash(yaml);
        let h2 = workflow_hash(yaml);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn workflow_hash_changes_on_content_change() {
        let h1 = workflow_hash("version: 1");
        let h2 = workflow_hash("version: 2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let entry = BenchCacheEntry {
            workflow_hash: "abcdef123456".to_string(),
            timestamp: "2026-03-28T12:00:00Z".to_string(),
            iterations: 3,
            results: {
                let mut m = HashMap::new();
                m.insert(
                    "anthropic".to_string(),
                    CachedProviderResult {
                        model: "claude-sonnet-4".to_string(),
                        avg_duration_ms: 12400,
                        avg_cost_usd: 0.023,
                        avg_quality: Some(0.92),
                        ttft_p50_ms: 230,
                        ttft_p90_ms: 340,
                        avg_input_tokens: 1200,
                        avg_output_tokens: 420,
                        avg_tokens_per_sec: 85.0,
                    },
                );
                m
            },
        };

        let path = write_cache(dir.path(), &entry).unwrap();
        assert!(path.exists());

        let loaded = read_cache(dir.path(), "abcdef123456").unwrap();
        assert_eq!(loaded.workflow_hash, "abcdef123456");
        assert_eq!(loaded.iterations, 3);
        assert_eq!(loaded.results.len(), 1);
        let anthropic = &loaded.results["anthropic"];
        assert_eq!(anthropic.avg_duration_ms, 12400);
        assert!((anthropic.avg_cost_usd - 0.023).abs() < f64::EPSILON);
    }

    #[test]
    fn read_cache_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_cache(dir.path(), "nonexistent").is_none());
    }

    #[test]
    fn cache_file_path_correct() {
        let path = cache_file(Path::new("/tmp/project"), "abc123");
        assert_eq!(
            path,
            PathBuf::from("/tmp/project/.nika/bench-cache/abc123.json")
        );
    }
}
