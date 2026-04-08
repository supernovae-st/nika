// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON Formatting Cache
//!
//! Cache for formatted JSON strings to avoid repeated `serde_json::to_string_pretty` calls.
//! Uses a simple LRU-style eviction strategy when cache exceeds capacity.

use std::collections::HashMap;

/// Cache for formatted JSON strings to avoid repeated serde_json::to_string_pretty calls
#[derive(Debug, Clone, Default)]
pub struct JsonFormatCache {
    /// Cached formatted JSON by key (task_id, mcp_call_id, or "final_output")
    cache: HashMap<String, String>,
    /// Maximum cache entries before eviction
    max_entries: usize,
}

impl JsonFormatCache {
    /// Create a new cache with default capacity
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_entries: 50,
        }
    }

    /// Create a new cache with specific capacity (for testing)
    #[cfg(test)]
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_entries,
        }
    }

    /// Get cached JSON for a key, or format and cache it
    /// PERF: Returns &str to avoid clone on every cache hit
    pub fn get_or_format<T: serde::Serialize>(&mut self, key: &str, value: &T) -> &str {
        // PERF: Only format and insert if not already cached
        if !self.cache.contains_key(key) {
            let formatted = serde_json::to_string_pretty(value).unwrap_or_default();

            // Simple LRU-style eviction: clear oldest entries if over limit
            if self.cache.len() >= self.max_entries {
                // Remove first 10% of entries, minimum 1 (oldest by insertion order)
                let to_remove = (self.max_entries / 10).max(1);
                let keys: Vec<String> = self.cache.keys().take(to_remove).cloned().collect();
                for k in keys {
                    self.cache.remove(&k);
                }
            }

            self.cache.insert(key.to_string(), formatted);
        }

        self.cache.get(key).map(|s| s.as_str()).unwrap_or("")
    }

    /// Invalidate specific cache entries
    pub fn invalidate(&mut self, key: &str) {
        self.cache.remove(key);
    }

    /// Invalidate all entries starting with a prefix
    pub fn invalidate_prefix(&mut self, prefix: &str) {
        self.cache.retain(|k, _| !k.starts_with(prefix));
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache stats for debugging
    #[cfg(test)]
    pub fn stats(&self) -> (usize, usize) {
        (self.cache.len(), self.max_entries)
    }
}
