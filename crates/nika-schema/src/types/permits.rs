// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `permits:` block — the declared capability boundary.
//!
//! Per spec `01-envelope.md` §permits · the workflow's entire blast
//! radius, declared in-file. **Optional and non-breaking** — absent means
//! today's behavior (bounded only by the engine's SSRF + blocklist
//! floor). **Once present, every category is DEFAULT-DENY unless
//! listed** — `permits: {}` is a workflow provably limited to pure
//! compute. Escapes are refused statically (`nika check`) and fail at
//! runtime with `NIKA-SEC-004` (`security_error` · never fed back to an
//! `agent:` model).

use serde::{Deserialize, Serialize};

/// The declared capability boundary (spec `01-envelope.md` §permits).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Permits {
    /// `fs:` — filesystem reach (path globs · gitignore-style).
    pub fs: Option<FsPermits>,
    /// `net:` — network reach (host allowlist · the in-file SSRF boundary).
    pub net: Option<NetPermits>,
    /// `exec:` — shell reach (`false` · `true` · a program allowlist).
    pub exec: Option<ExecPermit>,
    /// `tools:` — the `nika:`/`mcp:` tool surface (id globs).
    pub tools: Option<Vec<String>>,
}

impl Permits {
    /// An empty boundary — pure compute (no fs · no net · no exec · no tools).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `exec:` tasks are allowed at all under this boundary.
    ///
    /// An omitted `exec:` category is default-deny (spec §permits).
    #[must_use]
    pub fn allows_exec(&self) -> bool {
        !matches!(self.exec, None | Some(ExecPermit::No))
    }

    /// Whether the argv program `program` is allowed under `exec:`.
    ///
    /// String-form commands have no single program — they are governed by
    /// [`Self::allows_exec`] + the engine blocklist floor.
    #[must_use]
    pub fn allows_program(&self, program: &str) -> bool {
        match &self.exec {
            None | Some(ExecPermit::No) => false,
            Some(ExecPermit::Any) => true,
            Some(ExecPermit::Programs(list)) => list.iter().any(|p| p == program),
        }
    }

    /// Whether the tool id `tool` matches the declared `tools:` globs.
    ///
    /// Glob semantics follow the agent-whitelist rules (spec `02-verbs.md`
    /// §tool whitelist) — exact match, or a `*` suffix matching any tail
    /// (`mcp:browser/*` · `nika:*`).
    #[must_use]
    pub fn allows_tool(&self, tool: &str) -> bool {
        match &self.tools {
            None => false,
            Some(globs) => globs.iter().any(|g| glob_matches(g, tool)),
        }
    }
}

/// `fs:` — declared filesystem reach.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FsPermits {
    /// Readable path globs. Empty/omitted = no reads.
    #[serde(default)]
    pub read: Vec<String>,
    /// Writable path globs. Empty/omitted = no writes.
    #[serde(default)]
    pub write: Vec<String>,
}

/// `net:` — declared network reach.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NetPermits {
    /// Allowed HTTP(S) hosts (globs ok · `*.github.com`). Tightens the
    /// engine SSRF floor · never loosens it.
    #[serde(default)]
    pub http: Vec<String>,
}

/// `exec:` — declared shell reach (closed tri-state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExecPermit {
    /// `exec: false` — this workflow runs zero shells.
    No,
    /// `exec: true` — any command (still blocklist-gated by the engine).
    Any,
    /// `exec: ["git", "cargo"]` — only these argv program names
    /// (`command[0]` of the array form).
    Programs(Vec<String>),
}

/// Gitignore-style glob match for tool ids / hosts — v0.1 surface:
/// exact match, or a single trailing `*` matching any (possibly empty)
/// tail. Mirrors the agent `tools:` whitelist semantics.
#[must_use]
pub(crate) fn glob_matches(glob: &str, value: &str) -> bool {
    match glob.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => glob == value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_deny_once_present() {
        let p = Permits::new(); // permits: {}
        assert!(!p.allows_exec(), "omitted exec = no shells");
        assert!(!p.allows_tool("nika:read"), "omitted tools = no tools");
        assert!(!p.allows_program("git"));
    }

    #[test]
    fn exec_tristate() {
        let mut p = Permits::new();
        p.exec = Some(ExecPermit::No);
        assert!(!p.allows_exec());
        p.exec = Some(ExecPermit::Any);
        assert!(p.allows_exec());
        assert!(p.allows_program("anything"));
        p.exec = Some(ExecPermit::Programs(vec!["git".into(), "cargo".into()]));
        assert!(p.allows_exec());
        assert!(p.allows_program("git"));
        assert!(!p.allows_program("rm"));
    }

    #[test]
    fn tool_globs() {
        let mut p = Permits::new();
        p.tools = Some(vec![
            "nika:read".into(),
            "mcp:browser/*".into(),
            "nika:connectome/*".into(),
        ]);
        assert!(p.allows_tool("nika:read"));
        assert!(!p.allows_tool("nika:write"));
        assert!(p.allows_tool("mcp:browser/navigate"));
        assert!(!p.allows_tool("mcp:postgres/query"));
        assert!(p.allows_tool("nika:connectome/recall"));
    }

    #[test]
    fn glob_star_matches_empty_tail() {
        // `nika:*` admits every builtin — including a bare prefix boundary.
        assert!(glob_matches("nika:*", "nika:read"));
        assert!(glob_matches("nika:*", "nika:"));
        assert!(!glob_matches("nika:*", "mcp:x/y"));
        // no-star = exact only
        assert!(glob_matches("a", "a"));
        assert!(!glob_matches("a", "ab"));
    }
}
