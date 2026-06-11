// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tool whitelist — default-deny · gitignore-style globs (spec §3 ·
//! `02-verbs.md` §Tool whitelist · a **portability invariant**: a
//! v0.1-compliant engine MUST implement these semantics canonically).
//!
//! Match rules (canonical · verbatim from the spec):
//! - `*` matches any sequence EXCEPT the path separator `/` — so
//!   `mcp:browser/*` admits `mcp:browser/navigate`, never
//!   `mcp:browser/tab/open`;
//! - the namespace colon is never crossed by `*` either — `nika:*`
//!   can never reach an `mcp:` tool, and a bare `*` admits NO
//!   namespaced id (least-privilege by construction);
//! - `**` matches any sequence including `/` (rare · use sparingly);
//! - order matters: LATER rules override earlier ones;
//! - negation via the `!` prefix (`["mcp:browser/*",
//!   "!mcp:browser/navigate"]` denies navigate).
//!
//! Absent/empty list = NO tools (the pure-conversation agent). The
//! matcher is TOTAL — any pattern × any candidate, never panics, O(n·m)
//! (property-tested · the candidate is MODEL-controlled input).

/// One parsed rule: a glob pattern + its negation flag.
#[derive(Debug, Clone)]
struct Rule {
    tokens: Vec<Tok>,
    negated: bool,
}

/// A compiled whitelist (default-deny · last-match-wins).
#[derive(Debug, Clone)]
pub struct Whitelist {
    rules: Vec<Rule>,
}

impl Whitelist {
    /// Compile from the spec `tools:` list (empty = deny everything).
    #[must_use]
    pub fn new(patterns: &[String]) -> Self {
        let rules = patterns
            .iter()
            .map(|raw| {
                let (negated, body) = match raw.strip_prefix('!') {
                    Some(rest) => (true, rest),
                    None => (false, raw.as_str()),
                };
                Rule {
                    tokens: compile(body),
                    negated,
                }
            })
            .collect();
        Self { rules }
    }

    /// Does the rule list admit this tool id? Every rule is consulted in
    /// order; the LAST matching rule decides (gitignore semantics). No
    /// match = denied (default-deny).
    #[must_use]
    pub fn admits(&self, tool: &str) -> bool {
        let candidate: Vec<char> = tool.chars().collect();
        let mut verdict = false;
        for rule in &self.rules {
            if glob_match(&rule.tokens, &candidate) {
                verdict = !rule.negated;
            }
        }
        verdict
    }

    /// True when the list is empty (the pure-conversation agent).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Pattern token: literal char, `*` (bounded), or `**` (unbounded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Lit(char),
    /// `*` — any run of characters EXCEPT `/` and `:`.
    Star,
    /// `**` — any run of characters, separators included.
    DoubleStar,
}

fn compile(pattern: &str) -> Vec<Tok> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' {
            if chars.get(i + 1) == Some(&'*') {
                tokens.push(Tok::DoubleStar);
                i += 2;
            } else {
                tokens.push(Tok::Star);
                i += 1;
            }
        } else {
            tokens.push(Tok::Lit(chars[i]));
            i += 1;
        }
    }
    tokens
}

/// May a bounded `*` swallow `c`? (`**` swallows anything.)
fn star_admits(c: char) -> bool {
    c != '/' && c != ':'
}

/// Glob match via bottom-up DP — `dp[i][j]` = `pattern[i..]` matches
/// `candidate[j..]`. O(n·m), total, and CORRECT when bounded `*` and
/// unbounded `**` are interleaved (a single backtrack slot forgets an
/// earlier `**` when a later `*` dead-ends on a separator — the
/// `**a*b` vs `a/ab` class · pinned by test). The table is tiny (tool
/// ids are short) and the candidate is length-capped by the caller.
fn glob_match(pattern: &[Tok], candidate: &[char]) -> bool {
    let n = pattern.len();
    let m = candidate.len();
    // dp[i][j] across rows i = n..=0 ; row n+1 conceptually = the base.
    let mut dp = vec![vec![false; m + 1]; n + 1];
    dp[n][m] = true;
    for i in (0..n).rev() {
        // pattern[i..] vs the empty tail: only trailing stars survive.
        dp[i][m] = matches!(pattern[i], Tok::Star | Tok::DoubleStar) && dp[i + 1][m];
    }
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = match pattern[i] {
                Tok::Lit(l) => l == candidate[j] && dp[i + 1][j + 1],
                // `*`: consume zero (advance the pattern) OR one admitted
                // char (advance the candidate, same star).
                Tok::Star => dp[i + 1][j] || (star_admits(candidate[j]) && dp[i][j + 1]),
                // `**`: consume zero OR any one char.
                Tok::DoubleStar => dp[i + 1][j] || dp[i][j + 1],
            };
        }
    }
    dp[0][0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn wl(patterns: &[&str]) -> Whitelist {
        let owned: Vec<String> = patterns.iter().map(|s| (*s).to_owned()).collect();
        Whitelist::new(&owned)
    }

    #[test]
    fn exact_and_server_scoped_forms() {
        let list = wl(&["nika:read", "mcp:browser/*", "nika:done"]);
        assert!(list.admits("nika:read"));
        assert!(list.admits("nika:done"));
        assert!(list.admits("mcp:browser/click"));
        assert!(list.admits("mcp:browser/navigate"));
        assert!(!list.admits("nika:write"), "exact is exact");
        assert!(!list.admits("mcp:shell/run"), "server scope bounds");
        assert!(!list.admits("nika:read2"), "no implicit prefix");
    }

    #[test]
    fn star_never_crosses_slash_or_colon() {
        // The spec's own example: mcp:browser/* matches mcp:browser/navigate
        // but NOT mcp:browser/tab/open.
        let server = wl(&["mcp:browser/*"]);
        assert!(server.admits("mcp:browser/navigate"));
        assert!(!server.admits("mcp:browser/tab/open"), "* stops at /");

        // The namespace colon is never crossed: nika:* stays in nika:.
        let ns = wl(&["nika:*"]);
        assert!(ns.admits("nika:read"));
        assert!(ns.admits("nika:done"));
        assert!(!ns.admits("mcp:browser/click"));
        // And * inside nika: cannot reach a nested mcp-style path either.
        assert!(!ns.admits("nika:a/b"), "* stops at / inside the namespace");

        // A bare * admits NO namespaced id (every real tool id carries
        // `nika:` or `mcp:` — least-privilege by construction).
        let bare = wl(&["*"]);
        assert!(!bare.admits("nika:read"));
        assert!(!bare.admits("mcp:browser/click"));
        assert!(bare.admits("plain"), "un-namespaced text only");
    }

    #[test]
    fn double_star_crosses_separators() {
        let deep = wl(&["mcp:browser/**"]);
        assert!(deep.admits("mcp:browser/navigate"));
        assert!(deep.admits("mcp:browser/tab/open"), "** crosses /");

        let universe = wl(&["**"]);
        assert!(universe.admits("nika:read"));
        assert!(universe.admits("mcp:browser/tab/open"));
    }

    #[test]
    fn interleaved_double_star_then_bounded_star() {
        // The single-slot-backtrack counterexample: `**` swallows the
        // separator, the later bounded `*` matches the rest. A naive
        // matcher dead-ends the `*` on `/` and forgets the `**`.
        let mixed = wl(&["**a*b"]);
        assert!(mixed.admits("a/ab"), "** = `a/`, a, * = ``, b");
        assert!(mixed.admits("xx/yy/ab"), "** crosses both separators");
        assert!(!mixed.admits("a/ac"), "the trailing literal b still binds");

        // The reverse order (bounded then unbounded) is also exact.
        let rev = wl(&["mcp:*/**"]);
        assert!(rev.admits("mcp:browser/tab/open"));
        assert!(!rev.admits("mcp:browser"), "the literal / must be present");
    }

    #[test]
    fn negation_and_order_last_match_wins() {
        // The spec's own example: allow the server, carve out navigate.
        let carved = wl(&["mcp:browser/*", "!mcp:browser/navigate"]);
        assert!(carved.admits("mcp:browser/click"));
        assert!(!carved.admits("mcp:browser/navigate"), "negation carves");

        // Later rules override earlier ones — both directions.
        let re_allowed = wl(&["!nika:read", "nika:read"]);
        assert!(re_allowed.admits("nika:read"), "later allow wins");
        let re_denied = wl(&["nika:*", "!nika:read"]);
        assert!(!re_denied.admits("nika:read"), "later deny wins");
        assert!(re_denied.admits("nika:write"));

        // A pure-negation list admits nothing (default-deny floor).
        let only_neg = wl(&["!nika:read"]);
        assert!(!only_neg.admits("nika:read"));
        assert!(!only_neg.admits("nika:write"));
    }

    #[test]
    fn empty_is_default_deny() {
        let list = wl(&[]);
        assert!(list.is_empty());
        assert!(!list.admits("nika:read"));
        assert!(!list.admits(""));
    }

    #[test]
    fn interior_star_and_adjacent_stars() {
        let interior = wl(&["mcp:*/click"]);
        assert!(interior.admits("mcp:browser/click"));
        assert!(interior.admits("mcp:ui/click"));
        assert!(!interior.admits("mcp:browser/hover"));
        assert!(
            !interior.admits("mcp:a/b/click"),
            "the interior * cannot swallow a /"
        );

        let suffix = wl(&["*:done"]);
        assert!(suffix.admits("nika:done"), "* matches the namespace word");
        assert!(!suffix.admits("nika:done2"));
        assert!(!suffix.admits("a:b:done"), "* cannot swallow the first :");
    }

    proptest! {
        /// Totality: any pattern × any candidate never panics — the
        /// candidate string is MODEL-controlled input.
        #[test]
        fn matcher_is_total(pattern in ".{0,24}", candidate in ".{0,24}") {
            let list = Whitelist::new(&[pattern]);
            let _ = list.admits(&candidate);
        }

        /// Identity: a glob-free, negation-free pattern admits exactly
        /// itself.
        #[test]
        fn exact_patterns_admit_exactly_themselves(id in "[a-z:/_-]{1,16}") {
            prop_assume!(!id.contains('*'));
            let list = Whitelist::new(std::slice::from_ref(&id));
            let extended = format!("{id}x");
            prop_assert!(list.admits(&id));
            prop_assert!(!list.admits(&extended));
        }

        /// `prefix*` admits separator-free extensions and refuses any
        /// extension containing `/` or `:` (the canonical bound).
        #[test]
        fn star_bound_is_exact(prefix in "[a-z:]{1,8}", ext in "[a-z]{0,8}", sep in "[/:]") {
            let list = Whitelist::new(&[format!("{prefix}*")]);
            let within = format!("{prefix}{ext}");
            let beyond = format!("{prefix}{ext}{sep}tail");
            prop_assert!(list.admits(&within));
            prop_assert!(!list.admits(&beyond));
        }

        /// `prefix**` admits EVERY extension, separators included.
        #[test]
        fn double_star_admits_everything(prefix in "[a-z:]{1,8}", ext in "[a-z/:]{0,12}") {
            let list = Whitelist::new(&[format!("{prefix}**")]);
            let candidate = format!("{prefix}{ext}");
            prop_assert!(list.admits(&candidate));
        }
    }
}
