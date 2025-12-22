//! SmartString implementation for efficient ID storage
//!
//! Task IDs are typically short (8-32 chars), so we can avoid heap allocation
//! using inline storage for small strings.

use std::fmt;
use std::ops::Deref;

/// SmartString with inline storage for small strings
///
/// - Strings <= 31 chars are stored inline (stack)
/// - Strings > 31 chars use heap allocation
///
/// This is optimized for task IDs which are typically:
/// - "task-1", "step-2" (6-7 chars)
/// - "analyze-data", "validate-input" (12-15 chars)
/// - UUID-style IDs (36 chars including dashes)
#[derive(Clone)]
pub enum SmartString {
    /// Inline storage for strings up to 31 bytes
    Inline {
        /// Actual length of the string
        len: u8,
        /// Fixed-size buffer for inline storage
        buf: [u8; 31],
    },
    /// Heap-allocated for larger strings
    Heap(String),
}

impl SmartString {
    /// Maximum size for inline storage
    const INLINE_CAPACITY: usize = 31;

    /// Create a new SmartString
    pub fn new(s: &str) -> Self {
        let bytes = s.as_bytes();
        let len = bytes.len();

        if len <= Self::INLINE_CAPACITY {
            // Use inline storage
            let mut buf = [0u8; 31];
            buf[..len].copy_from_slice(bytes);
            SmartString::Inline {
                len: len as u8,
                buf,
            }
        } else {
            // Use heap allocation
            SmartString::Heap(s.to_string())
        }
    }

    /// Get the string slice
    pub fn as_str(&self) -> &str {
        match self {
            SmartString::Inline { len, buf } => {
                // SAFETY: We only store valid UTF-8 in the buffer
                unsafe { std::str::from_utf8_unchecked(&buf[..*len as usize]) }
            }
            SmartString::Heap(s) => s.as_str(),
        }
    }

    /// Check if this is using inline storage
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self, SmartString::Inline { .. })
    }

    /// Get the length of the string
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            SmartString::Inline { len, .. } => *len as usize,
            SmartString::Heap(s) => s.len(),
        }
    }

    /// Check if the string is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Deref for SmartString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for SmartString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for SmartString {
    fn from(s: &str) -> Self {
        SmartString::new(s)
    }
}

impl From<String> for SmartString {
    fn from(s: String) -> Self {
        // Try to use inline storage if possible
        if s.len() <= SmartString::INLINE_CAPACITY {
            SmartString::new(&s)
        } else {
            SmartString::Heap(s)
        }
    }
}

impl fmt::Display for SmartString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for SmartString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmartString::Inline { len, .. } => {
                write!(f, "SmartString::Inline({}, \"{}\")", len, self.as_str())
            }
            SmartString::Heap(s) => write!(f, "SmartString::Heap(\"{}\")", s),
        }
    }
}

impl PartialEq for SmartString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for SmartString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SmartString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for SmartString {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SmartString {}

impl std::hash::Hash for SmartString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

/// Implement Borrow<str> so SmartString can be used as HashMap key
impl std::borrow::Borrow<str> for SmartString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_storage() {
        let s = SmartString::new("task-1");
        assert!(s.is_inline());
        assert_eq!(s.as_str(), "task-1");
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn test_heap_storage() {
        let long_str = "a".repeat(32); // 32 chars, exceeds inline capacity
        let s = SmartString::new(&long_str);
        assert!(!s.is_inline());
        assert_eq!(s.as_str(), &long_str);
        assert_eq!(s.len(), 32);
    }

    #[test]
    fn test_boundary_case() {
        let str_31 = "a".repeat(31); // Exactly at inline capacity
        let s = SmartString::new(&str_31);
        assert!(s.is_inline());
        assert_eq!(s.len(), 31);
    }

    #[test]
    fn test_from_string() {
        let short = String::from("short");
        let s = SmartString::from(short);
        assert!(s.is_inline());
        assert_eq!(s.as_str(), "short");

        let long = "x".repeat(40);
        let s = SmartString::from(long.clone());
        assert!(!s.is_inline());
        assert_eq!(s.as_str(), &long);
    }

    #[test]
    fn test_equality() {
        let s1 = SmartString::new("test");
        let s2 = SmartString::new("test");
        let s3 = SmartString::new("different");

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
        assert_eq!(s1, "test");
        assert_eq!(s1, String::from("test"));
    }

    #[test]
    fn test_hash() {
        use std::collections::HashMap;

        let mut map = HashMap::new();
        map.insert(SmartString::new("key1"), "value1");
        map.insert(SmartString::new("key2"), "value2");

        assert_eq!(map.get("key1"), Some(&"value1"));
        assert_eq!(map.get("key2"), Some(&"value2"));
    }

    #[test]
    fn test_typical_task_ids() {
        // Common task ID patterns
        let ids = vec![
            "task-1",
            "step-2",
            "analyze",
            "validate-input",
            "process-data",
            "generate-report",
            "user-auth-check",
            "db-migration-v2",
        ];

        for id in ids {
            let s = SmartString::new(id);
            assert!(s.is_inline(), "{} should use inline storage", id);
        }
    }

    #[test]
    fn test_uuid_style() {
        // UUID-style IDs are 36 chars with dashes, so they'll use heap
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let s = SmartString::new(uuid);
        assert!(!s.is_inline());
        assert_eq!(s.as_str(), uuid);
    }
}
