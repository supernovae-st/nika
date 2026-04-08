// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! String interning — thin wrapper around `Arc::from`.
//!
//! Originally used a global DashMap for deduplication, but audit showed
//! DashMap overhead (80 bytes/entry) exceeded dedup savings (30 bytes/entry).
//! Now `intern()` is a trivial `Arc::from` wrapper, keeping the call sites
//! unchanged while letting Runner drop naturally free the strings.

use std::sync::Arc;

/// Create an `Arc<str>` from a string slice.
///
/// This is a thin wrapper that keeps all call sites consistent.
/// Previously interned via a global DashMap; now just allocates directly.
#[inline]
pub fn intern(s: &str) -> Arc<str> {
    Arc::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_arc_with_correct_content() {
        let a = intern("task_a");
        assert_eq!(&*a, "task_a");
    }

    #[test]
    fn intern_different_strings_different_content() {
        let a = intern("task_a");
        let b = intern("task_b");
        assert_ne!(&*a, &*b);
    }

    #[test]
    fn intern_clone_is_cheap() {
        let a = intern("task_a");
        let b = Arc::clone(&a);
        assert!(Arc::ptr_eq(&a, &b));
    }
}
