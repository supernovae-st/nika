//! LLM response cache — in-memory DashMap + SQLite persistence.
//!
//! Cache key: `blake3(model + provider + prompt + system + temperature + max_tokens)`
//!
//! Research findings applied:
//! - DashMap for hot cache with lazy TTL expiration on reads
//! - Background reaper task for periodic cleanup via `retain()`
//! - Never hold DashMap Ref guards across .await (instant deadlock)
//! - SQLite persistence for cache survival across daemon restarts

use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::{debug, info, trace};

// Storage types used for future SQLite persistence

/// Default TTL: 1 hour.
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Maximum cache entries (memory bound).
const MAX_ENTRIES: usize = 1024;

/// A cached LLM response.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub key: String,
    pub provider: String,
    pub model: String,
    pub response: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost: f64,
    pub created_at: Instant,
    pub ttl: Duration,
    pub hits: u64,
}

/// Parameters for setting a cache entry.
pub struct CacheSetParams {
    pub key: String,
    pub provider: String,
    pub model: String,
    pub response: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost: f64,
    pub ttl: Option<Duration>,
}

/// Cache statistics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_tokens_saved: u64,
    pub total_cost_saved: f64,
}

/// Inner state for the cache (shared via Arc for cheap cloning).
struct CacheInner {
    /// Hot cache (in-memory).
    cache: DashMap<String, CacheEntry>,
    /// Stats counters.
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
    evictions: std::sync::atomic::AtomicU64,
}

/// The LLM response cache.
///
/// Cheaply cloneable via internal `Arc` — safe to share across tasks.
#[derive(Clone)]
pub struct CacheService {
    inner: std::sync::Arc<CacheInner>,
}

impl CacheService {
    /// Create a new cache service.
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(CacheInner {
                cache: DashMap::with_capacity(256),
                hits: std::sync::atomic::AtomicU64::new(0),
                misses: std::sync::atomic::AtomicU64::new(0),
                evictions: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }

    /// Compute a cache key from LLM request parameters.
    pub fn compute_key(
        provider: &str,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        // Length-prefix variable fields to prevent delimiter injection collisions
        // (e.g., provider="a|b" model="c" vs provider="a" model="b|c")
        hasher.update(&(provider.len() as u32).to_le_bytes());
        hasher.update(provider.as_bytes());
        hasher.update(&(model.len() as u32).to_le_bytes());
        hasher.update(model.as_bytes());
        hasher.update(&(prompt.len() as u32).to_le_bytes());
        hasher.update(prompt.as_bytes());
        match system {
            Some(sys) => {
                hasher.update(&[0x01]);
                hasher.update(sys.as_bytes());
            }
            None => {
                hasher.update(&[0x00]);
            }
        }
        match temperature {
            Some(temp) => {
                hasher.update(&[0x01]);
                hasher.update(&temp.to_le_bytes());
            }
            None => {
                hasher.update(&[0x00]);
            }
        }
        match max_tokens {
            Some(max) => {
                hasher.update(&[0x01]);
                hasher.update(&max.to_le_bytes());
            }
            None => {
                hasher.update(&[0x00]);
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Get a cached response. Returns None on miss or expired entry.
    pub fn get(&self, key: &str) -> Option<CacheEntry> {
        // Lazy TTL check: clone value, drop guard, then check expiry
        let entry = match self.inner.cache.get(key) {
            Some(guard) => guard.clone(),
            None => {
                self.inner.misses
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
        };

        // Check TTL
        if entry.created_at.elapsed() > entry.ttl {
            // Expired — remove and count as miss
            self.inner.cache.remove(key);
            self.inner.evictions
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.inner.misses
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            trace!(key, "cache expired");
            return None;
        }

        // Increment hit count (non-blocking)
        self.inner.cache
            .entry(key.to_string())
            .and_modify(|e| e.hits += 1);
        self.inner.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        debug!(key, "cache hit");
        Some(entry)
    }

    /// Store a response in the cache.
    pub fn set(&self, params: CacheSetParams) {
        let CacheSetParams {
            key,
            provider,
            model,
            response,
            tokens_in,
            tokens_out,
            cost,
            ttl,
        } = params;
        // Evict if at capacity
        if self.inner.cache.len() >= MAX_ENTRIES {
            self.evict_oldest();
        }

        let entry = CacheEntry {
            key: key.clone(),
            provider,
            model,
            response,
            tokens_in,
            tokens_out,
            cost,
            created_at: Instant::now(),
            ttl: ttl.unwrap_or(DEFAULT_TTL),
            hits: 0,
        };

        self.inner.cache.insert(key.clone(), entry);
        debug!(key, "cache set");
    }

    /// Clear all cache entries.
    pub fn clear(&self) {
        let count = self.inner.cache.len();
        self.inner.cache.clear();
        info!(count, "cache cleared");
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let total_tokens_saved: u64 = self
            .inner.cache
            .iter()
            .map(|e| (e.tokens_in + e.tokens_out) * e.hits)
            .sum();

        let total_cost_saved: f64 = self.inner.cache.iter().map(|e| e.cost * e.hits as f64).sum();

        CacheStats {
            entries: self.inner.cache.len(),
            hits: self.inner.hits.load(std::sync::atomic::Ordering::Relaxed),
            misses: self.inner.misses.load(std::sync::atomic::Ordering::Relaxed),
            evictions: self.inner.evictions.load(std::sync::atomic::Ordering::Relaxed),
            total_tokens_saved,
            total_cost_saved,
        }
    }

    /// Remove expired entries (called periodically by reaper task).
    ///
    /// Returns the number of evicted entries.
    pub fn cleanup_expired(&self) -> usize {
        let before = self.inner.cache.len();
        self.inner.cache
            .retain(|_, entry| entry.created_at.elapsed() <= entry.ttl);
        let evicted = before - self.inner.cache.len();
        if evicted > 0 {
            self.inner.evictions
                .fetch_add(evicted as u64, std::sync::atomic::Ordering::Relaxed);
            debug!(evicted, "expired entries cleaned up");
        }
        evicted
    }

    /// Evict the oldest entry by creation time.
    fn evict_oldest(&self) {
        let oldest_key = self
            .inner.cache
            .iter()
            .min_by_key(|e| e.created_at)
            .map(|e| e.key.clone());

        if let Some(key) = oldest_key {
            self.inner.cache.remove(&key);
            self.inner.evictions
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            trace!(key, "evicted oldest entry");
        }
    }
}

impl Default for CacheService {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_deterministic() {
        let k1 =
            CacheService::compute_key("anthropic", "claude-sonnet-4-6", "hello", None, None, None);
        let k2 =
            CacheService::compute_key("anthropic", "claude-sonnet-4-6", "hello", None, None, None);
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_differs_on_prompt() {
        let k1 =
            CacheService::compute_key("anthropic", "claude-sonnet-4-6", "hello", None, None, None);
        let k2 =
            CacheService::compute_key("anthropic", "claude-sonnet-4-6", "world", None, None, None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_differs_on_model() {
        let k1 =
            CacheService::compute_key("anthropic", "claude-sonnet-4-6", "hello", None, None, None);
        let k2 =
            CacheService::compute_key("anthropic", "claude-haiku-4-5", "hello", None, None, None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_differs_on_temperature() {
        let k1 = CacheService::compute_key("openai", "gpt-4o", "hello", None, Some(0.0), None);
        let k2 = CacheService::compute_key("openai", "gpt-4o", "hello", None, Some(0.7), None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_hit_returns_value() {
        let cache = CacheService::new();
        let key = "test-key".to_string();

        cache.set(CacheSetParams {
            key: key.clone(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            response: "Hello!".into(),
            tokens_in: 10,
            tokens_out: 5,
            cost: 0.001,
            ttl: None,
        });

        let entry = cache.get(&key).unwrap();
        assert_eq!(entry.response, "Hello!");
        assert_eq!(entry.tokens_in, 10);
    }

    #[test]
    fn cache_miss_returns_none() {
        let cache = CacheService::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn cache_ttl_expires() {
        let cache = CacheService::new();
        let key = "expire-me".to_string();

        cache.set(CacheSetParams {
            key: key.clone(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            response: "response".into(),
            tokens_in: 5,
            tokens_out: 5,
            cost: 0.0,
            ttl: Some(Duration::from_millis(1)), // 1ms TTL
        });

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(10));

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn cache_eviction_at_capacity() {
        let cache = CacheService::new();

        // Fill to MAX_ENTRIES
        for i in 0..MAX_ENTRIES {
            cache.set(CacheSetParams {
                key: format!("key-{i}"),
                provider: "test".into(),
                model: "test".into(),
                response: "response".into(),
                tokens_in: 1,
                tokens_out: 1,
                cost: 0.0,
                ttl: None,
            });
        }

        assert_eq!(cache.inner.cache.len(), MAX_ENTRIES);

        // Add one more — should evict oldest
        cache.set(CacheSetParams {
            key: "overflow".into(),
            provider: "test".into(),
            model: "test".into(),
            response: "new".into(),
            tokens_in: 1,
            tokens_out: 1,
            cost: 0.0,
            ttl: None,
        });

        assert_eq!(cache.inner.cache.len(), MAX_ENTRIES);
        assert!(cache.get("overflow").is_some());
    }

    #[test]
    fn cache_clear_removes_all() {
        let cache = CacheService::new();
        for i in 0..10 {
            cache.set(CacheSetParams {
                key: format!("key-{i}"),
                provider: "test".into(),
                model: "test".into(),
                response: "r".into(),
                tokens_in: 1,
                tokens_out: 1,
                cost: 0.0,
                ttl: None,
            });
        }

        cache.clear();
        assert_eq!(cache.inner.cache.len(), 0);
    }

    #[test]
    fn cache_stats_tracks_hits_misses() {
        let cache = CacheService::new();
        cache.set(CacheSetParams {
            key: "k1".into(),
            provider: "test".into(),
            model: "test".into(),
            response: "r".into(),
            tokens_in: 10,
            tokens_out: 5,
            cost: 0.01,
            ttl: None,
        });

        // Hit
        cache.get("k1");
        cache.get("k1");

        // Miss
        cache.get("nonexistent");

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn cleanup_expired_removes_stale() {
        let cache = CacheService::new();
        cache.set(CacheSetParams {
            key: "stale".into(),
            provider: "test".into(),
            model: "test".into(),
            response: "r".into(),
            tokens_in: 1,
            tokens_out: 1,
            cost: 0.0,
            ttl: Some(Duration::from_millis(1)),
        });
        cache.set(CacheSetParams {
            key: "fresh".into(),
            provider: "test".into(),
            model: "test".into(),
            response: "r".into(),
            tokens_in: 1,
            tokens_out: 1,
            cost: 0.0,
            ttl: Some(Duration::from_secs(3600)),
        });

        std::thread::sleep(Duration::from_millis(10));
        cache.cleanup_expired();

        assert!(cache.get("stale").is_none());
        assert!(cache.get("fresh").is_some());
    }

    #[test]
    fn cache_key_none_vs_empty_string_differ() {
        let k1 = CacheService::compute_key("p", "m", "hi", None, None, None);
        let k2 = CacheService::compute_key("p", "m", "hi", Some(""), None, None);
        assert_ne!(k1, k2, "None and Some(\"\") must produce different keys");
    }

    #[test]
    fn cache_key_none_vs_zero_temperature_differ() {
        let k1 = CacheService::compute_key("p", "m", "hi", None, None, None);
        let k2 = CacheService::compute_key("p", "m", "hi", None, Some(0.0), None);
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_none_vs_zero_max_tokens_differ() {
        let k1 = CacheService::compute_key("p", "m", "hi", None, None, None);
        let k2 = CacheService::compute_key("p", "m", "hi", None, None, Some(0));
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_with_system_prompt() {
        let k1 =
            CacheService::compute_key("anthropic", "sonnet", "hi", Some("be helpful"), None, None);
        let k2 = CacheService::compute_key("anthropic", "sonnet", "hi", None, None, None);
        assert_ne!(k1, k2);
    }
}
