// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Interned identifiers for the Analyzed AST.
//!
//! These are u32 indices that reference into lookup tables,
//! providing O(1) comparison and hashing while keeping string
//! data deduplicated.

use rustc_hash::FxHashMap;
use std::fmt;

/// Interned task identifier.
///
/// Tasks are identified by a u32 index into the workflow's task table.
/// This enables O(1) comparison and efficient storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u32);

impl TaskId {
    /// Create a new task ID.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Get the raw index.
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "task#{}", self.0)
    }
}

/// Task name lookup table.
///
/// Bidirectional mapping between task names and TaskIds.
/// Uses HashMap for O(1) name→id lookup.
#[derive(Debug, Clone, Default)]
pub struct TaskTable {
    /// Task names indexed by TaskId.
    names: Vec<String>,
    /// Reverse lookup (name → TaskId) for O(1) lookup.
    index: FxHashMap<String, TaskId>,
}

impl TaskTable {
    /// Create a new empty task table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a task name, returning its ID.
    ///
    /// Does NOT check for duplicates - use `try_insert()` for safe insertion.
    pub fn insert(&mut self, name: &str) -> TaskId {
        let id = TaskId::new(self.names.len() as u32);
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), id);
        id
    }

    /// Try to insert a task name, returning None if duplicate.
    ///
    /// Returns `Some(TaskId)` on success, `None` if name already exists.
    pub fn try_insert(&mut self, name: &str) -> Option<TaskId> {
        if self.index.contains_key(name) {
            None
        } else {
            Some(self.insert(name))
        }
    }

    /// Check if a task name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Look up a task ID by name. O(1) operation.
    pub fn get_id(&self, name: &str) -> Option<TaskId> {
        self.index.get(name).copied()
    }

    /// Get a task name by ID.
    pub fn get_name(&self, id: TaskId) -> Option<&str> {
        self.names.get(id.0 as usize).map(|s| s.as_str())
    }

    /// Get the number of tasks.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Iterate over all (TaskId, name) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (TaskId, &str)> {
        self.names
            .iter()
            .enumerate()
            .map(|(i, name)| (TaskId::new(i as u32), name.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id() {
        let id = TaskId::new(42);
        assert_eq!(id.index(), 42);
        assert_eq!(format!("{}", id), "task#42");
    }

    #[test]
    fn test_task_table() {
        let mut table = TaskTable::new();
        assert!(table.is_empty());

        let id1 = table.insert("task1");
        let id2 = table.insert("task2");

        assert_eq!(id1.index(), 0);
        assert_eq!(id2.index(), 1);

        assert_eq!(table.get_id("task1"), Some(id1));
        assert_eq!(table.get_id("task2"), Some(id2));
        assert_eq!(table.get_id("unknown"), None);

        assert_eq!(table.get_name(id1), Some("task1"));
        assert_eq!(table.get_name(id2), Some("task2"));
    }

    #[test]
    fn test_task_table_iter() {
        let mut table = TaskTable::new();
        table.insert("a");
        table.insert("b");
        table.insert("c");

        let pairs: Vec<_> = table.iter().collect();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0].1, "a");
        assert_eq!(pairs[1].1, "b");
        assert_eq!(pairs[2].1, "c");
    }

    #[test]
    fn test_task_table_try_insert() {
        let mut table = TaskTable::new();

        // First insert succeeds
        let id1 = table.try_insert("task1");
        assert!(id1.is_some());
        assert_eq!(id1.unwrap().index(), 0);

        // Second different insert succeeds
        let id2 = table.try_insert("task2");
        assert!(id2.is_some());
        assert_eq!(id2.unwrap().index(), 1);

        // Duplicate insert fails
        let id3 = table.try_insert("task1");
        assert!(id3.is_none());

        // Table still has only 2 entries
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_task_table_contains() {
        let mut table = TaskTable::new();
        table.insert("existing");

        assert!(table.contains("existing"));
        assert!(!table.contains("missing"));
    }

    #[test]
    fn test_task_table_o1_lookup() {
        let mut table = TaskTable::new();
        // Insert many tasks to verify O(1) behavior
        for i in 0..1000 {
            table.insert(&format!("task_{}", i));
        }

        // Lookup should be O(1) with HashMap
        assert_eq!(table.get_id("task_500").map(|id| id.index()), Some(500));
        assert_eq!(table.get_id("task_999").map(|id| id.index()), Some(999));
        assert_eq!(table.get_id("nonexistent"), None);
    }
}
