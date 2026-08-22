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
    /// The npm-wrapper class (B6 · codex-acp · claude-agent-acp): these
    /// adapters have NO version flag at all (codex-acp rejects it,
    /// claude-agent-acp prints nothing), and the pin rides the exact
    /// package spec in the argv — so the probe is the wire itself:
    /// spawn the session argv, send `initialize`, judge the agent's
    /// self-report (`agentInfo.name` + `agentInfo.version` · the
    /// protocol version). When set, this supersedes `version_args`.
    pub probe_via_handshake: bool,
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
            probe_via_handshake: false,
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

    /// The npm-wrapper class's probe (B6): the version is judged from
    /// the agent's OWN initialize answer (`agentInfo`), not a flag —
    /// these adapters have no working version flag, and their pin rides
    /// the exact package spec in the argv.
    #[must_use]
    pub fn with_handshake_probe(mut self) -> Self {
        self.probe_via_handshake = true;
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
        if self.adapter.probe_via_handshake {
            return self.probe_handshake().await;
        }
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

    /// The npm-wrapper class's probe (B6): spawn the SESSION argv, send
    /// `initialize`, and judge the agent's self-report — `agentInfo.name`
    /// must be this adapter's id (identity, not assumption) and
    /// `agentInfo.version` must sit inside the pin when the row declares
    /// one. Deadlined like every probe (a cold npx resolve gets 30s; a
    /// hung agent is `Unavailable`, never a hang). The child is killed
    /// before any `session/new` — an initialize answer is free of side
    /// effects by construction.
    async fn probe_handshake(&self) -> Result<Option<(u32, u32)>, HarnessError> {
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
        let handshake =
            tokio::time::timeout(std::time::Duration::from_secs(30), handshake_roundtrip(cmd));
        let answered = handshake.await.map_err(|_| HarnessError::Unavailable {
            reason: format!(
                "adapter `{}`: the initialize handshake did not answer in 30s",
                self.adapter.id
            ),
        })??;
        // The answer is a v1 InitializeResult: protocol version 1, and
        // the agent's self-report for identity + version.
        let version = answered
            .pointer("/result/agentInfo/version")
            .and_then(serde_json::Value::as_str)
            .and_then(crate::probe::parse_version);
        let name = answered
            .pointer("/result/agentInfo/name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if name != self.adapter.id {
            return Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: the handshake answered as `{name}` — this binary is not \
                     the adapter it claims to be",
                    self.adapter.id
                ),
            });
        }
        match (&self.adapter.version_pin, version) {
            (Some(pin), Some((major, minor))) if pin.accepts(major, minor) => {
                Ok(Some((major, minor)))
            }
            (Some(pin), Some((major, minor))) => Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: handshake version {major}.{minor} is outside the pin \
                     (>= {}.{} · major <= {})",
                    self.adapter.id, pin.min.0, pin.min.1, pin.max_major
                ),
            }),
            (Some(_), None) => Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: the initialize answer carried no `agentInfo.version`",
                    self.adapter.id
                ),
            }),
            (None, seen) => Ok(seen),
        }
    }
}

/// One initialize roundtrip over the child's pipes (the probe's
/// transport half — write the request, read one bounded line).
async fn handshake_roundtrip(
    mut cmd: tokio::process::Command,
) -> Result<serde_json::Value, HarnessError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut child = cmd.spawn().map_err(|e| HarnessError::Unavailable {
        reason: format!("cannot spawn the adapter: {e}"),
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| HarnessError::Session {
        reason: "the child's stdin was not piped".to_owned(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| HarnessError::Session {
        reason: "the child's stdout was not piped".to_owned(),
    })?;
    let line = crate::wire::request_line(
        1,
        crate::wire::METHOD_INITIALIZE,
        &crate::wire::InitializeParams {
            protocol_version: crate::wire::PROTOCOL_V1,
            client_capabilities: serde_json::json!({}),
        },
    )
    .map_err(|e| HarnessError::Session {
        reason: e.to_string(),
    })?;
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake write: {e}"),
        })?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake write: {e}"),
        })?;
    let mut line = String::new();
    let n = tokio::io::BufReader::new(stdout)
        .read_line(&mut line)
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake read: {e}"),
        })?;
    if n == 0 {
        return Err(HarnessError::Unavailable {
            reason: "the adapter closed its pipe without answering initialize".to_owned(),
        });
    }
    serde_json::from_str(line.trim_end()).map_err(|e| HarnessError::Session {
        reason: format!("the initialize answer is not valid JSON: {e}"),
    })
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
    fn kimi_code_home_crosses_and_kimi_api_key_never_does() {
        let parent: BTreeMap<String, String> = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/u"),
            ("KIMI_CODE_HOME", "/tmp/relocated-kimi"),
            ("KIMI_API_KEY", "sk-never"),
            ("MOONSHOT_API_KEY", "sk-never-too"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let env = compose_env(
            &parent,
            &[
                "KIMI_CODE_HOME".to_owned(),
                "KIMI_API_KEY".to_owned(), // config mistake — refused
            ],
        );
        assert_eq!(
            env.get("KIMI_CODE_HOME").map(String::as_str),
            Some("/tmp/relocated-kimi")
        );
        assert!(!env.contains_key("KIMI_API_KEY"));
        assert!(!env.contains_key("MOONSHOT_API_KEY"));
    }

    #[tokio::test]
    async fn kimi_code_version_probe_judges_the_bare_triple_without_launching_kimi() {
        let dir = std::env::temp_dir().join(format!("nika-kimi-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let script = dir.join("fake-kimi.py");
        std::fs::write(&script, "print('0.37.2')\n").expect("script");
        let path = script.to_string_lossy().into_owned();
        let in_pin = HarnessAdapter::new("kimi-code", "python3")
            .expect("id is no class token")
            .with_args(vec!["acp".to_owned()])
            .with_version_args(vec![path.clone()])
            .with_version_pin(crate::probe::VersionPin::new((0, 37), 0));
        assert_eq!(in_pin.args, vec!["acp".to_owned()]);
        assert!(!in_pin.probe_via_handshake);
        let seen = SpawnedHarness::new(in_pin)
            .probe_version()
            .await
            .expect("in pin");
        assert_eq!(seen, Some((0, 37)));

        std::fs::write(&script, "print('0.36.0')\n").expect("old");
        let old = HarnessAdapter::new("kimi-code", "python3")
            .expect("id is fine")
            .with_args(vec!["acp".to_owned()])
            .with_version_args(vec![path])
            .with_version_pin(crate::probe::VersionPin::new((0, 37), 0));
        let err = SpawnedHarness::new(old)
            .probe_version()
            .await
            .expect_err("below the floor");
        let msg = err.to_string();
        assert!(msg.contains("kimi-code"), "{msg}");
        assert!(msg.contains("0.36"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
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

    /// The handshake probe (the npm-wrapper class): a fake agent answers
    /// initialize with its `agentInfo` self-report — judged by name AND
    /// version, never trusted from the argv alone.
    fn handshake_agent(
        dir: &std::path::Path,
        id: &str,
        reports: &str,
        version: &str,
    ) -> HarnessAdapter {
        let script = dir.join(format!("agent-{reports}.py"));
        std::fs::write(
            &script,
            format!(
                r#"import json, sys
line = sys.stdin.readline()
req = json.loads(line)
assert req["method"] == "initialize"
print(json.dumps({{"jsonrpc":"2.0","id":req["id"],"result":{{"protocolVersion":1,"agentInfo":{{"name":"{reports}","version":"{version}"}}}}}}))
sys.stdout.flush()
"#
            ),
        )
        .expect("script");
        let path = script.to_string_lossy().into_owned();
        HarnessAdapter::new(id, "python3")
            .expect("id is fine")
            .with_args(vec![path])
            .with_handshake_probe()
            .with_version_pin(crate::probe::VersionPin::new((0, 16), 0))
    }

    #[tokio::test]
    async fn the_handshake_probe_judges_the_agents_own_report() {
        let dir = std::env::temp_dir().join(format!("nika-hs-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        // In-pin + right name → detected with the version read.
        let ok = SpawnedHarness::new(handshake_agent(&dir, "codex-acp", "codex-acp", "0.16.2"));
        assert_eq!(ok.probe_version().await.expect("in pin"), Some((0, 16)));
        // Out-of-pin → refused with both sides named.
        let old = SpawnedHarness::new(handshake_agent(&dir, "codex-acp", "codex-acp", "0.9.0"));
        let err = old.probe_version().await.expect_err("below the floor");
        assert!(err.to_string().contains("0.9"), "{err}");
        // A DIFFERENT name answering → not the adapter it claims to be.
        let impostor = SpawnedHarness::new(handshake_agent(
            &dir,
            "claude-agent-acp",
            "qwen-code",
            "0.21.0",
        ));
        let err = impostor
            .probe_version()
            .await
            .expect_err("the impostor refuses");
        assert!(err.to_string().contains("qwen-code"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
