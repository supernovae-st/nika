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

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::LazyLock;

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

// ═══════════════════════════════════════════════════════════════════════════════
// Task 2.3: parse_mentions() Regex Parser
// ═══════════════════════════════════════════════════════════════════════════════

/// Compiled regex for @mention parsing.
///
/// Pattern matches @ preceded by whitespace, punctuation, or start of string.
/// This prevents false positives like emails (user@123.com).
///
/// Capture groups:
/// - Group 1,2: Range @N..M (start, end)
/// - Group 3: Number @N
/// - Group 4: Keyword (last|all)
static MENTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Match @ at start of string or after whitespace/punctuation
    // Range must be checked before number (longer match wins)
    Regex::new(r"(?:^|[\s\[\](),:;])@(?:(\d+)\.\.(\d+)|(\d+)|(last|all))")
        .expect("Invalid mention regex")
});

/// Parse @mentions from text.
///
/// Supports:
/// - `@N` - Reference message N (1-indexed display, 0 is valid but resolves empty)
/// - `@N..M` - Range of messages from N to M (inclusive)
/// - `@last` - Most recent message
/// - `@all` - All messages
///
/// Does NOT match emails (user@123.com) due to prefix requirement.
///
/// # Examples
///
/// ```
/// use nika::binding::mention::{parse_mentions, Mention};
///
/// let mentions = parse_mentions("Look at @1 and @2");
/// assert_eq!(mentions, vec![Mention::Number(1), Mention::Number(2)]);
///
/// let mentions = parse_mentions("Continue from @last");
/// assert_eq!(mentions, vec![Mention::Last]);
/// ```
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    let mut mentions = Vec::new();

    for cap in MENTION_REGEX.captures_iter(text) {
        if let (Some(start), Some(end)) = (cap.get(1), cap.get(2)) {
            // Range: @N..M
            let start: u32 = start.as_str().parse().unwrap();
            let end: u32 = end.as_str().parse().unwrap();
            mentions.push(Mention::Range { start, end });
        } else if let Some(num) = cap.get(3) {
            // Number: @N
            let n: u32 = num.as_str().parse().unwrap();
            mentions.push(Mention::Number(n));
        } else if let Some(keyword) = cap.get(4) {
            // Keyword: @last or @all
            match keyword.as_str() {
                "last" => mentions.push(Mention::Last),
                "all" => mentions.push(Mention::All),
                _ => {}
            }
        }
    }

    mentions
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.3: parse_mentions() tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_mentions_number() {
        let text = "Look at @1 and @2";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Number(1), Mention::Number(2)]);
    }

    #[test]
    fn test_parse_mentions_at_start() {
        let text = "@1 is the first message";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Number(1)]);
    }

    #[test]
    fn test_parse_mentions_last() {
        let text = "Continue from @last";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Last]);
    }

    #[test]
    fn test_parse_mentions_all() {
        let text = "Summarize @all";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::All]);
    }

    #[test]
    fn test_parse_mentions_range() {
        let text = "Combine @1..3";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Range { start: 1, end: 3 }]);
    }

    #[test]
    fn test_parse_mentions_mixed() {
        let text = "Based on @1, @last, and @all";
        let mentions = parse_mentions(text);
        assert_eq!(
            mentions,
            vec![Mention::Number(1), Mention::Last, Mention::All,]
        );
    }

    #[test]
    fn test_parse_mentions_no_matches() {
        let text = "No mentions here";
        let mentions = parse_mentions(text);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_parse_mentions_after_punctuation() {
        let text = "See (@1) and [@2]";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Number(1), Mention::Number(2)]);
    }

    #[test]
    fn test_parse_mentions_email_not_matched() {
        // Emails should NOT be parsed as mentions
        let text = "Contact user@123.com for help";
        let mentions = parse_mentions(text);
        assert!(mentions.is_empty(), "Email should not be parsed as mention");
    }

    #[test]
    fn test_parse_mentions_mixed_with_email() {
        // Standalone @123 should work, but email @456 should not
        let text = "Check @123 after emailing user@456.com";
        let mentions = parse_mentions(text);
        assert_eq!(mentions, vec![Mention::Number(123)]);
    }
}
