// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb Hints and Detection
//!
//! Input placeholder hints and verb command detection for the chat view.

use crate::theme::VerbColor;

// ═══════════════════════════════════════════════════════════════════════════════
// Verb Command Detection
// ═══════════════════════════════════════════════════════════════════════════════

/// Known verbs for command detection
const VERBS: &[(&str, VerbColor)] = &[
    ("invoke", VerbColor::Invoke),
    ("infer", VerbColor::Infer),
    ("fetch", VerbColor::Fetch),
    ("exec", VerbColor::Exec),
    ("agent", VerbColor::Agent),
];

/// Detect verb command at start of input (e.g., "/invoke", "/infer")
/// Returns (verb_len, verb_color, is_complete, full_verb_name) if found
pub fn detect_verb_in_input(input: &str) -> Option<(usize, VerbColor, bool, &'static str)> {
    // Must start with /
    if !input.starts_with('/') {
        return None;
    }

    // Extract the word after /
    let rest = &input[1..];
    let verb_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let verb_word = rest[..verb_end].to_lowercase();

    // Check for exact match first
    for (name, color) in VERBS {
        if verb_word == *name {
            return Some((1 + verb_end, *color, true, name));
        }
    }

    // Check for partial match (prefix) - only if at least 2 chars typed
    if verb_word.len() >= 2 {
        for (name, color) in VERBS {
            if name.starts_with(&verb_word) {
                return Some((1 + verb_end, *color, false, name));
            }
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Placeholder Hints
// ═══════════════════════════════════════════════════════════════════════════════

/// MCP tool examples from NovaNet + common patterns
const INVOKE_HINTS: &[&str] = &[
    "novanet:describe {\"entity\": \"qr-code\"}",
    "novanet:generate {\"locale\": \"fr-FR\", \"entity\": \"landing\"}",
    "novanet:traverse {\"start\": \"entity:qr\", \"depth\": 2}",
    "novanet:search {\"query\": \"pricing page\"}",
    "novanet:atoms {\"entity\": \"qr-code\", \"forms\": [\"title\", \"text\"]}",
    "filesystem:read_file {\"path\": \"./README.md\"}",
    "filesystem:list_directory {\"path\": \"./src\"}",
    "browser:screenshot {\"url\": \"https://example.com\"}",
    "database:query {\"sql\": \"SELECT * FROM users LIMIT 5\"}",
    "github:search_repos {\"query\": \"language:rust stars:>1000\"}",
];

/// Creative & practical LLM prompts
const INFER_HINTS: &[&str] = &[
    "Generate a landing page headline for a SaaS product",
    "Summarize this article in 3 bullet points",
    "Translate to French: Hello, how are you today?",
    "Explain this Rust code like I'm a beginner",
    "Write unit tests for this function using pytest",
    "Convert this SQL query to a TypeScript Prisma query",
    "Rewrite this paragraph in a more professional tone",
    "Generate 5 creative names for a coffee shop",
    "Create a regex to match email addresses",
    "Write a haiku about programming",
    "Debug this error: 'cannot borrow as mutable'",
    "Suggest 3 improvements for this API endpoint",
    "Write a commit message for these changes",
    "Create a product description for wireless earbuds",
    "Explain the difference between async and sync",
];

/// Productive shell one-liners
const EXEC_HINTS: &[&str] = &[
    "npm run build",
    "cargo test --release",
    "git status",
    "git log --oneline -10",
    "git diff --staged",
    "git commit -am \"fix: resolve issue\"",
    "ls -la ./src | head -20",
    "find . -name \"*.rs\" | wc -l",
    "grep -r \"TODO\" ./src",
    "docker ps -a",
    "docker-compose up -d",
    "npm outdated",
    "cargo clippy -- -D warnings",
    "python -m pytest -v",
    "curl -I https://example.com",
    "du -sh ./target",
    "cat package.json | jq '.dependencies'",
    "ps aux | grep node",
    "netstat -an | grep LISTEN",
    "tree -L 2 ./src",
];

/// Fun real APIs that actually work! (no auth required)
const FETCH_HINTS: &[&str] = &[
    "https://catfact.ninja/fact",
    "https://api.open-meteo.com/v1/forecast?latitude=48.8566&longitude=2.3522&current_weather=true",
    "https://httpbin.org/get",
    "https://dog.ceo/api/breeds/image/random",
    "https://api.chucknorris.io/jokes/random",
    "https://uselessfacts.jsph.pl/random.json?language=en",
    "https://api.coindesk.com/v1/bpi/currentprice.json",
    "https://api.github.com/zen",
    "https://official-joke-api.appspot.com/random_joke",
    "https://api.adviceslip.com/advice",
    "https://randomfox.ca/floof/",
    "https://xkcd.com/info.0.json",
    "https://api.ipify.org?format=json",
    "https://worldtimeapi.org/api/ip",
    "https://opentdb.com/api.php?amount=1",
    "http://api.open-notify.org/iss-now.json",
    "https://api.agify.io?name=thibaut",
];

/// Complex multi-step agentic tasks
const AGENT_HINTS: &[&str] = &[
    "Research competitors and write a market analysis report",
    "Analyze this codebase and suggest 5 improvements",
    "Build a landing page for QR Code AI with SEO",
    "Debug this error and propose a fix with tests",
    "Create a full REST API spec for a todo app",
    "Review this PR and provide detailed feedback",
    "Refactor this module to use async/await",
    "Generate documentation for all public functions",
    "Find and fix all security vulnerabilities",
    "Create a migration plan from v1 to v2 API",
    "Write a technical blog post about this feature",
    "Set up CI/CD pipeline with GitHub Actions",
    "Optimize database queries for better performance",
    "Create test fixtures for integration tests",
    "Design a caching strategy for this endpoint",
];

/// Spawned sub-agent task hints
const SPAWN_HINTS: &[&str] = &[
    "Delegate: research this topic in depth",
    "Spawn: handle this subtask independently",
    "Delegate: write tests for this module",
    "Spawn: analyze and summarize these files",
    "Delegate: generate documentation for API",
    "Spawn: refactor this function for clarity",
    "Delegate: investigate this bug in isolation",
    "Spawn: create sample data for testing",
];

/// Get a rotating placeholder hint for the given verb type
/// The hint changes every ~1 second at 60fps based on frame counter
pub fn verb_placeholder(verb_color: &VerbColor, frame: u8) -> &'static str {
    let idx = (frame / 60) as usize; // Change every ~1 second at 60fps
    match verb_color {
        VerbColor::Invoke => INVOKE_HINTS[idx % INVOKE_HINTS.len()],
        VerbColor::Infer => INFER_HINTS[idx % INFER_HINTS.len()],
        VerbColor::Exec => EXEC_HINTS[idx % EXEC_HINTS.len()],
        VerbColor::Fetch => FETCH_HINTS[idx % FETCH_HINTS.len()],
        VerbColor::Agent => AGENT_HINTS[idx % AGENT_HINTS.len()],
        VerbColor::Spawn => SPAWN_HINTS[idx % SPAWN_HINTS.len()],
        VerbColor::User => "Type your message...",
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_verb_exact_match() {
        let result = detect_verb_in_input("/invoke novanet:search");
        assert!(result.is_some());
        let (len, color, is_complete, name) = result.unwrap();
        assert_eq!(len, 7); // "/invoke"
        assert_eq!(color, VerbColor::Invoke);
        assert!(is_complete);
        assert_eq!(name, "invoke");
    }

    #[test]
    fn test_detect_verb_partial_match() {
        let result = detect_verb_in_input("/inv");
        assert!(result.is_some());
        let (_, color, is_complete, name) = result.unwrap();
        assert_eq!(color, VerbColor::Invoke);
        assert!(!is_complete);
        assert_eq!(name, "invoke");
    }

    #[test]
    fn test_detect_verb_no_match() {
        assert!(detect_verb_in_input("invoke").is_none()); // No leading /
        assert!(detect_verb_in_input("/x").is_none()); // Too short
        assert!(detect_verb_in_input("/unknown").is_none()); // Unknown verb
    }

    #[test]
    fn test_detect_all_verbs() {
        for (name, expected_color) in VERBS {
            let input = format!("/{} args", name);
            let result = detect_verb_in_input(&input);
            assert!(result.is_some(), "Should detect verb: {}", name);
            let (_, color, is_complete, detected_name) = result.unwrap();
            assert_eq!(color, *expected_color);
            assert!(is_complete);
            assert_eq!(detected_name, *name);
        }
    }

    #[test]
    fn test_verb_placeholder_rotates() {
        // Different frames should potentially return different hints
        let hint0 = verb_placeholder(&VerbColor::Infer, 0);
        let hint60 = verb_placeholder(&VerbColor::Infer, 60);
        let hint120 = verb_placeholder(&VerbColor::Infer, 120);

        // All should be valid non-empty strings
        assert!(!hint0.is_empty());
        assert!(!hint60.is_empty());
        assert!(!hint120.is_empty());

        // At frame 60 and 120, we should get different hints (they rotate)
        assert_ne!(hint0, hint60);
    }

    #[test]
    fn test_verb_placeholder_user() {
        assert_eq!(
            verb_placeholder(&VerbColor::User, 0),
            "Type your message..."
        );
        assert_eq!(
            verb_placeholder(&VerbColor::User, 100),
            "Type your message..."
        );
    }

    #[test]
    fn test_hint_arrays_not_empty() {
        assert!(!INVOKE_HINTS.is_empty());
        assert!(!INFER_HINTS.is_empty());
        assert!(!EXEC_HINTS.is_empty());
        assert!(!FETCH_HINTS.is_empty());
        assert!(!AGENT_HINTS.is_empty());
        assert!(!SPAWN_HINTS.is_empty());
    }
}
