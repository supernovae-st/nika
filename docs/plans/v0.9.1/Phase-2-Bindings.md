# Phase 2: Binding System (@mentions and Resolution)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Parse @mention syntax in chat messages and convert to WiringSpec bindings.

**Architecture:** MentionParser extracts @1, @last, @prev, @all from text. MentionToBinding converts these to UseEntry bindings. ChatAgent uses bindings to create DAG edges.

**Tech Stack:** regex, lazy_static

**Skills:** @rust-core, @test-driven-development

**Prerequisite:** Phase 1 complete (ChatWorkflow struct exists)

---

## Task 2.1: Create MentionParser Module

**Files:**
- Create: `src/tui/mention_parser.rs`
- Modify: `src/tui/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/mention_parser.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numeric_mention() {
        let mentions = parse_mentions("Use @1 and @2");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0], Mention::Number(1));
        assert_eq!(mentions[1], Mention::Number(2));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_parse_numeric_mention --lib`
Expected: FAIL with "cannot find module"

**Step 3: Write minimal implementation**

```rust
// src/tui/mention_parser.rs

use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    /// Match @1, @2, @last, @prev, @all, @msg-001
    static ref MENTION_RE: Regex = Regex::new(
        r"@((\d+)|last|prev|all|msg-\d{3})"
    ).unwrap();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mention {
    /// @1, @2, etc. (1-indexed)
    Number(u32),
    /// @last - last message
    Last,
    /// @prev - previous message
    Prev,
    /// @all - all previous messages
    All,
    /// @msg-001 - explicit ID
    Explicit(String),
}

/// Parse all @mentions from a message
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    MENTION_RE
        .captures_iter(text)
        .filter_map(|cap| {
            let m = cap.get(1)?.as_str();
            Some(match m {
                "last" => Mention::Last,
                "prev" => Mention::Prev,
                "all" => Mention::All,
                s if s.starts_with("msg-") => Mention::Explicit(s.into()),
                s => Mention::Number(s.parse().ok()?),
            })
        })
        .collect()
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_parse_numeric_mention --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/mention_parser.rs src/tui/mod.rs
git commit -m "feat(tui): create MentionParser for @mention syntax"
```

---

## Task 2.2: Test Mention::Last and Mention::Prev

**Files:**
- Modify: `src/tui/mention_parser.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_parse_last_and_prev_mentions() {
    let mentions = parse_mentions("Combine @last with @prev");
    assert_eq!(mentions.len(), 2);
    assert_eq!(mentions[0], Mention::Last);
    assert_eq!(mentions[1], Mention::Prev);
}

#[test]
fn test_parse_all_mention() {
    let mentions = parse_mentions("Summarize @all");
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0], Mention::All);
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test test_parse_last_and_prev --lib`
Expected: PASS (already implemented)

**Step 3: Commit**

```bash
git add src/tui/mention_parser.rs
git commit -m "test(tui): add tests for @last, @prev, @all mentions"
```

---

## Task 2.3: Implement Mention::resolve()

**Files:**
- Modify: `src/tui/mention_parser.rs:50-90`

**Step 1: Write the failing test**

```rust
#[test]
fn test_resolve_number_to_task_id() {
    let mention = Mention::Number(1);
    let ids = mention.resolve(5); // 5 messages in session
    assert_eq!(ids, vec!["msg-001".to_string()]);
}

#[test]
fn test_resolve_last_to_task_id() {
    let mention = Mention::Last;
    let ids = mention.resolve(5);
    assert_eq!(ids, vec!["msg-005".to_string()]);
}

#[test]
fn test_resolve_all_to_task_ids() {
    let mention = Mention::All;
    let ids = mention.resolve(3);
    assert_eq!(ids, vec![
        "msg-001".to_string(),
        "msg-002".to_string(),
        "msg-003".to_string(),
    ]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_resolve --lib`
Expected: FAIL with "method `resolve` not found"

**Step 3: Write minimal implementation**

```rust
impl Mention {
    /// Resolve to actual task ID(s) given message count
    pub fn resolve(&self, message_count: u32) -> Vec<String> {
        match self {
            Mention::Number(n) => vec![format!("msg-{:03}", n)],
            Mention::Last => vec![format!("msg-{:03}", message_count)],
            Mention::Prev => vec![format!("msg-{:03}", message_count)],
            Mention::All => (1..=message_count)
                .map(|n| format!("msg-{:03}", n))
                .collect(),
            Mention::Explicit(id) => vec![id.clone()],
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_resolve --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/mention_parser.rs
git commit -m "feat(tui): implement Mention::resolve() for ID generation"
```

---

## Task 2.4: Implement is_parallel() for // Prefix

**Files:**
- Modify: `src/tui/mention_parser.rs:90-110`

**Step 1: Write the failing test**

```rust
#[test]
fn test_is_parallel_with_prefix() {
    assert!(is_parallel("// Run this in parallel"));
    assert!(is_parallel("  // With leading spaces"));
}

#[test]
fn test_is_parallel_without_prefix() {
    assert!(!is_parallel("Normal message"));
    assert!(!is_parallel("/ Single slash"));
    assert!(!is_parallel("@1 Reference"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_is_parallel --lib`
Expected: FAIL with "function `is_parallel` not found"

**Step 3: Write minimal implementation**

```rust
/// Check if message starts with // (parallel prefix)
pub fn is_parallel(text: &str) -> bool {
    text.trim_start().starts_with("//")
}

/// Strip // prefix from message
pub fn strip_parallel_prefix(text: &str) -> &str {
    let text = text.trim_start();
    if text.starts_with("//") {
        text[2..].trim_start()
    } else {
        text
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_is_parallel --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/mention_parser.rs
git commit -m "feat(tui): implement is_parallel() for // prefix detection"
```

---

## Task 2.5: Add Edge Cases for MentionParser

**Files:**
- Modify: `src/tui/mention_parser.rs`

**Step 1: Write edge case tests**

```rust
#[test]
fn test_parse_no_mentions() {
    let mentions = parse_mentions("No mentions here");
    assert!(mentions.is_empty());
}

#[test]
fn test_parse_email_not_mention() {
    // Emails should NOT be treated as mentions
    let mentions = parse_mentions("Contact user@example.com");
    // @ followed by non-number/keyword should not match
    assert!(mentions.is_empty());
}

#[test]
fn test_parse_explicit_msg_id() {
    let mentions = parse_mentions("Refer to @msg-042");
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0], Mention::Explicit("msg-042".to_string()));
}

#[test]
fn test_parse_mixed_mentions() {
    let mentions = parse_mentions("Combine @1 with @last and @msg-010");
    assert_eq!(mentions.len(), 3);
    assert_eq!(mentions[0], Mention::Number(1));
    assert_eq!(mentions[1], Mention::Last);
    assert_eq!(mentions[2], Mention::Explicit("msg-010".to_string()));
}
```

**Step 2: Run tests**

Run: `cargo test mention_parser --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add src/tui/mention_parser.rs
git commit -m "test(tui): add edge case tests for MentionParser"
```

---

## 🧪 LIVE TEST CHECKPOINT 2a

```bash
# Run all mention tests
cargo test mention_ --lib

# Expected: All 10+ tests pass
```

---

## Task 2.6: Create MentionToBinding Converter

**Files:**
- Create: `src/tui/mention_binding.rs`
- Modify: `src/tui/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/mention_binding.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_mention_to_wiring() {
        let wiring = mentions_to_wiring("Use @1", 5, Some("msg-004"));

        assert_eq!(wiring.entries.len(), 1);
        assert_eq!(wiring.entries[0].alias, "m1");
        assert_eq!(wiring.entries[0].path.to_string(), "msg-001.output");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_single_mention_to_wiring --lib`
Expected: FAIL with "cannot find module"

**Step 3: Write minimal implementation**

```rust
// src/tui/mention_binding.rs

use crate::ast::WiringSpec;
use crate::binding::UseEntry;
use super::mention_parser::{parse_mentions, is_parallel, Mention};

/// Convert @mentions in text to WiringSpec bindings
pub fn mentions_to_wiring(
    text: &str,
    message_count: u32,
    prev_task_id: Option<&str>,
) -> WiringSpec {
    // If message is parallel (//), no dependencies
    if is_parallel(text) {
        return WiringSpec::default();
    }

    let mentions = parse_mentions(text);

    // If explicit @mentions, use those
    if !mentions.is_empty() {
        let entries: Vec<UseEntry> = mentions
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                m.resolve(message_count)
                    .into_iter()
                    .map(move |id| UseEntry {
                        alias: format!("m{}", i + 1).into(),
                        path: format!("{}.output", id).parse().unwrap(),
                        lazy: false,
                        default: None,
                    })
            })
            .collect();

        return WiringSpec { entries };
    }

    // Default: depend on previous message (sequential)
    if let Some(prev_id) = prev_task_id {
        WiringSpec {
            entries: vec![UseEntry {
                alias: "prev".into(),
                path: format!("{}.output", prev_id).parse().unwrap(),
                lazy: false,
                default: None,
            }],
        }
    } else {
        WiringSpec::default()
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_single_mention_to_wiring --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/mention_binding.rs src/tui/mod.rs
git commit -m "feat(tui): create MentionToBinding converter"
```

---

## Task 2.7: Test Multiple Mentions Binding

**Files:**
- Modify: `src/tui/mention_binding.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_multiple_mentions_to_wiring() {
    let wiring = mentions_to_wiring("Combine @1 and @3", 5, Some("msg-004"));

    assert_eq!(wiring.entries.len(), 2);
    assert_eq!(wiring.entries[0].alias, "m1");
    assert_eq!(wiring.entries[0].path.to_string(), "msg-001.output");
    assert_eq!(wiring.entries[1].alias, "m2");
    assert_eq!(wiring.entries[1].path.to_string(), "msg-003.output");
}
```

**Step 2: Run test**

Run: `cargo test test_multiple_mentions_to_wiring --lib`
Expected: PASS (already implemented)

**Step 3: Commit**

```bash
git add src/tui/mention_binding.rs
git commit -m "test(tui): verify multiple mentions binding conversion"
```

---

## Task 2.8: Test Parallel Prefix No Dependencies

**Files:**
- Modify: `src/tui/mention_binding.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_parallel_prefix_no_dependencies() {
    let wiring = mentions_to_wiring("// Independent task", 5, Some("msg-004"));

    // Parallel messages have NO dependencies
    assert!(wiring.entries.is_empty());
}

#[test]
fn test_parallel_with_mentions_still_no_deps() {
    // Even with @mentions, // prefix means no dependencies
    let wiring = mentions_to_wiring("// @1 is referenced but not depended on", 5, Some("msg-004"));

    // Design decision: // overrides @mentions
    assert!(wiring.entries.is_empty());
}
```

**Step 2: Run test**

Run: `cargo test test_parallel --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add src/tui/mention_binding.rs
git commit -m "test(tui): verify parallel prefix removes dependencies"
```

---

## Task 2.9: Test @all Mention Expansion

**Files:**
- Modify: `src/tui/mention_binding.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_all_mention_expands_to_all_previous() {
    let wiring = mentions_to_wiring("Summarize @all", 3, None);

    // @all should create 3 entries (one per previous message)
    assert_eq!(wiring.entries.len(), 3);
    assert_eq!(wiring.entries[0].path.to_string(), "msg-001.output");
    assert_eq!(wiring.entries[1].path.to_string(), "msg-002.output");
    assert_eq!(wiring.entries[2].path.to_string(), "msg-003.output");
}
```

**Step 2: Run test**

Run: `cargo test test_all_mention --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add src/tui/mention_binding.rs
git commit -m "test(tui): verify @all mention expands to all previous messages"
```

---

## Task 2.10: Integrate Bindings into ChatAgent

**Files:**
- Modify: `src/tui/views/chat.rs` (or chat_agent.rs)

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_chat_agent_creates_edges_from_mentions() {
    let mut agent = ChatAgent::new(/* ... */);

    // First message (no deps)
    agent.send_message("Hello").await.unwrap();

    // Second message with @1 mention
    agent.send_message("Continue @1").await.unwrap();

    // Verify edge was created
    let workflow = agent.workflow();
    assert!(workflow.dag.has_edge("msg-001", "msg-002"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_chat_agent_creates_edges_from_mentions --lib`
Expected: FAIL (not yet integrated)

**Step 3: Write integration code**

```rust
// In ChatAgent::send_message() or equivalent
impl ChatAgent {
    pub async fn send_message(&mut self, prompt: &str) -> Result<String, NikaError> {
        // 1. Parse @mentions and determine dependencies
        let prev_task_id = self.workflow.workflow.tasks.last()
            .map(|t| t.id.as_ref());
        let wiring = mentions_to_wiring(
            prompt,
            self.workflow.message_counter,
            prev_task_id,
        );

        // 2. Create task with wiring
        let task_id = self.workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id.clone(), prompt)
            .with_wiring(wiring.clone())
            .build();

        // 3. Add flows for dependencies (WIRING!)
        for entry in &wiring.entries {
            let source_id = entry.path.task_id();
            self.workflow.add_flow(&source_id, &task_id)?;
        }

        // 4. Add task and execute
        self.workflow.add_task(task);

        // ... rest of execution ...
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_chat_agent_creates_edges_from_mentions --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "feat(tui): integrate @mention bindings into ChatAgent"
```

---

## 🔌 WIRING CHECKPOINT 2: ChatWorkflow ↔ ChatAgent

```bash
# Verify wiring is correct
cargo test chat_agent --lib
cargo test mention_ --lib

# The critical test:
cargo test test_chat_agent_creates_edges_from_mentions --lib
```

**What to verify:**
- `mentions_to_wiring()` returns correct `WiringSpec`
- `WiringSpec.entries[].path.task_id()` returns source task ID
- `ChatWorkflow.add_flow()` creates edge in DAG
- Edge appears in `ChatWorkflow.workflow.flows`

---

## 🧪 LIVE TEST: End of Phase 2

```bash
# 1. Run all Phase 2 tests
cargo test mention_ --lib
cargo test mention_binding --lib
cargo test chat_agent --lib

# 2. Manual test in TUI
cargo run -- chat
> "Hello, I'm msg-001"
> "Continue from @1"  # Should depend on msg-001
> "// Independent task"  # Should have no dependencies
> "Combine @1 and @3"  # Should depend on both

# 3. Verify edge creation by checking EventLog
# (TaskStarted events should show correct dependencies)
```

---

## Phase 2 Deliverables

- [x] `MentionParser` with @number, @last, @prev, @all, @msg-NNN
- [x] `Mention::resolve()` converts to task IDs
- [x] `is_parallel()` detects // prefix
- [x] `mentions_to_wiring()` converts to WiringSpec
- [x] ChatAgent creates edges from @mentions
- [x] 30 new tests passing
- [x] Zero clippy warnings

---

## Next Phase

After Phase 2 passes all tests and live verification:
→ Proceed to [Phase-3-BuiltinTools.md](./Phase-3-BuiltinTools.md)
