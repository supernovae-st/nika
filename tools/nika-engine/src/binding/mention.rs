// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mention Module - @reference parsing for Chat-as-DAG
//!
//! Parses @mentions in chat messages and converts them to BindingSpec bindings.
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
//! @1 USER: Analyze this file
//! @2 ASSISTANT: Here is the analysis...
//! @3 USER: Translate @2 to French    ◄── Reference @2
//! @4 USER: // Independent task       ◄── Parallel (no edge from @3)
//! ```

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::LazyLock;

use super::{BindingEntry, BindingSpec};

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
/// use nika_engine::binding::mention::{parse_mentions, Mention};
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
            let Ok(start) = start.as_str().parse::<u32>() else {
                continue;
            };
            let Ok(end) = end.as_str().parse::<u32>() else {
                continue;
            };
            mentions.push(Mention::Range { start, end });
        } else if let Some(num) = cap.get(3) {
            // Number: @N
            let Ok(n) = num.as_str().parse::<u32>() else {
                continue;
            };
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

// ═══════════════════════════════════════════════════════════════════════════════
// Task 2.7: Parallel Marker Detection
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if text starts with the parallel marker `//`.
///
/// A message starting with `//` indicates it has no dependency on the previous message
/// (parallel execution). The marker can have leading whitespace.
///
/// # Examples
///
/// ```
/// use nika_engine::binding::mention::has_parallel_marker;
///
/// assert!(has_parallel_marker("// Independent task"));
/// assert!(has_parallel_marker("  // Also parallel"));
/// assert!(!has_parallel_marker("Normal message"));
/// assert!(!has_parallel_marker("@1 Reference")); // mentions, not parallel
/// ```
pub fn has_parallel_marker(text: &str) -> bool {
    text.trim_start().starts_with("//")
}

/// Strip the parallel marker from text if present.
///
/// Returns the text content after the `//` marker, trimmed.
/// If no marker, returns the original text unchanged.
///
/// # Examples
///
/// ```
/// use nika_engine::binding::mention::strip_parallel_marker;
///
/// assert_eq!(strip_parallel_marker("// Task"), "Task");
/// assert_eq!(strip_parallel_marker("  //  Parallel work"), "Parallel work");
/// assert_eq!(strip_parallel_marker("Normal message"), "Normal message");
/// ```
pub fn strip_parallel_marker(text: &str) -> &str {
    let trimmed = text.trim_start();
    if let Some(stripped) = trimmed.strip_prefix("//") {
        stripped.trim_start()
    } else {
        text
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Task 2.4-2.6: resolve_mention() - Mention → Message Indices
// ═══════════════════════════════════════════════════════════════════════════════

/// Resolved message indices from a mention.
///
/// All indices are 1-indexed (matching @N syntax).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedMention {
    /// Single message index
    Single(u32),
    /// Multiple message indices (for @all and ranges)
    Multiple(Vec<u32>),
    /// Empty resolution (no messages to reference)
    Empty,
}

/// Error when resolving a mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionResolutionError {
    /// Referenced message doesn't exist
    MessageNotFound { index: u32, max: u32 },
    /// Invalid range (start > end)
    InvalidRange { start: u32, end: u32 },
    /// No messages to reference for @last/@all
    NoMessages,
}

impl std::fmt::Display for MentionResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageNotFound { index, max } => {
                write!(f, "Message @{} not found (max: @{})", index, max)
            }
            Self::InvalidRange { start, end } => {
                write!(f, "Invalid range @{}..{} (start > end)", start, end)
            }
            Self::NoMessages => write!(f, "No messages to reference"),
        }
    }
}

impl std::error::Error for MentionResolutionError {}

/// Resolve a mention to message indices.
///
/// # Arguments
/// * `mention` - The mention to resolve
/// * `message_count` - Total number of messages in the chat
///
/// # Returns
/// * `Ok(ResolvedMention)` - Resolved indices (1-indexed)
/// * `Err(MentionResolutionError)` - Resolution failed
///
/// # Examples
///
/// ```
/// use nika_engine::binding::mention::{resolve_mention, Mention, ResolvedMention};
///
/// // @last with 3 messages → @3
/// let result = resolve_mention(&Mention::Last, 3);
/// assert_eq!(result, Ok(ResolvedMention::Single(3)));
///
/// // @2 with 3 messages → @2
/// let result = resolve_mention(&Mention::Number(2), 3);
/// assert_eq!(result, Ok(ResolvedMention::Single(2)));
/// ```
pub fn resolve_mention(
    mention: &Mention,
    message_count: u32,
) -> Result<ResolvedMention, MentionResolutionError> {
    match mention {
        Mention::Last => {
            if message_count == 0 {
                Err(MentionResolutionError::NoMessages)
            } else {
                Ok(ResolvedMention::Single(message_count))
            }
        }
        Mention::Number(n) => {
            if *n == 0 || *n > message_count {
                Err(MentionResolutionError::MessageNotFound {
                    index: *n,
                    max: message_count,
                })
            } else {
                Ok(ResolvedMention::Single(*n))
            }
        }
        Mention::All => {
            if message_count == 0 {
                Ok(ResolvedMention::Empty)
            } else {
                Ok(ResolvedMention::Multiple((1..=message_count).collect()))
            }
        }
        Mention::Range { start, end } => {
            if start > end {
                return Err(MentionResolutionError::InvalidRange {
                    start: *start,
                    end: *end,
                });
            }
            if *start == 0 || *end > message_count {
                return Err(MentionResolutionError::MessageNotFound {
                    index: if *start == 0 { 0 } else { *end },
                    max: message_count,
                });
            }
            Ok(ResolvedMention::Multiple((*start..=*end).collect()))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Task 2.8: mentions_to_bindings() - Mentions → BindingSpec
// ═══════════════════════════════════════════════════════════════════════════════

/// Convert resolved mentions to BindingSpec bindings.
///
/// Creates a binding for each referenced message with:
/// - Alias: `ref_N` where N is the message number (1-indexed)
/// - Path: `msg-NNN.output` following ChatWorkflow message ID format
///
/// # Arguments
/// * `resolved` - Resolved mention indices
///
/// # Returns
/// BindingSpec with bindings for each referenced message
///
/// # Examples
///
/// ```
/// use nika_engine::binding::mention::{mentions_to_bindings, ResolvedMention};
///
/// // Single reference @2
/// let spec = mentions_to_bindings(&ResolvedMention::Single(2));
/// assert!(spec.contains_key("ref_2"));
///
/// // Multiple references @1..3
/// let spec = mentions_to_bindings(&ResolvedMention::Multiple(vec![1, 2, 3]));
/// assert!(spec.contains_key("ref_1"));
/// assert!(spec.contains_key("ref_2"));
/// assert!(spec.contains_key("ref_3"));
/// ```
pub fn mentions_to_bindings(resolved: &ResolvedMention) -> BindingSpec {
    let mut spec = BindingSpec::default();

    match resolved {
        ResolvedMention::Single(n) => {
            let alias = format!("ref_{}", n);
            let path = format!("msg-{:03}.output", n);
            spec.insert(alias, BindingEntry::new(path));
        }
        ResolvedMention::Multiple(indices) => {
            for n in indices {
                let alias = format!("ref_{}", n);
                let path = format!("msg-{:03}.output", n);
                spec.insert(alias, BindingEntry::new(path));
            }
        }
        ResolvedMention::Empty => {
            // No bindings for empty resolution
        }
    }

    spec
}

/// Convert all mentions in text to BindingSpec.
///
/// Parses, resolves, and converts mentions in one step.
///
/// # Arguments
/// * `text` - Text containing @mentions
/// * `message_count` - Current message count for resolution
///
/// # Returns
/// Combined BindingSpec for all mentions in text
pub fn text_to_bindings(
    text: &str,
    message_count: u32,
) -> Result<BindingSpec, MentionResolutionError> {
    let mut spec = BindingSpec::default();

    for mention in parse_mentions(text) {
        let resolved = resolve_mention(&mention, message_count)?;
        spec.extend(mentions_to_bindings(&resolved));
    }

    Ok(spec)
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

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.4: resolve_mention() for @last
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_resolve_last_with_messages() {
        // @last with 3 messages → @3
        let result = resolve_mention(&Mention::Last, 3);
        assert_eq!(result, Ok(ResolvedMention::Single(3)));
    }

    #[test]
    fn test_resolve_last_with_one_message() {
        // @last with 1 message → @1
        let result = resolve_mention(&Mention::Last, 1);
        assert_eq!(result, Ok(ResolvedMention::Single(1)));
    }

    #[test]
    fn test_resolve_last_with_no_messages() {
        // @last with 0 messages → error
        let result = resolve_mention(&Mention::Last, 0);
        assert_eq!(result, Err(MentionResolutionError::NoMessages));
    }

    #[test]
    fn test_resolve_number_valid() {
        // @2 with 3 messages → @2
        let result = resolve_mention(&Mention::Number(2), 3);
        assert_eq!(result, Ok(ResolvedMention::Single(2)));
    }

    #[test]
    fn test_resolve_number_first_message() {
        // @1 with 5 messages → @1
        let result = resolve_mention(&Mention::Number(1), 5);
        assert_eq!(result, Ok(ResolvedMention::Single(1)));
    }

    #[test]
    fn test_resolve_number_out_of_bounds() {
        // @5 with 3 messages → error
        let result = resolve_mention(&Mention::Number(5), 3);
        assert_eq!(
            result,
            Err(MentionResolutionError::MessageNotFound { index: 5, max: 3 })
        );
    }

    #[test]
    fn test_resolve_number_zero() {
        // @0 is invalid (1-indexed)
        let result = resolve_mention(&Mention::Number(0), 3);
        assert_eq!(
            result,
            Err(MentionResolutionError::MessageNotFound { index: 0, max: 3 })
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.5: resolve_mention() for @all
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_resolve_all_with_messages() {
        // @all with 3 messages → [1, 2, 3]
        let result = resolve_mention(&Mention::All, 3);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![1, 2, 3])));
    }

    #[test]
    fn test_resolve_all_with_one_message() {
        // @all with 1 message → [1]
        let result = resolve_mention(&Mention::All, 1);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![1])));
    }

    #[test]
    fn test_resolve_all_with_no_messages() {
        // @all with 0 messages → Empty (not error)
        let result = resolve_mention(&Mention::All, 0);
        assert_eq!(result, Ok(ResolvedMention::Empty));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.6: resolve_mention() for @N..M range
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_resolve_range_valid() {
        // @1..3 with 5 messages → [1, 2, 3]
        let result = resolve_mention(&Mention::Range { start: 1, end: 3 }, 5);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![1, 2, 3])));
    }

    #[test]
    fn test_resolve_range_single() {
        // @2..2 with 3 messages → [2]
        let result = resolve_mention(&Mention::Range { start: 2, end: 2 }, 3);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![2])));
    }

    #[test]
    fn test_resolve_range_full() {
        // @1..3 with 3 messages → [1, 2, 3]
        let result = resolve_mention(&Mention::Range { start: 1, end: 3 }, 3);
        assert_eq!(result, Ok(ResolvedMention::Multiple(vec![1, 2, 3])));
    }

    #[test]
    fn test_resolve_range_out_of_bounds() {
        // @1..5 with 3 messages → error
        let result = resolve_mention(&Mention::Range { start: 1, end: 5 }, 3);
        assert_eq!(
            result,
            Err(MentionResolutionError::MessageNotFound { index: 5, max: 3 })
        );
    }

    #[test]
    fn test_resolve_range_invalid_order() {
        // @3..1 → error (start > end)
        let result = resolve_mention(&Mention::Range { start: 3, end: 1 }, 5);
        assert_eq!(
            result,
            Err(MentionResolutionError::InvalidRange { start: 3, end: 1 })
        );
    }

    #[test]
    fn test_resolve_range_zero_start() {
        // @0..3 → error (0 is invalid)
        let result = resolve_mention(&Mention::Range { start: 0, end: 3 }, 5);
        assert_eq!(
            result,
            Err(MentionResolutionError::MessageNotFound { index: 0, max: 5 })
        );
    }

    #[test]
    fn test_error_display() {
        let err = MentionResolutionError::MessageNotFound { index: 5, max: 3 };
        assert_eq!(format!("{}", err), "Message @5 not found (max: @3)");

        let err = MentionResolutionError::InvalidRange { start: 3, end: 1 };
        assert_eq!(format!("{}", err), "Invalid range @3..1 (start > end)");

        let err = MentionResolutionError::NoMessages;
        assert_eq!(format!("{}", err), "No messages to reference");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.7: Parallel marker tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_has_parallel_marker_basic() {
        assert!(has_parallel_marker("// Independent task"));
        assert!(has_parallel_marker("//Task"));
    }

    #[test]
    fn test_has_parallel_marker_with_whitespace() {
        assert!(has_parallel_marker("  // Also parallel"));
        assert!(has_parallel_marker("\t// Tab prefixed"));
    }

    #[test]
    fn test_has_parallel_marker_false() {
        assert!(!has_parallel_marker("Normal message"));
        assert!(!has_parallel_marker("@1 Reference"));
        assert!(!has_parallel_marker("/ Single slash"));
        assert!(!has_parallel_marker(""));
    }

    #[test]
    fn test_has_parallel_marker_not_url() {
        // URLs start with protocol://, not just //
        assert!(!has_parallel_marker("https://example.com"));
        assert!(!has_parallel_marker("http://localhost"));
    }

    #[test]
    fn test_strip_parallel_marker_basic() {
        assert_eq!(strip_parallel_marker("// Task"), "Task");
        assert_eq!(strip_parallel_marker("//Task"), "Task");
    }

    #[test]
    fn test_strip_parallel_marker_with_whitespace() {
        assert_eq!(
            strip_parallel_marker("  //  Parallel work"),
            "Parallel work"
        );
        assert_eq!(strip_parallel_marker("\t// Tab"), "Tab");
    }

    #[test]
    fn test_strip_parallel_marker_no_marker() {
        assert_eq!(strip_parallel_marker("Normal message"), "Normal message");
        assert_eq!(strip_parallel_marker("@1 Reference"), "@1 Reference");
    }

    #[test]
    fn test_strip_parallel_marker_empty() {
        assert_eq!(strip_parallel_marker("//"), "");
        assert_eq!(strip_parallel_marker("//  "), "");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Task 2.8: mentions_to_bindings() tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mentions_to_bindings_single() {
        let spec = mentions_to_bindings(&ResolvedMention::Single(2));
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_2"));
        assert_eq!(spec["ref_2"].path, "msg-002.output");
    }

    #[test]
    fn test_mentions_to_bindings_single_large_number() {
        let spec = mentions_to_bindings(&ResolvedMention::Single(123));
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_123"));
        assert_eq!(spec["ref_123"].path, "msg-123.output");
    }

    #[test]
    fn test_mentions_to_bindings_multiple() {
        let spec = mentions_to_bindings(&ResolvedMention::Multiple(vec![1, 2, 3]));
        assert_eq!(spec.len(), 3);
        assert_eq!(spec["ref_1"].path, "msg-001.output");
        assert_eq!(spec["ref_2"].path, "msg-002.output");
        assert_eq!(spec["ref_3"].path, "msg-003.output");
    }

    #[test]
    fn test_mentions_to_bindings_empty() {
        let spec = mentions_to_bindings(&ResolvedMention::Empty);
        assert!(spec.is_empty());
    }

    #[test]
    fn test_mentions_to_bindings_entry_is_eager() {
        // Verify bindings are eager (not lazy)
        let spec = mentions_to_bindings(&ResolvedMention::Single(1));
        assert!(!spec["ref_1"].lazy);
        assert!(spec["ref_1"].default.is_none());
    }

    #[test]
    fn test_text_to_bindings_simple() {
        let spec = text_to_bindings("Based on @1", 3).unwrap();
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_1"));
    }

    #[test]
    fn test_text_to_bindings_multiple() {
        let spec = text_to_bindings("Combine @1 and @2", 3).unwrap();
        assert_eq!(spec.len(), 2);
        assert!(spec.contains_key("ref_1"));
        assert!(spec.contains_key("ref_2"));
    }

    #[test]
    fn test_text_to_bindings_with_last() {
        let spec = text_to_bindings("Continue from @last", 5).unwrap();
        assert_eq!(spec.len(), 1);
        assert!(spec.contains_key("ref_5")); // @last resolves to 5
    }

    #[test]
    fn test_text_to_bindings_with_range() {
        let spec = text_to_bindings("Summarize @1..3", 5).unwrap();
        assert_eq!(spec.len(), 3);
        assert!(spec.contains_key("ref_1"));
        assert!(spec.contains_key("ref_2"));
        assert!(spec.contains_key("ref_3"));
    }

    #[test]
    fn test_text_to_bindings_with_all() {
        let spec = text_to_bindings("Based on @all", 3).unwrap();
        assert_eq!(spec.len(), 3);
        assert!(spec.contains_key("ref_1"));
        assert!(spec.contains_key("ref_2"));
        assert!(spec.contains_key("ref_3"));
    }

    #[test]
    fn test_text_to_bindings_no_mentions() {
        let spec = text_to_bindings("Just a normal message", 5).unwrap();
        assert!(spec.is_empty());
    }

    #[test]
    fn test_text_to_bindings_error_out_of_bounds() {
        let result = text_to_bindings("Reference @10", 3);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            MentionResolutionError::MessageNotFound { index: 10, max: 3 }
        );
    }

    #[test]
    fn test_text_to_bindings_dedup() {
        // Same reference multiple times should produce one binding
        let spec = text_to_bindings("See @1 and again @1", 3).unwrap();
        assert_eq!(spec.len(), 1); // HashMap deduplicates
        assert!(spec.contains_key("ref_1"));
    }
}
