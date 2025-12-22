//! Memory pool for ExecutionContext reuse
//!
//! Reduces allocations by reusing ExecutionContext instances across workflow runs.

use crate::runner::ExecutionContext;
use std::sync::Mutex;

/// Thread-safe pool for ExecutionContext reuse
pub struct ContextPool {
    /// Pool of available contexts
    pool: Mutex<Vec<ExecutionContext>>,
    /// Maximum number of contexts to keep in the pool
    max_size: usize,
}

impl ContextPool {
    /// Create a new context pool
    pub fn new() -> Self {
        Self::with_capacity(8)
    }

    /// Create a pool with specified maximum capacity
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            pool: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }

    /// Get a context from the pool or create a new one
    pub fn get(&self) -> ExecutionContext {
        self.pool.lock().unwrap().pop().unwrap_or_default()
    }

    /// Return a context to the pool for reuse
    ///
    /// The context is cleared before being added to the pool
    pub fn return_context(&self, mut ctx: ExecutionContext) {
        // Clear all data from the context
        ctx.clear();

        // Try to add to the pool if not at capacity
        let mut pool = self.pool.lock().unwrap();
        if pool.len() < self.max_size {
            pool.push(ctx);
        }
        // Otherwise, let it be dropped
    }

    /// Get the current size of the pool
    pub fn size(&self) -> usize {
        self.pool.lock().unwrap().len()
    }

    /// Clear the pool, dropping all cached contexts
    pub fn clear(&self) {
        self.pool.lock().unwrap().clear();
    }

    /// Pre-warm the pool with empty contexts
    pub fn warm(&self, count: usize) {
        let mut pool = self.pool.lock().unwrap();
        let to_add = (self.max_size - pool.len()).min(count);

        for _ in 0..to_add {
            pool.push(ExecutionContext::new());
        }
    }
}

impl Default for ContextPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Global context pool for the application
static GLOBAL_POOL: once_cell::sync::Lazy<ContextPool> =
    once_cell::sync::Lazy::new(|| ContextPool::with_capacity(16));

/// Get a context from the global pool
pub fn get_context() -> ExecutionContext {
    GLOBAL_POOL.get()
}

/// Return a context to the global pool
pub fn return_context(ctx: ExecutionContext) {
    GLOBAL_POOL.return_context(ctx);
}

/// Pre-warm the global pool
pub fn warm_pool(count: usize) {
    GLOBAL_POOL.warm(count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_reuse() {
        let pool = ContextPool::with_capacity(2);

        // Get a context and use it
        let mut ctx = pool.get();
        ctx.set_output("task1", "output".to_string());

        // Return it to the pool
        pool.return_context(ctx);
        assert_eq!(pool.size(), 1);

        // Get it again - should be cleared
        let ctx = pool.get();
        assert_eq!(ctx.get_output("task1"), None);
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_pool_capacity() {
        let pool = ContextPool::with_capacity(2);

        // Return 3 contexts, but pool should only keep 2
        for i in 0..3 {
            let mut ctx = ExecutionContext::new();
            ctx.set_output("task", format!("output{}", i));
            pool.return_context(ctx);
        }

        assert_eq!(pool.size(), 2);
    }

    #[test]
    fn test_pool_warm() {
        let pool = ContextPool::with_capacity(5);

        // Warm with 3 contexts
        pool.warm(3);
        assert_eq!(pool.size(), 3);

        // Try to warm beyond capacity
        pool.warm(10);
        assert_eq!(pool.size(), 5);
    }

    #[test]
    fn test_global_pool() {
        // Use global pool functions
        warm_pool(2);

        let mut ctx = get_context();
        ctx.set_output("test", "value".to_string());

        return_context(ctx);

        // Get another context (might be the same one, cleared)
        let ctx = get_context();
        assert_eq!(ctx.get_output("test"), None);
    }
}
