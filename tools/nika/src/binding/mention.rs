//! Mention Module - @reference parsing for Chat-as-DAG (v0.9.2)
//!
//! Parses @mentions in chat messages and converts them to WiringSpec bindings.
//!
//! # Syntax
//!
//! | Pattern | Description |
//! |---------|-------------|
//! | `@N` | Reference message #N (msg-001) |
//! | `@last` | Reference the last message |
//! | `@all` | Reference all previous messages |
//! | `@N..M` | Reference messages N through M (range) |
//! | `//` | Parallel marker (no dependency on previous message) |
//!
//! # Example
//!
//! ```text
//! @1 USER: Analyse ce fichier
//! @2 ASSISTANT: Voici l'analyse...
//! @3 USER: Traduis @2 en français    ◄── Reference @2
//! @4 USER: // Independent task       ◄── Parallel (no edge from @3)
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// A reference to a previous chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mention {
    /// @N - specific message number (1-indexed)
    Number(u32),
    /// @last - most recent message
    Last,
    /// @all - all previous messages
    All,
    /// @N..M - range of messages (inclusive)
    Range { start: u32, end: u32 },
}

impl fmt::Display for Mention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mention::Number(n) => write!(f, "@{}", n),
            Mention::Last => write!(f, "@last"),
            Mention::All => write!(f, "@all"),
            Mention::Range { start, end } => write!(f, "@{}..{}", start, end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.1: Module existence tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mention_module_exists() {
        let _: Mention = Mention::Number(1);
    }

    #[test]
    fn test_mention_number_stores_value() {
        let m = Mention::Number(42);
        if let Mention::Number(n) = m {
            assert_eq!(n, 42);
        } else {
            panic!("Expected Number variant");
        }
    }

    #[test]
    fn test_mention_range_stores_bounds() {
        let m = Mention::Range { start: 1, end: 5 };
        if let Mention::Range { start, end } = m {
            assert_eq!(start, 1);
            assert_eq!(end, 5);
        } else {
            panic!("Expected Range variant");
        }
    }

    #[test]
    fn test_mention_equality() {
        assert_eq!(Mention::Number(1), Mention::Number(1));
        assert_ne!(Mention::Number(1), Mention::Number(2));
        assert_eq!(Mention::Last, Mention::Last);
        assert_eq!(Mention::All, Mention::All);
    }

    #[test]
    fn test_mention_clone() {
        let m = Mention::Range { start: 1, end: 10 };
        let cloned = m.clone();
        assert_eq!(m, cloned);
    }

    #[test]
    fn test_mention_serialization() {
        let m = Mention::Number(5);
        let json = serde_json::to_string(&m).unwrap();
        let restored: Mention = serde_json::from_str(&json).unwrap();
        assert_eq!(m, restored);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.2: Display implementation tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mention_display_number() {
        assert_eq!(format!("{}", Mention::Number(1)), "@1");
        assert_eq!(format!("{}", Mention::Number(42)), "@42");
        assert_eq!(format!("{}", Mention::Number(999)), "@999");
    }

    #[test]
    fn test_mention_display_last() {
        assert_eq!(format!("{}", Mention::Last), "@last");
    }

    #[test]
    fn test_mention_display_all() {
        assert_eq!(format!("{}", Mention::All), "@all");
    }

    #[test]
    fn test_mention_display_range() {
        assert_eq!(format!("{}", Mention::Range { start: 1, end: 3 }), "@1..3");
        assert_eq!(
            format!("{}", Mention::Range { start: 10, end: 20 }),
            "@10..20"
        );
    }
}
