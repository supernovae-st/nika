//! HTTP response caching with ETag / If-Modified-Since support.
//!
//! In-memory cache for the lifetime of a workflow run. Stores response
//! body + cache-control metadata, and sends conditional requests on repeat
//! fetches to the same URL.

use dashmap::DashMap;
use std::time::Instant;

/// Cached response entry.
#[derive(Debug, Clone)]
pub struct CachedResponse {
    /// Response body
    pub body: String,
    /// ETag header (if present)
    pub etag: Option<String>,
    /// Last-Modified header (if present)
    pub last_modified: Option<String>,
    /// HTTP status code of the original response
    pub status: u16,
    /// When this entry was cached
    pub cached_at: Instant,
}

/// Per-workflow HTTP response cache.
///
/// Thread-safe via DashMap. Entries persist for the workflow run duration.
pub struct FetchCache {
    entries: DashMap<String, CachedResponse>,
}

impl FetchCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Store a response in the cache.
    pub fn store(
        &self,
        url: &str,
        body: String,
        status: u16,
        etag: Option<String>,
        last_modified: Option<String>,
    ) {
        self.entries.insert(
            url.to_string(),
            CachedResponse {
                body,
                etag,
                last_modified,
                status,
                cached_at: Instant::now(),
            },
        );
    }

    /// Get a cached response for the URL.
    pub fn get(&self, url: &str) -> Option<CachedResponse> {
        self.entries.get(url).map(|e| e.value().clone())
    }

    /// Get conditional request headers for a URL (If-None-Match, If-Modified-Since).
    ///
    /// Returns headers to add to the request. If the URL is not cached, returns empty vec.
    pub fn conditional_headers(&self, url: &str) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(entry) = self.entries.get(url) {
            if let Some(ref etag) = entry.etag {
                headers.push(("If-None-Match".to_string(), etag.clone()));
            }
            if let Some(ref lm) = entry.last_modified {
                headers.push(("If-Modified-Since".to_string(), lm.clone()));
            }
        }
        headers
    }

    /// Check if a URL is cached.
    pub fn contains(&self, url: &str) -> bool {
        self.entries.contains_key(url)
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FetchCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_get() {
        let cache = FetchCache::new();
        cache.store(
            "https://example.com",
            "body".to_string(),
            200,
            Some("\"abc123\"".to_string()),
            Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string()),
        );
        let entry = cache.get("https://example.com").unwrap();
        assert_eq!(entry.body, "body");
        assert_eq!(entry.status, 200);
        assert_eq!(entry.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(
            entry.last_modified.as_deref(),
            Some("Mon, 01 Jan 2024 00:00:00 GMT")
        );
    }

    #[test]
    fn conditional_headers_with_etag() {
        let cache = FetchCache::new();
        cache.store(
            "https://example.com",
            "body".to_string(),
            200,
            Some("\"etag-val\"".to_string()),
            None,
        );
        let headers = cache.conditional_headers("https://example.com");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "If-None-Match");
        assert_eq!(headers[0].1, "\"etag-val\"");
    }

    #[test]
    fn conditional_headers_with_last_modified() {
        let cache = FetchCache::new();
        cache.store(
            "https://example.com",
            "body".to_string(),
            200,
            None,
            Some("Mon, 01 Jan 2024".to_string()),
        );
        let headers = cache.conditional_headers("https://example.com");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "If-Modified-Since");
    }

    #[test]
    fn conditional_headers_uncached_returns_empty() {
        let cache = FetchCache::new();
        let headers = cache.conditional_headers("https://uncached.com");
        assert!(headers.is_empty());
    }

    #[test]
    fn cache_updates_on_re_store() {
        let cache = FetchCache::new();
        cache.store("https://example.com", "old".to_string(), 200, None, None);
        cache.store("https://example.com", "new".to_string(), 200, None, None);
        let entry = cache.get("https://example.com").unwrap();
        assert_eq!(entry.body, "new");
    }
}
