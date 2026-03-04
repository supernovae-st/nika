//! Interned identifiers for the Analyzed AST.
//!
//! These are u32 indices that reference into lookup tables,
//! providing O(1) comparison and hashing while keeping string
//! data deduplicated.

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

/// Interned flow definition identifier.
///
/// Flow definitions (in the `flows:` section) are identified by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowDefId(pub u32);

impl FlowDefId {
    /// Create a new flow definition ID.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Get the raw index.
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for FlowDefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "flow#{}", self.0)
    }
}

/// Interned MCP server identifier.
///
/// MCP servers configured in the `mcp:` section are identified by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct McpServerId(pub u32);

impl McpServerId {
    /// Create a new MCP server ID.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Get the raw index.
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Display for McpServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mcp#{}", self.0)
    }
}

/// A string table for interning.
///
/// Maps indices to their string values.
#[derive(Debug, Clone, Default)]
pub struct StringTable {
    /// The interned strings.
    strings: Vec<String>,
}

impl StringTable {
    /// Create a new empty string table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a string, returning its index.
    ///
    /// If the string is already interned, returns the existing index.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.strings.iter().position(|x| x == s) {
            idx as u32
        } else {
            let idx = self.strings.len() as u32;
            self.strings.push(s.to_string());
            idx
        }
    }

    /// Get a string by its index.
    pub fn get(&self, idx: u32) -> Option<&str> {
        self.strings.get(idx as usize).map(|s| s.as_str())
    }

    /// Get the number of interned strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// Task name lookup table.
///
/// Bidirectional mapping between task names and TaskIds.
#[derive(Debug, Clone, Default)]
pub struct TaskTable {
    /// Task names indexed by TaskId.
    names: Vec<String>,
}

impl TaskTable {
    /// Create a new empty task table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a task name, returning its ID.
    ///
    /// Does NOT check for duplicates - caller must ensure uniqueness.
    pub fn insert(&mut self, name: &str) -> TaskId {
        let id = TaskId::new(self.names.len() as u32);
        self.names.push(name.to_string());
        id
    }

    /// Look up a task ID by name.
    pub fn get_id(&self, name: &str) -> Option<TaskId> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| TaskId::new(i as u32))
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
    fn test_string_table() {
        let mut table = StringTable::new();
        assert!(table.is_empty());

        let idx1 = table.intern("hello");
        let idx2 = table.intern("world");
        let idx3 = table.intern("hello"); // duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // same as idx1

        assert_eq!(table.get(0), Some("hello"));
        assert_eq!(table.get(1), Some("world"));
        assert_eq!(table.get(99), None);
        assert_eq!(table.len(), 2);
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
}
