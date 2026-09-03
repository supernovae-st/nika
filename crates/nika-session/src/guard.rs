// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The hallucination guard — every reply is read before a human sees
//! it. A Nika-specific named entity (a builtin · a model · an error code
//! · an MCP server · a CLI verb · a workflow field) is validated against
//! the installed catalogs and the project's configuration; what this
//! engine does not carry is CORRECTED in the reply, never presented as
//! real. A claim of ignorance about Nika is corrected too: the canon is
//! a retrieval away.

use std::collections::BTreeSet;
use std::path::Path;

/// One thing the reply named that the engine does not carry.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Finding {
    /// A `nika:<name>` this engine does not ship.
    Builtin(String),
    /// A `<provider>/<model>` this binary cannot resolve.
    Model {
        /// The id as written.
        id: String,
        /// The resolver's why.
        why: String,
    },
    /// A `NIKA-…` code neither the registry nor the spec knows.
    Code(String),
    /// An `mcp:<server>/…` this project does not configure.
    McpServer(String),
    /// A `nika <verb>` the binary does not have.
    Verb(String),
    /// A dead or foreign top-level field in a YAML block.
    Field(String),
    /// A « verb » or « task type » the language does not have (the four
    /// verbs are `infer` · `exec` · `invoke` · `agent`).
    WorkflowVerb(String),
    /// « I don't know Nika » — the canon is a retrieval away.
    ClaimedIgnorance,
}

impl Finding {
    /// The correction a human reads under the reply.
    #[must_use]
    pub fn correction(&self) -> String {
        match self {
            Self::Builtin(name) => format!(
                "`nika:{name}` is not a builtin this engine ships — `nika catalog --tools` lists the builtins it does"
            ),
            Self::Model { id, why } => format!("`{id}` does not resolve in this binary — {why}"),
            Self::Code(code) => format!("`{code}` is not a code this engine knows — `nika explain` teaches the ones it does"),
            Self::McpServer(name) => format!(
                "no MCP server `{name}` is configured in this project (`.nika/mcp_servers.json`) — `nika wire` adds one"
            ),
            Self::Verb(verb) => format!("`nika {verb}` is not a command — `nika --help` lists the verbs"),
            Self::Field(field) => format!(
                "`{field}` is not a workflow field — the envelope is nine keys and `tasks:` is a map (ask for the schema)"
            ),
            Self::WorkflowVerb(verb) => format!(
                "`{verb}` is not a verb — the four verbs are `infer` · `exec` · `invoke` · `agent` (a fetch is the builtin `nika:fetch` under `invoke`)"
            ),
            Self::ClaimedIgnorance => {
                "the installed engine's canon is available: ask for the schema, the canon, an example or a template".to_owned()
            }
        }
    }
}

/// What this engine and this project carry — the truth the guard reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnownWorld {
    /// The builtin names (`nika:<name>` → `name`).
    pub builtins: BTreeSet<String>,
    /// The CLI verbs.
    pub verbs: BTreeSet<String>,
    /// The MCP servers the project configures.
    pub mcp_servers: BTreeSet<String>,
}

/// The nine keys plus the task-level keys a YAML block may open with.
const LEGAL_KEYS: &[&str] = &[
    "nika",
    "model",
    "inputs",
    "const",
    "secrets",
    "permits",
    "run",
    "tasks",
    "outputs",
    "infer",
    "exec",
    "invoke",
    "agent",
    "with",
    "after",
    "when",
    "for_each",
    "timeout",
    "retry",
    "on_error",
    "extract",
    "lift",
    "returns",
    "group",
    "prompt",
    "system",
    "max_tokens",
    "temperature",
    "schema",
    "thinking",
    "vision",
    "command",
    "shell",
    "cwd",
    "env",
    "stdin",
    "capture",
    "decode",
    "tool",
    "workflow",
    "args",
    "tools",
    "skills",
    "max_turns",
    "max_tokens_total",
    "type",
    "default",
    "required",
    "items",
    "max_parallel",
    "fail_fast",
    "path",
    "url",
    "mode",
    "content",
    "source",
    "key",
    "egress",
    "to",
    "fs",
    "read",
    "write",
    "net",
    "http",
    "entropy",
    "clock",
    "recover",
    "skip",
    "on_codes",
    "max_attempts",
    "backoff_ms",
    "backoff_strategy",
    "jitter",
    "law",
    "from",
    "because",
    "expression",
    "input",
    "description",
    "enabled",
    "budget_tokens",
];

/// The dead and foreign forms the guard names when a YAML block opens with them.
const DEAD_FIELDS: &[&str] = &[
    "steps",
    "version",
    "depends_on",
    "fetch",
    "vars",
    "config",
    "types",
    "assert",
    "policy",
    "artifacts",
    "structured",
    "jobs",
    "nodes",
    "connections",
    "on_finally",
    "needs",
    "apiVersion",
];

impl KnownWorld {
    /// The world as installed: the builtins from the catalog, the verbs
    /// from the binary's tree, the MCP servers from the project file.
    #[must_use]
    pub fn installed(root: &Path) -> Self {
        let builtins = builtin_names()
            .into_iter()
            .map(|n| n.trim_start_matches("nika:").to_owned())
            .collect();
        let verbs = [
            "check",
            "run",
            "try",
            "test",
            "trace",
            "explain",
            "inspect",
            "new",
            "catalog",
            "doctor",
            "welcome",
            "wire",
            "model",
            "init",
            "spec",
            "sign",
            "key",
            "mcp",
            "lsp",
            "dap",
            "completions",
            "guard",
            "arm",
            "serve",
            "list",
            "graph",
            "tools",
            "fix",
            "version",
            "help",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let mut mcp_servers = BTreeSet::new();
        if let Ok(text) = std::fs::read_to_string(root.join(".nika").join("mcp_servers.json"))
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        {
            if let Some(map) = value.as_object() {
                mcp_servers.extend(map.keys().cloned());
            }
            if let Some(list) = value.get("servers").and_then(serde_json::Value::as_array) {
                mcp_servers.extend(
                    list.iter()
                        .filter_map(|s| s.get("name").and_then(serde_json::Value::as_str))
                        .map(str::to_owned),
                );
            }
        }
        Self {
            builtins,
            verbs,
            mcp_servers,
        }
    }

    /// Read a reply: every named entity checked, every miss a finding.
    #[must_use]
    pub fn audit(&self, reply: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut seen = BTreeSet::new();
        for token in tokens(reply) {
            if let Some(name) = token.strip_prefix("nika:") {
                let name = name.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
                if !name.is_empty()
                    && !self.builtins.contains(name)
                    && seen.insert(format!("b:{name}"))
                {
                    findings.push(Finding::Builtin(name.to_owned()));
                }
            } else if let Some(rest) = token.strip_prefix("mcp:") {
                let server = rest
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
                if !server.is_empty()
                    && !self.mcp_servers.contains(server)
                    && seen.insert(format!("m:{server}"))
                {
                    findings.push(Finding::McpServer(server.to_owned()));
                }
            } else if token.starts_with("NIKA-") {
                let code = token.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if code.len() > 5 && !code_known(code) && seen.insert(format!("c:{code}")) {
                    findings.push(Finding::Code(code.to_owned()));
                }
            } else if let Some((provider, model)) = token.split_once('/')
                && looks_like_model(provider, model)
                && let Some(refusal) = nika_providers::resolve_refusal(token)
                && seen.insert(format!("p:{token}"))
            {
                findings.push(Finding::Model {
                    id: token.to_owned(),
                    why: refusal.why,
                });
            }
        }
        for token in tokens(reply) {
            let bare = token.trim_end_matches(':');
            if token.ends_with(':')
                && DEAD_FIELDS.contains(&bare)
                && seen.insert(format!("f:{bare}"))
            {
                findings.push(Finding::Field(bare.to_owned()));
            }
        }
        for verb in invented_verbs(reply) {
            if seen.insert(format!("w:{verb}")) {
                findings.push(Finding::WorkflowVerb(verb));
            }
        }
        for verb in cli_verbs(reply) {
            if !self.verbs.contains(&verb) && seen.insert(format!("v:{verb}")) {
                findings.push(Finding::Verb(verb));
            }
        }
        for field in yaml_top_fields(reply) {
            let bare = field.trim_end_matches(':');
            if DEAD_FIELDS.contains(&bare) && seen.insert(format!("f:{bare}")) {
                findings.push(Finding::Field(bare.to_owned()));
            }
        }
        let lower = reply.to_ascii_lowercase();
        if [
            "don't know nika",
            "do not know nika",
            "not familiar with nika",
            "never heard of nika",
            "unfamiliar with nika",
        ]
        .iter()
        .any(|p| lower.contains(p))
        {
            findings.push(Finding::ClaimedIgnorance);
        }
        findings
    }

    /// The reply a human sees: the text, then one correction per finding.
    #[must_use]
    pub fn correct(reply: &str, findings: &[Finding]) -> String {
        if findings.is_empty() {
            return reply.to_owned();
        }
        let mut out = reply.trim_end().to_owned();
        out.push_str("\n\n· grounding (the installed engine disagrees with the reply above):");
        for f in findings {
            out.push_str("\n  · ");
            out.push_str(&f.correction());
        }
        out
    }
}

/// The builtin names the catalog ships (`nika:<name>`), whichever shape
/// the projection wraps them in (a bare array, or an object's `tools`).
#[must_use]
pub fn builtin_names() -> Vec<String> {
    let value = nika_builtin::tools_json();
    let rows = value
        .as_array()
        .cloned()
        .or_else(|| {
            value
                .get("tools")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    rows.iter()
        .filter_map(|t| t.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn code_known(code: &str) -> bool {
    nika_error::codes::lookup(code).is_some()
        || nika_pack::error_codes().iter().any(|r| r.code == code)
}

fn looks_like_model(provider: &str, model: &str) -> bool {
    !provider.is_empty()
        && !model.is_empty()
        && provider
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':')
        && provider.len() >= 3
        && ![
            "http", "https", "file", "usr", "tmp", "etc", "var", "home", "out", "src", "docs",
        ]
        .contains(&provider)
}

/// The backtick-quoted and bare tokens a reply names.
fn tokens(reply: &str) -> Vec<&str> {
    reply
        .split(|c: char| {
            c.is_whitespace()
                || c == '`'
                || c == '('
                || c == ')'
                || c == '"'
                || c == '\''
                || c == ','
                || c == ';'
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// A `snake_case` word the reply calls a « verb » or a « task type » that is
/// not one of the four (`the fetch_internet verb` · `a notify task type`).
fn invented_verbs(reply: &str) -> Vec<String> {
    let words: Vec<&str> = reply.split_whitespace().collect();
    let mut out = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let names_a_verb = w.eq_ignore_ascii_case("verb")
            || (w.eq_ignore_ascii_case("task")
                && words.get(i + 1).is_some_and(|n| n.starts_with("type")));
        if !names_a_verb || i == 0 {
            continue;
        }
        let candidate = words[i - 1].trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        let identifier = candidate.len() > 2
            && candidate
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && candidate.contains('_');
        if identifier && !["infer", "exec", "invoke", "agent"].contains(&candidate) {
            out.push(candidate.to_owned());
        }
    }
    out
}

/// `nika <verb>` mentions.
fn cli_verbs(reply: &str) -> Vec<String> {
    let mut out = Vec::new();
    let words: Vec<&str> = reply
        .split(|c: char| {
            c.is_whitespace() || c == '`' || c == '"' || c == '\'' || c == '(' || c == ')'
        })
        .filter(|t| !t.is_empty())
        .collect();
    for pair in words.windows(2) {
        if pair[0] == "nika" {
            let verb = pair[1].trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            if !verb.is_empty()
                && verb.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !verb.starts_with('-')
                && verb.len() > 1
            {
                out.push(verb.to_owned());
            }
        }
    }
    out
}

/// Top-level keys of YAML blocks in the reply (column-0 `key:` lines).
fn yaml_top_fields(reply: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in reply.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block {
            continue;
        }
        if let Some((key, _)) = line.split_once(':')
            && !line.starts_with(' ')
            && !line.starts_with('-')
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !key.is_empty()
            && !LEGAL_KEYS.contains(&key)
        {
            out.push(key.to_owned());
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn world() -> KnownWorld {
        KnownWorld::installed(Path::new("/nonexistent-root"))
    }

    /// The adversarial corpus of the pack: an invented builtin, model,
    /// code, MCP server, verb and field, and a claim of ignorance — each
    /// caught, each corrected.
    #[test]
    fn the_guard_catches_the_seven_inventions() {
        let w = world();
        let reply = "Use `nika:telegram` to post, with `acme/gpt-9` as the model; on error you get `NIKA-9999-ZZ`. Configure `mcp:telegram/send`, then run `nika deploy`.\n```yaml\nsteps:\n  - name: fetch\nnika: demo\ntasks:\n  t:\n    infer: { prompt: hi }\n```\nHonestly I don't know Nika well.";
        let findings = w.audit(reply);
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Builtin(n) if n == "telegram")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Model { id, .. } if id == "acme/gpt-9")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Code(c) if c == "NIKA-9999-ZZ")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::McpServer(s) if s == "telegram")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Verb(v) if v == "deploy")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Field(k) if k == "steps")),
            "{findings:?}"
        );
        assert!(
            findings.contains(&Finding::ClaimedIgnorance),
            "{findings:?}"
        );
        let shown = KnownWorld::correct(reply, &findings);
        assert!(
            shown.contains("grounding (the installed engine disagrees"),
            "{shown}"
        );
        assert!(
            shown.contains("`nika:telegram` is not a builtin"),
            "{shown}"
        );
        assert!(shown.contains("`steps` is not a workflow field"), "{shown}");
    }

    /// The rig's seat sentence: a dead field named in prose and an invented
    /// verb, both corrected; the real builtin untouched.
    #[test]
    fn a_dead_field_in_prose_and_an_invented_verb_are_corrected() {
        let w = world();
        let reply = "I would put the summary in a nika:write task; use `steps:` if you prefer, or the fetch_internet verb.";
        let findings = w.audit(reply);
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::Field(k) if k == "steps")),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, Finding::WorkflowVerb(v) if v == "fetch_internet")),
            "{findings:?}"
        );
        assert!(
            !findings.iter().any(|f| matches!(f, Finding::Builtin(_))),
            "nika:write is real: {findings:?}"
        );
        let shown = KnownWorld::correct(reply, &findings);
        assert!(shown.contains("`fetch_internet` is not a verb"), "{shown}");
    }

    /// The truth passes untouched: real builtins, a resolvable model, a
    /// registered code, real verbs, the nine keys.
    #[test]
    fn the_truth_passes_untouched() {
        let w = world();
        let reply = "Read it with `nika:read`, shape it with `nika:jq`, run `nika check` then `nika run`. `mock/echo` rehearses; `NIKA-AUTH-006` is the absent boundary.\n```yaml\nnika: demo\nmodel: mock/echo\npermits: { tools: [\"nika:read\"] }\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: ./x } }\n```";
        let findings = w.audit(reply);
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(KnownWorld::correct(reply, &findings), reply);
    }

    /// A configured MCP server is known; an unconfigured one is not.
    #[test]
    fn the_projects_mcp_servers_are_the_known_ones() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::create_dir_all(dir.path().join(".nika")).expect("dir");
        std::fs::write(
            dir.path().join(".nika").join("mcp_servers.json"),
            "{\"servers\":[{\"name\":\"github\",\"command\":\"x\"}]}",
        )
        .expect("servers");
        let w = KnownWorld::installed(dir.path());
        assert!(w.mcp_servers.contains("github"));
        let findings = w.audit("call `mcp:github/list_issues` and `mcp:slack/post`");
        assert_eq!(findings, vec![Finding::McpServer("slack".to_owned())]);
    }
}
