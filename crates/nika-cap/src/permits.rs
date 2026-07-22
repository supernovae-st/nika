// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `permits:` block — the declared capability boundary.
//!
//! Per spec `01-envelope.md` §permits · the workflow's entire blast
//! radius, declared in-file. **F-O8 « absent = zero authority »
//! (NEP-0003)** — a MISSING block is the UNDECLARED zero: every effect
//! is refused (`NIKA-AUTH-006` at check · `NIKA-SEC-004` at run in
//! defense-in-depth) and only the always-on floors (SSRF · blocklist ·
//! masking) stay on top. `permits: {}` is the LEGAL declared zero — a
//! workflow provably limited to pure compute.
//! **Once present, every category is DEFAULT-DENY unless
//! listed**. Escapes are refused statically (`nika check`) and fail at
//! runtime with `NIKA-SEC-004` (`security_error` · never fed back to an
//! `agent:` model).

use serde::{Deserialize, Serialize};

/// The declared capability boundary (spec `01-envelope.md` §permits).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Permits {
    /// `fs:` — filesystem reach (path globs · gitignore-style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs: Option<FsPermits>,
    /// `net:` — network reach (host allowlist · the in-file SSRF boundary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<NetPermits>,
    /// `exec:` — shell reach (`false` · `true` · a program allowlist).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecPermit>,
    /// `tools:` — the `nika:`/`mcp:` tool surface (id globs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

impl FsPermits {
    /// Construct from read + write path-glob lists (the downstream-constructor
    /// non-exhaustive structs need · forward-compat invariant #19).
    #[must_use]
    pub fn new(read: Vec<String>, write: Vec<String>) -> Self {
        Self { read, write }
    }
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

impl NetPermits {
    /// Construct from an http host allowlist.
    #[must_use]
    pub fn new(http: Vec<String>) -> Self {
        Self { http }
    }
}

/// `exec:` — declared shell reach (closed tri-state).
#[derive(Debug, Clone, PartialEq, Eq)]
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

// Spec wire form (`01-envelope.md` §permits): `exec:` is `false | true |
// [programs]`, NOT serde's default externally-tagged `"No" | {"Programs":…}`.
// The engine's own parse/render paths are hand-written; these impls make the
// public serde surface agree with the spec so any future consumer (a cache, a
// snapshot, `nika-policy`) reads/writes the same shape the author typed.
impl Serialize for ExecPermit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ExecPermit::No => serializer.serialize_bool(false),
            ExecPermit::Any => serializer.serialize_bool(true),
            ExecPermit::Programs(programs) => programs.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ExecPermit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Toggle(bool),
            Programs(Vec<String>),
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Toggle(false) => ExecPermit::No,
            Wire::Toggle(true) => ExecPermit::Any,
            Wire::Programs(programs) => ExecPermit::Programs(programs),
        })
    }
}

/// Gitignore-style glob match for tool ids / hosts — v0.1 surface:
/// exact match, or a single trailing `*` matching any (possibly empty)
/// tail. Mirrors the agent `tools:` whitelist semantics. Public since
/// v0.105.x (NEP-0002 v2.0's whitelist classification needs the ONE
/// matcher — a second spelling of the same rule would drift).
#[must_use]
pub fn glob_matches(glob: &str, value: &str) -> bool {
    match glob.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => glob == value,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

    #[test]
    fn exec_serde_speaks_the_spec_wire_form() {
        use serde_json::{from_str, json, to_value};
        // M2 (Gate-11): the tri-state serializes as `false | true | [programs]`
        // (spec 01-envelope §permits), NOT serde's default `"No" | {"Programs":…}`.
        assert_eq!(to_value(ExecPermit::No).unwrap(), json!(false));
        assert_eq!(to_value(ExecPermit::Any).unwrap(), json!(true));
        assert_eq!(
            to_value(ExecPermit::Programs(vec!["git".into(), "cargo".into()])).unwrap(),
            json!(["git", "cargo"])
        );
        assert_eq!(from_str::<ExecPermit>("false").unwrap(), ExecPermit::No);
        assert_eq!(from_str::<ExecPermit>("true").unwrap(), ExecPermit::Any);
        assert_eq!(
            from_str::<ExecPermit>(r#"["git"]"#).unwrap(),
            ExecPermit::Programs(vec!["git".into()])
        );
    }

    #[test]
    fn empty_permits_round_trips_as_braces() {
        use serde_json::{from_str, to_string};
        // `permits: {}` is pure compute — no explicit nulls in the wire form.
        assert_eq!(to_string(&Permits::new()).unwrap(), "{}");
        assert_eq!(from_str::<Permits>("{}").unwrap(), Permits::new());
        // a partial boundary round-trips field-for-field with absent = dropped.
        let mut p = Permits::new();
        p.exec = Some(ExecPermit::No);
        p.tools = Some(vec!["nika:read".into()]);
        let wire = to_string(&p).unwrap();
        assert_eq!(from_str::<Permits>(&wire).unwrap(), p);
        assert!(
            !wire.contains("null"),
            "skip_serializing_if drops absent categories"
        );
    }
}
