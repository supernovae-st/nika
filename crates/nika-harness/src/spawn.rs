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

use nika_kernel::ai::harness::{AgentBackendDyn, HarnessError, HarnessEventStream, HarnessRequest};

use crate::client::drive;

/// The env floor a spawned adapter always receives — the variables a
/// CLI needs to run at all, none of them a secret channel.
/// How long a version probe may take before the adapter is called
/// unavailable — a `--version` is a millisecond operation; ten seconds
/// is already generous for a cold npx resolve.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

const ENV_FLOOR: [&str; 7] = ["HOME", "PATH", "TERM", "LANG", "LC_ALL", "USER", "TMPDIR"];

/// Credential-shaped NAME fragments — a suffix list is not enough:
/// the single most common cloud family (`AWS_SECRET_ACCESS_KEY` ·
/// `AWS_ACCESS_KEY_ID`) ends in neither `_API_KEY` nor `_SECRET`, and
/// the refuter proved it crossed. The predicate is now SUBSTRING-based
/// over the vocabulary that names authority, plus the explicit
/// ambient-authority handles no pattern would catch.
const CREDENTIAL_FRAGMENTS: [&str; 8] = [
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    // Bare `KEY` on purpose (review 2026-08-06): the qualified list
    // (`API_KEY` · `ACCESS_KEY` · `PRIVATE_KEY`) is the same
    // incomplete-vocabulary class the AWS refutation already cost us —
    // a provider that ships `<NAME>_KEY` would have walked through.
    "KEY",
    "AUTH",
    "PASSPHRASE",
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
    /// The accepted `--version` range (spec §4). `None` = unpinned,
    /// which is honest for a locally-built adapter under development
    /// and refused by the B6 registry for shipped rows.
    pub version_pin: Option<crate::probe::VersionPin>,
    /// The argv that makes the ADAPTER print ITS version. Defaults to
    /// `["--version"]`, which is right when `command` IS the adapter —
    /// and WRONG for every wrapper shape (`npx codex-acp`, `node
    /// dist/index.js`, `python -m …`): there the bare flag probes the
    /// WRAPPER (the gauntlet caught `python3 --version` being judged
    /// against a codex pin). A wrapper row overrides this.
    pub version_args: Vec<String>,
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
            version_pin: None,
            version_args: vec!["--version".to_owned()],
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

    /// Pin the accepted `--version` range — the probe then runs BEFORE
    /// every session (spec §4).
    #[must_use]
    pub fn with_version_pin(mut self, pin: crate::probe::VersionPin) -> Self {
        self.version_pin = Some(pin);
        self
    }

    /// Override the version argv for a WRAPPER command (the bare
    /// `--version` would probe the wrapper, not the adapter).
    #[must_use]
    pub fn with_version_args(mut self, args: Vec<String>) -> Self {
        self.version_args = args;
        self
    }
}

/// A spawned, session-ready harness — `AgentBackend` over the
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
        // NOTE (review 2026-08-06): the child deliberately inherits the
        // ENGINE's cwd — the task's `cwd` is a SESSION-scoped logical
        // root carried on the wire (`session/new.cwd`), not a process
        // cwd. An adapter that resolved paths against its OS cwd would
        // escape the intended root; the sandbox backing at B5 is what
        // enforces that, never this spawn.
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

impl SpawnedHarness {
    /// Run `<command> --version` and judge it against the adapter's pin
    /// (spec §4 · identity BEFORE dialect). `Ok(None)` when the adapter
    /// declares no pin.
    ///
    /// # Errors
    ///
    /// The binary cannot spawn, prints no version, or sits outside the
    /// pin — every case an [`HarnessError::Unavailable`] naming the
    /// adapter.
    pub async fn probe_version(&self) -> Result<Option<(u32, u32)>, HarnessError> {
        let Some(pin) = &self.adapter.version_pin else {
            return Ok(None);
        };
        let parent: BTreeMap<String, String> = std::env::vars().collect();
        let env = compose_env(&parent, &self.adapter.passthrough_env);
        let out = tokio::process::Command::new(&self.adapter.command)
            .args(&self.adapter.version_args)
            .env_clear()
            .envs(&env)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();
        // A `--version` that never answers (a TTY prompt · a licence
        // check on the network) hung the verb before a driver existed
        // (review 2026-08-06): the probe is bounded like every other
        // wait, and kill-on-drop reaps the stalled child.
        let out = tokio::time::timeout(PROBE_TIMEOUT, out)
            .await
            .map_err(|_| HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: `{} {}` did not answer in {}s",
                    self.adapter.id,
                    self.adapter.command,
                    self.adapter.version_args.join(" "),
                    PROBE_TIMEOUT.as_secs()
                ),
            })?
            .map_err(|e| HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: cannot probe `{} {}`: {e}",
                    self.adapter.id,
                    self.adapter.command,
                    self.adapter.version_args.join(" ")
                ),
            })?;
        // Some CLIs print their version on stderr — judge both, in
        // stdout-first order (never a guess about which one it is).
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        crate::probe::judge_version(&self.adapter.id, &text, pin).map(Some)
    }
}

// The Send variant is what the house consumes (the `ProviderInferDyn`
// precedent: every impl site writes the `*Dyn` form, and the kernel's
// blanket erasure builds `Arc<dyn DynAgentBackend>` from it).
impl AgentBackendDyn for SpawnedHarness {
    async fn run_agent(&self, request: HarnessRequest) -> Result<HarnessEventStream, HarnessError> {
        // Identity before dialect (spec §4): a version outside the pin
        // refuses HERE, with the version named — never as a protocol
        // confusion three frames into a session.
        self.probe_version().await?;
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
