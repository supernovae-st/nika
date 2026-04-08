// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verification Cache Module
//!
//! TTL-based caching for provider and MCP server verification results.
//! Prevents redundant API calls when opening the provider selector.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_tui::verification::{VerificationCache, VerificationEntry};
//! use std::time::Duration;
//!
//! let mut cache = VerificationCache::new(Duration::from_secs(30));
//!
//! // Check if cached (returns None if expired)
//! if let Some(entry) = cache.get_provider("claude") {
//!     println!("cached: {:?}", entry.status);
//! }
//!
//! // Store new result (expired entries are purged automatically)
//! cache.set_provider("claude".to_string(), entry);
//! ```

use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};

use super::widgets::VerifyStatus;

/// Cached verification result for a provider or MCP server
#[derive(Debug, Clone)]
pub struct VerificationEntry {
    /// Verification status (Unknown, Verifying, Verified, Failed)
    pub status: VerifyStatus,
    /// Measured latency (if verified)
    pub latency: Option<Duration>,
    /// Number of tools (MCP servers only)
    pub tool_count: Option<usize>,
    /// Model name (providers only)
    pub model: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// When this entry was created
    pub verified_at: Instant,
}

impl VerificationEntry {
    /// Create a new verified entry with latency
    pub fn verified(latency: Duration, model: Option<String>) -> Self {
        Self {
            status: VerifyStatus::Verified,
            latency: Some(latency),
            tool_count: None,
            model,
            error: None,
            verified_at: Instant::now(),
        }
    }

    /// Create a new verified MCP entry with latency and tool count
    pub fn verified_mcp(latency: Duration, tool_count: usize) -> Self {
        Self {
            status: VerifyStatus::Verified,
            latency: Some(latency),
            tool_count: Some(tool_count),
            model: None,
            error: None,
            verified_at: Instant::now(),
        }
    }

    /// Create a new failed entry with error message
    pub fn failed(error: String) -> Self {
        Self {
            status: VerifyStatus::Failed,
            latency: None,
            tool_count: None,
            model: None,
            error: Some(error),
            verified_at: Instant::now(),
        }
    }

    /// Create a verifying (in-progress) entry
    pub fn verifying() -> Self {
        Self {
            status: VerifyStatus::Verifying,
            latency: None,
            tool_count: None,
            model: None,
            error: None,
            verified_at: Instant::now(),
        }
    }
}

/// TTL-based verification cache
///
/// Stores verification results for providers and MCP servers with automatic
/// expiration. Default TTL is 30 seconds. Expired entries are purged on
/// write operations and filtered out on read operations.
#[derive(Debug)]
pub struct VerificationCache {
    /// Cached provider verification results
    providers: FxHashMap<String, VerificationEntry>,
    /// Cached MCP server verification results
    mcp_servers: FxHashMap<String, VerificationEntry>,
    /// Time-to-live for cache entries
    ttl: Duration,
}

impl Default for VerificationCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl VerificationCache {
    /// Create a new cache with the specified TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            providers: FxHashMap::default(),
            mcp_servers: FxHashMap::default(),
            ttl,
        }
    }

    /// Get a cached provider entry (returns None if expired)
    pub fn get_provider(&self, id: &str) -> Option<&VerificationEntry> {
        self.providers
            .get(id)
            .filter(|e| e.verified_at.elapsed() < self.ttl)
    }

    /// Get a cached MCP server entry (returns None if expired)
    pub fn get_mcp(&self, name: &str) -> Option<&VerificationEntry> {
        self.mcp_servers
            .get(name)
            .filter(|e| e.verified_at.elapsed() < self.ttl)
    }

    /// Store a provider verification result, purging expired entries first
    pub fn set_provider(&mut self, id: String, entry: VerificationEntry) {
        self.purge_expired();
        self.providers.insert(id, entry);
    }

    /// Store an MCP server verification result, purging expired entries first
    pub fn set_mcp(&mut self, name: String, entry: VerificationEntry) {
        self.purge_expired();
        self.mcp_servers.insert(name, entry);
    }

    /// Remove all entries that have exceeded their TTL
    pub fn purge_expired(&mut self) {
        let ttl = self.ttl;
        self.providers.retain(|_, e| e.verified_at.elapsed() < ttl);
        self.mcp_servers
            .retain(|_, e| e.verified_at.elapsed() < ttl);
    }

    /// Check if an entry is still valid (within TTL)
    pub fn is_valid(&self, entry: &VerificationEntry) -> bool {
        entry.verified_at.elapsed() < self.ttl
    }

    /// Check if a provider has a valid cached entry
    pub fn has_valid_provider(&self, id: &str) -> bool {
        self.get_provider(id).is_some()
    }

    /// Check if an MCP server has a valid cached entry
    pub fn has_valid_mcp(&self, name: &str) -> bool {
        self.get_mcp(name).is_some()
    }

    /// Invalidate all cached entries
    pub fn invalidate_all(&mut self) {
        self.providers.clear();
        self.mcp_servers.clear();
    }

    /// Invalidate a specific provider
    pub fn invalidate_provider(&mut self, id: &str) {
        self.providers.remove(id);
    }

    /// Invalidate a specific MCP server
    pub fn invalidate_mcp(&mut self, name: &str) {
        self.mcp_servers.remove(name);
    }

    /// Get the number of cached providers
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Get the number of cached MCP servers
    pub fn mcp_count(&self) -> usize {
        self.mcp_servers.len()
    }

    /// Get count of verified providers
    pub fn verified_provider_count(&self) -> usize {
        self.providers
            .values()
            .filter(|e| e.status == VerifyStatus::Verified && self.is_valid(e))
            .count()
    }

    /// Check if ANY provider has been verified
    pub fn has_any_verified_provider(&self) -> bool {
        self.providers
            .values()
            .any(|e| e.status == VerifyStatus::Verified && self.is_valid(e))
    }

    /// Get count of verified MCP servers
    pub fn verified_mcp_count(&self) -> usize {
        self.mcp_servers
            .values()
            .filter(|e| e.status == VerifyStatus::Verified && self.is_valid(e))
            .count()
    }

    /// Get the TTL duration
    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_cache_new_empty() {
        let cache = VerificationCache::new(Duration::from_secs(30));
        assert_eq!(cache.provider_count(), 0);
        assert_eq!(cache.mcp_count(), 0);
        assert_eq!(cache.ttl(), Duration::from_secs(30));
    }

    #[test]
    fn test_cache_default_ttl() {
        let cache = VerificationCache::default();
        assert_eq!(cache.ttl(), Duration::from_secs(30));
    }

    #[test]
    fn test_cache_set_get_provider() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        let entry = VerificationEntry::verified(
            Duration::from_millis(450),
            Some("claude-sonnet-4".to_string()),
        );
        cache.set_provider("claude".to_string(), entry);
        assert_eq!(cache.provider_count(), 1);
        let retrieved = cache.get_provider("claude").unwrap();
        assert_eq!(retrieved.status, VerifyStatus::Verified);
        assert_eq!(retrieved.latency, Some(Duration::from_millis(450)));
        assert_eq!(retrieved.model, Some("claude-sonnet-4".to_string()));
    }

    #[test]
    fn test_cache_set_get_mcp() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        let entry = VerificationEntry::verified_mcp(Duration::from_millis(45), 12);
        cache.set_mcp("novanet".to_string(), entry);
        assert_eq!(cache.mcp_count(), 1);
        let retrieved = cache.get_mcp("novanet").unwrap();
        assert_eq!(retrieved.status, VerifyStatus::Verified);
        assert_eq!(retrieved.latency, Some(Duration::from_millis(45)));
        assert_eq!(retrieved.tool_count, Some(12));
    }

    #[test]
    fn test_cache_ttl_valid() {
        let cache = VerificationCache::new(Duration::from_secs(30));
        let entry = VerificationEntry::verified(Duration::from_millis(100), None);
        assert!(cache.is_valid(&entry));
    }

    #[test]
    fn test_cache_ttl_expired() {
        let cache = VerificationCache::new(Duration::from_millis(10));
        let entry = VerificationEntry::verified(Duration::from_millis(100), None);
        sleep(Duration::from_millis(15));
        assert!(!cache.is_valid(&entry));
    }

    #[test]
    fn test_cache_has_valid_provider() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        assert!(!cache.has_valid_provider("claude"));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        assert!(cache.has_valid_provider("claude"));
    }

    #[test]
    fn test_cache_invalidate_all() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        cache.set_provider(
            "openai".to_string(),
            VerificationEntry::verified(Duration::from_millis(200), None),
        );
        cache.set_mcp(
            "novanet".to_string(),
            VerificationEntry::verified_mcp(Duration::from_millis(50), 10),
        );
        assert_eq!(cache.provider_count(), 2);
        assert_eq!(cache.mcp_count(), 1);
        cache.invalidate_all();
        assert_eq!(cache.provider_count(), 0);
        assert_eq!(cache.mcp_count(), 0);
    }

    #[test]
    fn test_cache_invalidate_specific() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        cache.set_provider(
            "openai".to_string(),
            VerificationEntry::verified(Duration::from_millis(200), None),
        );
        cache.invalidate_provider("claude");
        assert_eq!(cache.provider_count(), 1);
        assert!(cache.get_provider("claude").is_none());
        assert!(cache.get_provider("openai").is_some());
    }

    #[test]
    fn test_cache_failed_entry() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        let entry = VerificationEntry::failed("Connection refused".to_string());
        cache.set_provider("native".to_string(), entry);
        let retrieved = cache.get_provider("native").unwrap();
        assert_eq!(retrieved.status, VerifyStatus::Failed);
        assert_eq!(retrieved.error, Some("Connection refused".to_string()));
        assert!(retrieved.latency.is_none());
    }

    #[test]
    fn test_cache_verifying_entry() {
        let entry = VerificationEntry::verifying();
        assert_eq!(entry.status, VerifyStatus::Verifying);
        assert!(entry.latency.is_none());
        assert!(entry.error.is_none());
    }

    #[test]
    fn test_verified_provider_count() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        cache.set_provider(
            "openai".to_string(),
            VerificationEntry::verified(Duration::from_millis(200), None),
        );
        cache.set_provider(
            "native".to_string(),
            VerificationEntry::failed("Offline".to_string()),
        );
        assert_eq!(cache.verified_provider_count(), 2);
    }

    #[test]
    fn test_verified_mcp_count() {
        let mut cache = VerificationCache::new(Duration::from_secs(30));
        cache.set_mcp(
            "novanet".to_string(),
            VerificationEntry::verified_mcp(Duration::from_millis(50), 12),
        );
        cache.set_mcp(
            "firecrawl".to_string(),
            VerificationEntry::failed("Timeout".to_string()),
        );
        assert_eq!(cache.verified_mcp_count(), 1);
    }

    #[test]
    fn test_purge_expired() {
        let mut cache = VerificationCache::new(Duration::from_millis(10));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        cache.set_mcp(
            "novanet".to_string(),
            VerificationEntry::verified_mcp(Duration::from_millis(50), 5),
        );
        assert_eq!(cache.provider_count(), 1);
        assert_eq!(cache.mcp_count(), 1);
        sleep(Duration::from_millis(15));
        cache.purge_expired();
        assert_eq!(cache.provider_count(), 0);
        assert_eq!(cache.mcp_count(), 0);
    }

    #[test]
    fn test_get_provider_filters_expired() {
        let mut cache = VerificationCache::new(Duration::from_millis(10));
        cache.set_provider(
            "claude".to_string(),
            VerificationEntry::verified(Duration::from_millis(100), None),
        );
        assert!(cache.get_provider("claude").is_some());
        sleep(Duration::from_millis(15));
        assert!(cache.get_provider("claude").is_none());
    }
}
