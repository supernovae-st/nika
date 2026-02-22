//! Render cache for TUI performance optimization
//!
//! Caches expensive rendering computations to avoid redundant calculations.

use std::collections::HashMap;

/// Cache for rendered content to avoid redundant computations
#[derive(Debug, Default, Clone)]
pub struct RenderCache {
    /// Cached line widths for text content
    line_widths: HashMap<u64, usize>,
    /// Cache hit count for debugging
    hits: usize,
    /// Cache miss count for debugging
    misses: usize,
}

impl RenderCache {
    /// Create a new empty render cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached line width or compute and cache it
    pub fn get_or_compute_width<F>(&mut self, key: u64, compute: F) -> usize
    where
        F: FnOnce() -> usize,
    {
        if let Some(&width) = self.line_widths.get(&key) {
            self.hits += 1;
            width
        } else {
            self.misses += 1;
            let width = compute();
            self.line_widths.insert(key, width);
            width
        }
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.line_widths.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    /// Get cache size (number of entries)
    pub fn len(&self) -> usize {
        self.line_widths.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.line_widths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_cache_new() {
        let cache = RenderCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.stats(), (0, 0));
    }

    #[test]
    fn test_render_cache_get_or_compute() {
        let mut cache = RenderCache::new();

        // First call should compute
        let width = cache.get_or_compute_width(1, || 42);
        assert_eq!(width, 42);
        assert_eq!(cache.stats(), (0, 1)); // 1 miss

        // Second call should use cache
        let width = cache.get_or_compute_width(1, || 100);
        assert_eq!(width, 42); // Still 42, not 100
        assert_eq!(cache.stats(), (1, 1)); // 1 hit, 1 miss
    }

    #[test]
    fn test_render_cache_clear() {
        let mut cache = RenderCache::new();
        cache.get_or_compute_width(1, || 42);
        cache.get_or_compute_width(2, || 24);

        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.stats(), (0, 0));
    }

    #[test]
    fn test_render_cache_multiple_keys() {
        let mut cache = RenderCache::new();

        cache.get_or_compute_width(1, || 10);
        cache.get_or_compute_width(2, || 20);
        cache.get_or_compute_width(3, || 30);

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get_or_compute_width(1, || 0), 10);
        assert_eq!(cache.get_or_compute_width(2, || 0), 20);
        assert_eq!(cache.get_or_compute_width(3, || 0), 30);
    }
}
