// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tool-reference grammar — ONE parser, three readers.
//!
//! A tool id (`nika:<path>` · `mcp:<server>/<tool>`) is judged at three
//! moments by two crates, and until 2026-08-15 each carried its own
//! rules. They disagreed **in both directions**, measured:
//!
//! ```text
//! nika:a:b         the CHECK refused it · the RUNTIME accepted it
//! nika:x\u{7f}y    the CHECK PASSED it  · the RUNTIME refused it
//! ```
//!
//! The second is the dangerous half. A control character in a forwarded
//! tool name is a log-injection vector — the runtime's own comment says
//! so — and the checker waved it through, at the only moment an author is
//! still reading. A check that says « clean » about an id the runtime
//! will reject is a check that LIES.
//!
//! The third reader (the agent `tools:` whitelist) carried a comment
//! claiming « parity with `validate_tool_ref` (invoke) ». It was not
//! true: it refused a second colon, invoke allowed it. A hand-maintained
//! mirror that asserts a parity it does not have is worse than no claim.
//!
//! ## Which rule won, and why
//!
//! The union, because each side uniquely caught something real:
//!
//! - **padding + control chars** come from the RUNTIME. They close the
//!   log-injection lane, and nothing upstream was closing it.
//! - **exactly one colon** comes from the CHECK, which follows the spec
//!   (`02-verbs.md` §invoke · « the colon marks the namespace boundary
//!   (exactly once) »). The runtime was lax against its own spec.
//!
//! Tightening the runtime breaks nothing that could exist: an id with a
//! second colon was already refused by the checker, so no *checked*
//! workflow carries one.
//!
//! ## What stays out
//!
//! Whether a tool RESOLVES (does that server exist, does it answer) is
//! the dispatcher's, not the grammar's. This module answers « is this a
//! tool id at all ».

/// The closed v1 namespace set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolNamespace {
    /// The engine's own tools.
    Nika,
    /// An MCP server's tools — always `<server>/<tool>`.
    Mcp,
}

impl ToolNamespace {
    /// The prefix as it is written, without the colon.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nika => "nika",
            Self::Mcp => "mcp",
        }
    }

    /// Read a namespace word, or `None` when it names no v1 namespace.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "nika" => Some(Self::Nika),
            "mcp" => Some(Self::Mcp),
            _ => None,
        }
    }
}

/// The parsed parts of a tool id — borrowed, so no reader pays an
/// allocation to ask a question.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolRef<'a> {
    /// Which closed namespace the id names.
    pub namespace: ToolNamespace,
    /// Everything after the colon.
    pub path: &'a str,
    /// The MCP server, when the namespace is `mcp:`.
    pub server: Option<&'a str>,
    /// The MCP tool name, when the namespace is `mcp:`.
    pub name: Option<&'a str>,
}

impl<'a> ToolRef<'a> {
    /// The constructor the `#[non_exhaustive]` marker requires
    /// (invariant #19).
    #[must_use]
    pub const fn new(
        namespace: ToolNamespace,
        path: &'a str,
        server: Option<&'a str>,
        name: Option<&'a str>,
    ) -> Self {
        Self {
            namespace,
            path,
            server,
            name,
        }
    }
}

/// Why a string is not a tool id — each arm carries the sentence the
/// author reads, so all three readers refuse in the SAME words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolRefDefect {
    /// Leading or trailing whitespace.
    Padding,
    /// An ASCII control character anywhere in the id.
    ControlChar,
    /// No `namespace:` prefix at all.
    MissingNamespace,
    /// A second colon after the namespace boundary.
    ExtraColon,
    /// A namespace outside the closed v1 set.
    UnknownNamespace,
    /// Nothing after the namespace.
    EmptyPath,
    /// `mcp:` without a non-empty `<server>/<tool>`.
    McpNeedsSlash,
}

impl ToolRefDefect {
    /// The teaching sentence — one voice for all three readers.
    #[must_use]
    pub const fn teaching(self) -> &'static str {
        match self {
            Self::Padding => "a tool id carries no leading or trailing whitespace",
            Self::ControlChar => {
                "a tool id carries no control character (it would ride into the log and the event fields)"
            }
            Self::MissingNamespace => "expected `nika:<path>` or `mcp:<server>/<tool>`",
            Self::ExtraColon => "the colon marks the namespace boundary exactly once",
            Self::UnknownNamespace => "the v1 namespaces are `nika:` and `mcp:`",
            Self::EmptyPath => "nothing follows the namespace",
            Self::McpNeedsSlash => "`mcp:` requires `<server>/<tool>`, both non-empty",
        }
    }
}

/// Parse a full tool id.
///
/// # Errors
/// [`ToolRefDefect`] — each arm carries its own teaching sentence.
pub fn parse(tool: &str) -> Result<ToolRef<'_>, ToolRefDefect> {
    reject_shape(tool)?;
    let Some((word, path)) = tool.split_once(':') else {
        return Err(ToolRefDefect::MissingNamespace);
    };
    if path.contains(':') {
        return Err(ToolRefDefect::ExtraColon);
    }
    let Some(namespace) = ToolNamespace::from_word(word) else {
        return Err(ToolRefDefect::UnknownNamespace);
    };
    if path.is_empty() {
        return Err(ToolRefDefect::EmptyPath);
    }
    match namespace {
        ToolNamespace::Nika => Ok(ToolRef::new(namespace, path, None, None)),
        ToolNamespace::Mcp => match path.split_once('/') {
            Some((server, name)) if !server.is_empty() && !name.is_empty() => {
                Ok(ToolRef::new(namespace, path, Some(server), Some(name)))
            }
            _ => Err(ToolRefDefect::McpNeedsSlash),
        },
    }
}

/// Read the namespace of ONE agent `tools:` whitelist glob.
///
/// A glob is not a tool id: `mcp:browser/*` names a namespace but no
/// tool, and a colon-less pattern (`*` · `**`) names no namespace at all
/// and is legal — it matches nothing namespaced, which is
/// least-privilege. A leading `!` negates and is stripped first.
///
/// What a glob still owes: the same shape rules, and a namespace from
/// the closed set when it names one.
///
/// # Errors
/// [`ToolRefDefect`] — shape defects, or an unknown namespace.
pub fn glob_namespace(glob: &str) -> Result<Option<ToolNamespace>, ToolRefDefect> {
    reject_shape(glob)?;
    let body = glob.strip_prefix('!').unwrap_or(glob);
    let Some((word, path)) = body.split_once(':') else {
        return Ok(None);
    };
    if path.contains(':') {
        return Err(ToolRefDefect::ExtraColon);
    }
    ToolNamespace::from_word(word)
        .map(Some)
        .ok_or(ToolRefDefect::UnknownNamespace)
}

/// The shape both forms owe — padding and control characters, refused
/// before anything is split. This is the RUNTIME's rule, and it is the
/// half the checker never had.
fn reject_shape(s: &str) -> Result<(), ToolRefDefect> {
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
        return Err(ToolRefDefect::Padding);
    }
    if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(ToolRefDefect::ControlChar);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_two_forms_parse() {
        let n = parse("nika:read").expect("an engine tool");
        assert_eq!(n.namespace, ToolNamespace::Nika);
        assert_eq!(n.path, "read");
        assert_eq!(n.server, None);

        let m = parse("mcp:browser/open").expect("an mcp tool");
        assert_eq!(m.namespace, ToolNamespace::Mcp);
        assert_eq!(m.server, Some("browser"));
        assert_eq!(m.name, Some("open"));
    }

    /// ⭐ The two ids the two readers disagreed about, measured on the
    /// tree before this module existed. Both are now refused at BOTH
    /// moments, and the author learns at the first.
    #[test]
    fn the_ids_the_readers_disagreed_about() {
        // The CHECK passed this; the RUNTIME refused it. A control char
        // in a forwarded tool name is a log-injection vector.
        assert_eq!(parse("nika:x\u{7f}y"), Err(ToolRefDefect::ControlChar));
        // The CHECK refused this; the RUNTIME accepted it, against the
        // spec's own « exactly once ».
        assert_eq!(parse("nika:a:b"), Err(ToolRefDefect::ExtraColon));
    }

    #[test]
    fn the_shape_rules_hold_for_both_forms() {
        assert_eq!(parse(" nika:x"), Err(ToolRefDefect::Padding));
        assert_eq!(parse("nika:x "), Err(ToolRefDefect::Padding));
        assert_eq!(parse("nika:x\ny"), Err(ToolRefDefect::ControlChar));
        assert_eq!(
            glob_namespace("nika:*\u{1}"),
            Err(ToolRefDefect::ControlChar)
        );
    }

    #[test]
    fn the_namespace_set_is_closed() {
        assert_eq!(parse("agent:compose"), Err(ToolRefDefect::UnknownNamespace));
        assert_eq!(parse("readfile"), Err(ToolRefDefect::MissingNamespace));
        assert_eq!(parse("nika:"), Err(ToolRefDefect::EmptyPath));
    }

    #[test]
    fn mcp_owes_both_segments() {
        for bad in ["mcp:browser", "mcp:/open", "mcp:browser/"] {
            assert_eq!(
                parse(bad),
                Err(ToolRefDefect::McpNeedsSlash),
                "`{bad}` must refuse"
            );
        }
    }

    /// A colon-less glob names no namespace and is LEGAL — it matches
    /// nothing namespaced, which is least-privilege. This is the one
    /// place the glob form and the id form legitimately differ.
    #[test]
    fn a_colonless_glob_names_no_namespace_and_is_legal() {
        assert_eq!(glob_namespace("*"), Ok(None));
        assert_eq!(glob_namespace("**"), Ok(None));
        assert_eq!(glob_namespace("!mcp:x"), Ok(Some(ToolNamespace::Mcp)));
        assert_eq!(
            glob_namespace("agent:*"),
            Err(ToolRefDefect::UnknownNamespace)
        );
        // A glob names no TOOL, so `mcp:browser/*` needs no second
        // segment rule here — the id form owes that, the glob does not.
        assert_eq!(glob_namespace("mcp:browser"), Ok(Some(ToolNamespace::Mcp)));
    }

    /// Every defect teaches, and no two arms teach the same sentence —
    /// a refusal an author cannot tell apart is a refusal that does not
    /// teach.
    #[test]
    fn every_defect_teaches_its_own_sentence() {
        let all = [
            ToolRefDefect::Padding,
            ToolRefDefect::ControlChar,
            ToolRefDefect::MissingNamespace,
            ToolRefDefect::ExtraColon,
            ToolRefDefect::UnknownNamespace,
            ToolRefDefect::EmptyPath,
            ToolRefDefect::McpNeedsSlash,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for d in all {
            assert!(!d.teaching().is_empty());
            assert!(seen.insert(d.teaching()), "`{d:?}` repeats a sentence");
        }
    }
}
