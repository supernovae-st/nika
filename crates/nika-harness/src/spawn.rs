// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The confined spawn (B3.2 · spec §4) — the adapter process is engine
//! INFRASTRUCTURE, strictly bounded: pinned binary identity · version
//! handshake BEFORE any session · controlled argv · composed env · no
//! shell · kill-on-drop.
//!
//! **The ONE deliberate difference from the `nika-mcp` spawn
//! discipline (A-3):** no env scrub of the harness's own auth store.
//! The harness MUST reach its own credentials (its config files under
//! `$HOME` · its keychain); scrubbing its environment would break the
//! whole legitimacy model. What never crosses is NIKA'S side of the
//! boundary: the child env is COMPOSED (the runner floor ∪ the
//! adapter's declared passthrough), and no credential-shaped variable
//! of the parent (`*_API_KEY` · `*_TOKEN` · `*_SECRET` · `NIKA_*`)
//! ever rides the floor — [`compose_env`] is pure and the negative
//! test pins it.

use std::collections::BTreeMap;
use std::process::Stdio;

use nika_kernel::ai::harness::{AgentBackend, HarnessError, HarnessEventStream, HarnessRequest};

use crate::client::drive;

/// The env floor a spawned adapter always receives — the variables a
/// CLI needs to run at all, none of them a secret channel.
const ENV_FLOOR: [&str; 7] = ["HOME", "PATH", "TERM", "LANG", "LC_ALL", "USER", "TMPDIR"];

/// Credential-shaped NAME fragments — a suffix list is not enough:
/// the single most common cloud family (`AWS_SECRET_ACCESS_KEY` ·
/// `AWS_ACCESS_KEY_ID`) ends in neither `_API_KEY` nor `_SECRET`, and
/// the refuter proved it crossed. The predicate is now SUBSTRING-based
/// over the vocabulary that names authority, plus the explicit
/// ambient-authority handles no pattern would catch.
const CREDENTIAL_FRAGMENTS: [&str; 10] = [
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "API_KEY",
    "ACCESS_KEY",
    "PRIVATE_KEY",
    "SESSION_KEY",
    "AUTH",
];

/// Ambient-authority handles: not static secrets, but LIVE authority a
/// child could wield (an agent socket · a connection string that
/// commonly embeds a password).
const AMBIENT_AUTHORITY: [&str; 4] = [
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "DATABASE_URL",
    "GOOGLE_APPLICATION_CREDENTIALS",
];

/// A parent variable whose NAME carries authority never rides the floor
/// or a passthrough — nika's own keys above all (A-3: the harness owns
/// ITS auth; nika's never crosses). Deliberately over-broad: a false
/// positive costs a harness one env var it can ask for by another name;
/// a false negative hands a child a live credential.
fn credential_shaped(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    name.starts_with("NIKA_")
        || AMBIENT_AUTHORITY.contains(&upper.as_str())
        || CREDENTIAL_FRAGMENTS.iter().any(|f| upper.contains(f))
}

/// Compose the child environment — PURE (the negative test's whole
/// surface): `floor ∩ parent` ∪ `passthrough ∩ parent`, with the
/// credential-shape ban applied to BOTH lists (a passthrough row
/// naming a key var is a config mistake, refused silently-safe by
/// exclusion — the doctor surface teaches it at probe time).
#[must_use]
pub fn compose_env(
    parent: &BTreeMap<String, String>,
    passthrough: &[String],
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for name in ENV_FLOOR {
        if let Some(v) = parent.get(name) {
            env.insert(name.to_owned(), v.clone());
        }
    }
    for name in passthrough {
        if credential_shaped(name) {
            continue;
        }
        if let Some(v) = parent.get(name) {
            env.insert(name.clone(), v.clone());
        }
    }
    env.retain(|k, _| !credential_shaped(k));
    env
}

/// One configured harness adapter — the pinned identity of a binary
/// this machine may drive (`#[non_exhaustive]` · registry rows arrive
/// at B6 with the probe surface).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HarnessAdapter {
    /// The adapter id — unique, and NEVER equal to an access-class
    /// wire string (`--access harness` must stay unambiguous · R-5d).
    pub id: String,
    /// The binary to spawn (absolute path or `$PATH` name) — `argv[0]`,
    /// never a shell line.
    pub command: String,
    /// The session-mode argv (e.g. `["--experimental-acp"]`).
    pub args: Vec<String>,
    /// Extra parent env vars the adapter may read (beyond the floor) —
    /// credential-shaped names are refused by composition.
    pub passthrough_env: Vec<String>,
}

impl HarnessAdapter {
    /// Construct (INV-019). Refuses an id that collides with the
    /// access-class vocabulary — the R-5d ambiguity is unrepresentable.
    ///
    /// # Errors
    ///
    /// The id equals an [`nika_kernel::ai::harness`]-era class token
    /// (`local` · `api` · `harness` · `oauth` · `mock`).
    pub fn new(id: impl Into<String>, command: impl Into<String>) -> Result<Self, HarnessError> {
        let id = id.into();
        if nika_types::access::AccessClass::ALL
            .iter()
            .any(|c| c.as_str() == id)
        {
            return Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter id `{id}` collides with an access-class token — \
                     `--access {id}` would be ambiguous (R-5d)"
                ),
            });
        }
        Ok(Self {
            id,
            command: command.into(),
            args: Vec::new(),
            passthrough_env: Vec::new(),
        })
    }

    /// Set the session-mode argv.
    #[must_use]
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Declare extra env passthrough (credential-shaped names never
    /// compose — see [`compose_env`]).
    #[must_use]
    pub fn with_passthrough_env(mut self, vars: Vec<String>) -> Self {
        self.passthrough_env = vars;
        self
    }
}

/// A spawned, session-ready harness — [`AgentBackend`] over the
/// adapter's stdio. Serial by construction: `run_agent` consumes ONE
/// spawn per call (the self-queue lives at the verb seam · B4).
#[derive(Debug, Clone)]
pub struct SpawnedHarness {
    adapter: HarnessAdapter,
}

impl SpawnedHarness {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(adapter: HarnessAdapter) -> Self {
        Self { adapter }
    }

    /// Spawn the adapter child — piped stdio · composed env ·
    /// kill-on-drop (a harness never outlives the task that asked).
    fn spawn_child(&self) -> Result<tokio::process::Child, HarnessError> {
        let parent: BTreeMap<String, String> = std::env::vars().collect();
        let env = compose_env(&parent, &self.adapter.passthrough_env);
        let mut cmd = tokio::process::Command::new(&self.adapter.command);
        cmd.args(&self.adapter.args)
            .env_clear()
            .envs(&env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        cmd.spawn().map_err(|e| HarnessError::Unavailable {
            reason: format!(
                "adapter `{}`: cannot spawn `{}`: {e}",
                self.adapter.id, self.adapter.command
            ),
        })
    }
}

impl AgentBackend for SpawnedHarness {
    async fn run_agent(&self, request: HarnessRequest) -> Result<HarnessEventStream, HarnessError> {
        let mut child = self.spawn_child()?;
        let stdout = child.stdout.take().ok_or_else(|| HarnessError::Session {
            reason: "the child's stdout was not piped".to_owned(),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| HarnessError::Session {
            reason: "the child's stdin was not piped".to_owned(),
        })?;
        // The child rides INSIDE the stream's driver task: dropping the
        // stream drops the driver, the driver drops the child, and
        // kill_on_drop reaps it — the cancel-safety contract.
        Ok(drive_with_child(stdout, stdin, request, child))
    }
}

/// [`drive`] with the child's lifetime tied to the stream — the child
/// handle parks inside a wrapper stream so its `Drop` (and the OS kill
/// underneath) fires exactly when the consumer lets go.
fn drive_with_child(
    stdout: tokio::process::ChildStdout,
    stdin: tokio::process::ChildStdin,
    request: HarnessRequest,
    child: tokio::process::Child,
) -> HarnessEventStream {
    let inner = drive(stdout, stdin, request);
    Box::pin(ChildStream {
        inner,
        _child: child,
    })
}

struct ChildStream {
    inner: HarnessEventStream,
    _child: tokio::process::Child,
}

impl futures_core::Stream for ChildStream {
    type Item = <HarnessEventStream as futures_core::Stream>::Item;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent_with_secrets() -> BTreeMap<String, String> {
        [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/u"),
            ("TERM", "xterm"),
            ("NIKA_MISTRAL_API_KEY", "sk-nika-never"),
            ("MISTRAL_API_KEY", "sk-raw-never"),
            ("GITHUB_TOKEN", "ghp-never"),
            ("DEPLOY_SECRET", "never"),
            ("NIKA_TRACE_KEEP", "10"),
            ("NO_COLOR", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    }

    #[test]
    fn the_floor_crosses_and_no_credential_shape_ever_does() {
        let env = compose_env(&parent_with_secrets(), &[]);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/u"));
        // The negative half — nika's keys, raw provider keys, tokens,
        // secrets, and EVERY NIKA_* var stay on nika's side (A-3).
        for banned in [
            "NIKA_MISTRAL_API_KEY",
            "MISTRAL_API_KEY",
            "GITHUB_TOKEN",
            "DEPLOY_SECRET",
            "NIKA_TRACE_KEEP",
        ] {
            assert!(!env.contains_key(banned), "{banned} must never cross");
        }
        // Non-floor, non-passthrough plain vars do not cross either.
        assert!(!env.contains_key("NO_COLOR"));
    }

    #[test]
    fn the_refuters_counterexamples_never_cross() {
        // The suffix heuristic let these through (refuted 2026-08-06):
        // AWS's canonical pair ends in `_KEY`/`_KEY_ID`, GCP's in
        // `_CREDENTIALS`, and the ambient handles match no pattern.
        let parent: BTreeMap<String, String> = [
            ("AWS_SECRET_ACCESS_KEY", "AKIA-never"),
            ("AWS_ACCESS_KEY_ID", "AKIA-id-never"),
            ("GOOGLE_APPLICATION_CREDENTIALS", "/path/to/sa.json"),
            ("DATABASE_URL", "postgres://u:p@h/db"),
            ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
            ("ANTHROPIC_AUTH_TOKEN", "sk-never"),
            ("MY_PASSWORD", "hunter2"),
            ("SERVICE_PRIVATE_KEY", "-----BEGIN"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        // Even DECLARED as passthrough (the config-mistake path), none
        // may cross.
        let declared: Vec<String> = parent.keys().cloned().collect();
        let env = compose_env(&parent, &declared);
        assert!(
            env.is_empty(),
            "every authority-shaped name must stay on nika's side, got {env:?}"
        );
    }

    #[test]
    fn passthrough_crosses_plain_vars_and_refuses_credential_shapes() {
        let env = compose_env(
            &parent_with_secrets(),
            &[
                "NO_COLOR".to_owned(),
                "MISTRAL_API_KEY".to_owned(), // a config mistake — refused
                "ABSENT_VAR".to_owned(),      // absent in parent — skipped
            ],
        );
        assert_eq!(env.get("NO_COLOR").map(String::as_str), Some("1"));
        assert!(!env.contains_key("MISTRAL_API_KEY"));
        assert!(!env.contains_key("ABSENT_VAR"));
    }

    #[test]
    fn an_adapter_id_never_shadows_a_class_token() {
        for token in ["local", "api", "harness", "oauth", "mock"] {
            let err = HarnessAdapter::new(token, "/bin/true")
                .expect_err("a class-token id must refuse (R-5d)");
            assert!(err.to_string().contains(token), "{err}");
        }
        let ok = HarnessAdapter::new("codex-acp", "codex-acp").expect("a real id constructs");
        assert_eq!(ok.id, "codex-acp");
    }

    #[test]
    fn an_absent_binary_is_unavailable_with_the_adapter_named() {
        let adapter =
            HarnessAdapter::new("ghost", "/nonexistent/ghost-bin-2026").expect("id is fine");
        let spawned = SpawnedHarness::new(adapter);
        let err = spawned.spawn_child().expect_err("no such binary");
        let HarnessError::Unavailable { reason } = &err else {
            panic!("an absent binary is Unavailable, got {err:?}");
        };
        assert!(reason.contains("ghost"), "{reason}");
    }
}
