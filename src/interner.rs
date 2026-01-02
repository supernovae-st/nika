//! String interning for recurring task IDs
//!
//! Ensures each unique task_id string is stored only once in memory.
//! Uses DashMap for lock-free concurrent access.
//!
//! Performance benefits:
//! - Memory: Single allocation per unique string
//! - Comparison: Pointer equality instead of string comparison (O(1) vs O(n))
//! - Cloning: Arc::clone is O(1), no string copy

use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

/// Global string interner (thread-safe, lock-free)
static INTERNER: Lazy<Interner> = Lazy::new(Interner::new);

/// Thread-safe string interner using DashMap
pub struct Interner {
    /// Map from string content to interned Arc<str>
    strings: DashMap<Arc<str>, ()>,
}

impl Interner {
    /// Create a new interner
    pub fn new() -> Self {
        Self {
            strings: DashMap::new(),
        }
    }

    /// Intern a string, returning a shared Arc<str>
    ///
    /// If the string was already interned, returns the existing Arc.
    /// Otherwise, creates a new Arc and stores it.
    pub fn intern(&self, s: &str) -> Arc<str> {
        // Fast path: check if already interned
        let key: Arc<str> = Arc::from(s);

        // DashMap entry API: get_or_insert pattern
        if let Some(existing) = self.strings.get(&key) {
            return Arc::clone(existing.key());
        }

        // Insert and return the Arc
        self.strings.insert(Arc::clone(&key), ());
        key
    }

    /// Intern an already-Arc'd string
    #[inline]
    pub fn intern_arc(&self, s: Arc<str>) -> Arc<str> {
        if let Some(existing) = self.strings.get(&s) {
            return Arc::clone(existing.key());
        }

        self.strings.insert(Arc::clone(&s), ());
        s
    }

    /// Number of interned strings
    #[allow(dead_code)] // Used in tests
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if empty
    #[allow(dead_code)] // Used in tests
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Default for Interner {
    fn default() -> Self {
        Self::new()
    }
}

/// Intern a task_id string using the global interner
#[inline]
pub fn intern(s: &str) -> Arc<str> {
    INTERNER.intern(s)
}

/// Intern an already-Arc'd string using the global interner
#[inline]
#[allow(dead_code)] // Used in tests and future optimization paths
pub fn intern_arc(s: Arc<str>) -> Arc<str> {
    INTERNER.intern_arc(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_same_arc_for_same_string() {
        let interner = Interner::new();

        let a1 = interner.intern("task_a");
        let a2 = interner.intern("task_a");

        // Same pointer (not just equal content)
        assert!(Arc::ptr_eq(&a1, &a2));
    }

    #[test]
    fn intern_different_strings_different_arcs() {
        let interner = Interner::new();

        let a = interner.intern("task_a");
        let b = interner.intern("task_b");

        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn intern_arc_reuses_existing() {
        let interner = Interner::new();

        let a1 = interner.intern("task_a");
        let a2 = interner.intern_arc(Arc::from("task_a"));

        assert!(Arc::ptr_eq(&a1, &a2));
    }

    #[test]
    fn global_intern_works() {
        let a1 = intern("global_test");
        let a2 = intern("global_test");

        assert!(Arc::ptr_eq(&a1, &a2));
    }

    #[test]
    fn concurrent_intern_is_safe() {
        use std::thread;

        let interner = Arc::new(Interner::new());
        let mut handles = vec![];

        for i in 0..10 {
            let interner = Arc::clone(&interner);
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    interner.intern(&format!("task_{}_{}", i, j));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Should have 1000 unique strings
        assert_eq!(interner.len(), 1000);
    }

    #[test]
    fn concurrent_same_string_returns_same_arc() {
        use std::thread;
        use std::sync::mpsc;

        let interner = Arc::new(Interner::new());
        let (tx, rx) = mpsc::channel();

        for _ in 0..10 {
            let interner = Arc::clone(&interner);
            let tx = tx.clone();
            thread::spawn(move || {
                let result = interner.intern("shared_task");
                tx.send(result).unwrap();
            });
        }

        drop(tx);

        let results: Vec<Arc<str>> = rx.iter().collect();
        assert_eq!(results.len(), 10);

        // All should be the same Arc
        let first = &results[0];
        for result in &results[1..] {
            assert!(Arc::ptr_eq(first, result));
        }
    }
}
