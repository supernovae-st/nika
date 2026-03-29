//! Record — compressed representation of task output.
//!
//! When `record: { compress: true }`, the engine uses a cheap LLM to compress
//! raw output into a structured `Record` (summary, key_findings, confidence).
//! Downstream tasks receive the compressed Record via bindings instead of raw output.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Compressed task output produced by the RecordCompressor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Task that produced this record
    pub task_id: Arc<str>,
    /// Compressed summary of the output
    pub summary: String,
    /// Key findings extracted from the output
    pub key_findings: Vec<String>,
    /// Original raw output (retained when compress fails or for debugging)
    pub raw_output: Option<String>,
    /// Confidence score from the compression LLM (0.0-1.0)
    pub confidence: f64,
    /// Token count of the original output
    pub tokens_original: u64,
    /// Token count of the compressed summary
    pub tokens_compressed: u64,
    /// Model used for compression
    pub compression_model: String,
    /// Cost of the compression call
    pub compression_cost_usd: f64,
    /// Duration of the compression call
    #[serde(with = "duration_millis")]
    pub compression_duration: Duration,
}

mod duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(duration: &Duration, s: S) -> Result<S::Ok, S::Error> {
        (duration.as_millis() as u64).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}

impl Record {
    /// Compression ratio: compressed / original tokens. Returns 1.0 if original is 0.
    pub fn compression_ratio(&self) -> f64 {
        if self.tokens_original == 0 {
            1.0
        } else {
            self.tokens_compressed as f64 / self.tokens_original as f64
        }
    }

    /// Whether this record meets the minimum confidence threshold.
    pub fn meets_threshold(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }

    /// Convert to a serde_json::Value for use in bindings.
    ///
    /// Returns an object with summary, key_findings, and confidence.
    /// When accessed as `$task`, the summary string is used directly.
    /// When accessed as `$task.confidence`, the nested field is resolved.
    pub fn to_binding_value(&self) -> serde_json::Value {
        serde_json::json!({
            "summary": self.summary,
            "key_findings": self.key_findings,
            "confidence": self.confidence,
            "tokens_original": self.tokens_original,
            "tokens_compressed": self.tokens_compressed,
            "compression_ratio": self.compression_ratio(),
        })
    }

    /// Create a truncation-based fallback record (no LLM, just cut output).
    pub fn truncated(task_id: Arc<str>, raw_output: &str, max_tokens: u32) -> Self {
        let char_budget = (max_tokens as usize) * 4; // ~4 chars per token heuristic
        let summary = if raw_output.len() > char_budget {
            format!("{}...", &raw_output[..char_budget])
        } else {
            raw_output.to_string()
        };
        let tokens_compressed = (summary.len() / 4) as u64;
        let tokens_original = (raw_output.len() / 4) as u64;
        Self {
            task_id,
            summary,
            key_findings: vec![],
            raw_output: Some(raw_output.to_string()),
            confidence: 0.0,
            tokens_original,
            tokens_compressed,
            compression_model: "truncation".to_string(),
            compression_cost_usd: 0.0,
            compression_duration: Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record() -> Record {
        Record {
            task_id: "research".into(),
            summary: "AI is transforming industries.".to_string(),
            key_findings: vec![
                "Healthcare adoption growing".to_string(),
                "Manufacturing leads in ROI".to_string(),
            ],
            raw_output: Some("Very long research output...".to_string()),
            confidence: 0.85,
            tokens_original: 5000,
            tokens_compressed: 150,
            compression_model: "claude-haiku-4-5".to_string(),
            compression_cost_usd: 0.0002,
            compression_duration: Duration::from_millis(340),
        }
    }

    #[test]
    fn test_record_construction() {
        let r = make_record();
        assert_eq!(r.task_id.as_ref(), "research");
        assert_eq!(r.key_findings.len(), 2);
    }

    #[test]
    fn test_compression_ratio_normal() {
        let r = make_record();
        let ratio = r.compression_ratio();
        assert!(ratio > 0.0 && ratio < 1.0, "ratio={ratio}");
        assert!((ratio - 0.03).abs() < 0.01, "150/5000 = 0.03");
    }

    #[test]
    fn test_compression_ratio_zero_tokens() {
        let mut r = make_record();
        r.tokens_original = 0;
        assert!((r.compression_ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_meets_threshold_above() {
        let r = make_record();
        assert!(r.meets_threshold(0.8));
        assert!(r.meets_threshold(0.85));
    }

    #[test]
    fn test_meets_threshold_below() {
        let r = make_record();
        assert!(!r.meets_threshold(0.9));
    }

    #[test]
    fn test_to_binding_value() {
        let r = make_record();
        let v = r.to_binding_value();
        assert_eq!(v["summary"], "AI is transforming industries.");
        assert_eq!(v["key_findings"].as_array().unwrap().len(), 2);
        assert_eq!(v["confidence"], 0.85);
        assert!(v["compression_ratio"].as_f64().unwrap() < 1.0);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let r = make_record();
        let json = serde_json::to_string(&r).unwrap();
        let r2: Record = serde_json::from_str(&json).unwrap();
        assert_eq!(r2.task_id.as_ref(), "research");
        assert_eq!(r2.summary, r.summary);
        assert_eq!(r2.compression_duration, Duration::from_millis(340));
    }

    #[test]
    fn test_truncated_fallback() {
        let raw = "a".repeat(8000);
        let r = Record::truncated("task1".into(), &raw, 500);
        assert_eq!(r.confidence, 0.0);
        assert_eq!(r.compression_model, "truncation");
        assert!(r.summary.len() <= 2003); // 500*4 + "..."
        assert!(r.raw_output.is_some());
    }
}
