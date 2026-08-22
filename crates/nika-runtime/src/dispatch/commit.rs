// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F-P6 · PREVIEW→COMMIT — the judged request IS the fired request (the
//! TOCTOU between the re-gate and the sink, closed deterministically).
//!
//! The F-O1 re-gate (NEP-0004 law 2) judges the RESOLVED form of a step's
//! request — but nothing proved the bytes FIRED are the bytes JUDGED. The
//! binding, à la `SecureClaw`:
//!
//! 1. **Preview** — at the resolution point (argv rendered · args
//!    materialized · the re-gate passed) the runtime hashes the CANONICAL
//!    request (`preview_digest`).
//! 2. **Commit** — at the firing point (the only doors to the sink) the
//!    digest is recomputed over the about-to-fire bytes · equality required.
//! 3. **Divergence = fail-closed + finding** — the fire is refused
//!    ([`DIVERGENCE_CODE`], never transient) + the `divergence:
//!    {preview, commit}` finding rides the terminal frame (never a warn).
//! 4. **Attestation** — the terminal frame of every fired exec/invoke
//!    step carries both digests (additive fields · no `trace_format` bump).
//!
//! The canonical request is WHAT THE JUDGMENT SAW — fields that change
//! BETWEEN preview and commit by construction are EXCLUDED, or every
//! honest run would self-refuse:
//!
//! - exec · INCLUDED: the command form (`argv` ordered · `shell` line),
//!   `cwd`, `env`, `stdin`. EXCLUDED: `sandbox` + `env_passthrough`
//!   (authority derived AFTER the preview), `capture` + `raw_capture`
//!   (output shaping), `timeout` (scheduling).
//! - invoke · INCLUDED: the tool id + the rendered `args`. EXCLUDED:
//!   `call_id` (engine-derived identity, never judged).
//!
//! The digest law is the receipt family's own ([`crate::resume::jcs_blake3_hex`]
//! · JCS + number-fold + blake3). DETERMINISTIC — never a judge-LLM.

use nika_verb_exec::{ExecCommand, ExecInput};
use nika_verb_invoke::InvokeInput;
use serde_json::{Value, json};

/// The wire code the divergence refusal speaks — the `security_error`
/// family row after `NIKA-SEC-010` (the F-P4 approval). Catchable by
/// `on_error.on_codes:` like every spec-plane security stop; never
/// transient (a mutated request is not healed by a retry).
pub(crate) const DIVERGENCE_CODE: &str = "NIKA-SEC-011";

/// The canonical-request recipe version (the resume `KEY_VERSION` /
/// F-P4 `CONTENT_RECIPE_VERSION` precedent): bump on any shape change —
/// older digests simply mismatch, honest, never a wrong bind.
const REQUEST_RECIPE_VERSION: u32 = 1;

/// The binding evidence of ONE fired step — the two digests, equal on an
/// honest fire. Rides the terminal frame as `preview_digest` +
/// `commit_digest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitAttestation {
    /// The judged request's digest (taken at the resolution point).
    pub preview_digest: String,
    /// The fired request's digest (recomputed at the firing point).
    pub commit_digest: String,
}

/// The fail-closed finding — the judged digest vs the fired digest.
/// Rides the terminal frame as one `divergence` field (compact JSON text,
/// the `child` row precedent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Divergence {
    /// What the judgment saw.
    pub preview: String,
    /// What was about to fire.
    pub commit: String,
}

impl Divergence {
    /// The frame field value — `{"preview":"…","commit":"…"}` (one compact
    /// JSON text · the `child` precedent: object fields ride as text).
    #[must_use]
    pub(crate) fn frame_json(&self) -> String {
        json!({ "preview": self.preview, "commit": self.commit }).to_string()
    }
}

/// The commit gate's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitGate {
    /// `preview == commit` — fire, and attest the equality.
    Fire(CommitAttestation),
    /// The about-to-fire request is NOT the judged one — refuse the fire.
    Divergence(Divergence),
}

/// The binding evidence of ONE effectful dispatch — XOR by construction:
/// the gate FIRED (the judged ≡ the fired, attested) or REFUSED (the
/// finding). One field on the dispatch/attempt/settle channels, so the
/// two states can never both ride (and the cold payloads stay boxed —
/// the `DispatchOk::child` precedent: evidence must not widen the hot
/// result channels).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitEvidence {
    /// The gate passed — both digests, equal.
    Fired(Box<CommitAttestation>),
    /// The gate refused the fire — the fail-closed finding.
    Refused(Box<Divergence>),
}

impl super::Dispatched {
    /// Attach the F-P6 attestation to whichever outcome fired — success
    /// AND post-gate verb failure alike (the refusal never passes here).
    pub(super) fn with_commit(mut self, attestation: CommitAttestation) -> Self {
        match &mut self.result {
            Ok(ok) => ok.commit = Some(Box::new(attestation)),
            Err(failed) => {
                failed.evidence = Some(CommitEvidence::Fired(Box::new(attestation)));
            }
        }
        self
    }

    /// F-P6 · the gate's fail-closed refusal: the about-to-fire request
    /// is NOT the judged one — zero byte reached the sink; the finding
    /// rides the terminal frame (never transient · never silent).
    pub(super) fn divergence_refusal(note: &str, divergence: Divergence) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(super::FailedDispatch {
                record: crate::record::TaskErrorRecord {
                    code: DIVERGENCE_CODE.to_owned(),
                    message: format!(
                        "preview/commit divergence on `{note}` — the fired request is not \
                         the judged request (the digest recomputed at the firing point \
                         disagrees with the judged form · zero byte reached the sink)"
                    ),
                    transient: false,
                },
                cost_usd: None,
                cost_source: None,
                cost_unpriced: None,
                access_receipt: None,
                evidence: Some(CommitEvidence::Refused(Box::new(divergence))),
            }),
        }
    }
}

impl<S, T, H, P, D, C> super::Runtime<S, T, H, P, D, C>
where
    S: nika_kernel::process::ShellRunDyn + Sync,
    T: nika_kernel::tool_executor::ToolExecuteDyn,
    H: nika_kernel::http::HttpPostDyn + Send + Sync + 'static,
    P: nika_kernel::ai::provider::ProviderInferDyn + nika_kernel::ai::provider::ProviderMeta,
    D: nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn,
{
    /// F-P6 · the invoke firing lane: PREVIEW (the digest at the
    /// resolution point — re-gate passed) → COMMIT gate → the tool call.
    // The `mut` serves the cfg(test) tamper seam only (production never
    // mutates between preview and commit — that is the whole point).
    #[cfg_attr(not(test), allow(unused_mut))]
    pub(super) async fn run_invoke_gated(
        &self,
        note: String,
        mut input: InvokeInput,
    ) -> super::Dispatched {
        let Some(preview) = digest(&invoke_request(&input)) else {
            return super::Dispatched::unwired(
                &note,
                "invoke request cannot canonicalize (F-P6)".to_owned(),
            );
        };
        #[cfg(test)]
        test_tamper::invoke(&mut input);
        match gate(&preview, &invoke_request(&input)) {
            Some(CommitGate::Fire(attestation)) => match self.invoke.run(input).await {
                // A tool's typed value (builtins · MCP `structuredContent`)
                // flows to tasks.X.output AS ITSELF (spec 04 §tasks.X.output);
                // a text-only tool stays a String — never JSON-coerced.
                Ok(out) => {
                    let value = match out.structured {
                        Some(value) => value,
                        None => Value::String(out.content),
                    };
                    // A tool's self-reported REAL spend (top-level numeric
                    // `cost_usd` in its structured output) is metered into
                    // the run ledger — absent/invalid → unmetered, never a
                    // guess (the honest-absence channel).
                    let cost_usd = value
                        .get("cost_usd")
                        .and_then(Value::as_f64)
                        .filter(|c| c.is_finite() && *c >= 0.0);
                    let cost_source = cost_usd
                        .is_some()
                        .then(|| note.trim_start_matches("invoke · ").to_owned());
                    super::Dispatched::ok_metered(
                        note,
                        value,
                        None,
                        None,
                        cost_usd,
                        cost_source,
                        None,
                    )
                    .with_commit(attestation)
                }
                Err(err) => super::Dispatched::verb_err(note, &err).with_commit(attestation),
            },
            Some(CommitGate::Divergence(divergence)) => {
                super::Dispatched::divergence_refusal(&note, divergence)
            }
            None => super::Dispatched::unwired(
                &note,
                "invoke request cannot canonicalize at fire (F-P6)".to_owned(),
            ),
        }
    }

    /// F-P6 · the exec firing lane: PREVIEW (argv/cwd/env/stdin resolved ·
    /// re-gate passed — the derivations BELOW are excluded from the request
    /// by law) → jail + passthrough → COMMIT gate → the spawn.
    pub(super) async fn run_exec_gated(
        &self,
        note: String,
        mut input: ExecInput,
        permits: Option<&nika_schema::types::Permits>,
        witness: &crate::witness::PermitWitness,
        decode: nika_schema::DecodeMode,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> super::Dispatched {
        let Some(preview) = exec_request(&input).and_then(|r| digest(&r)) else {
            return super::Dispatched::unwired(
                &note,
                "exec request cannot canonicalize (F-P6)".to_owned(),
            );
        };
        // ADR-095 Layer 6 · the jail rides the declared boundary (F-O8) ·
        // NEP-0009: a planted symlink refuses HERE, before any spawn.
        input.sandbox = match self.exec_sandbox_spec(permits, &note, witness) {
            Ok(spec) => Some(spec),
            Err(refusal) => return *refusal,
        };
        // NEP-0005 · the declared `permits.env:` passthrough rides the
        // command to the spawn site (a cleared slate — nothing ambient
        // crosses undeclared).
        input.env_passthrough = super::permits::env_passthrough_witnessed(permits, witness);
        #[cfg(test)]
        test_tamper::exec(&mut input);
        match exec_request(&input).and_then(|r| gate(&preview, &r)) {
            Some(CommitGate::Fire(attestation)) => match self.shell.run(input).await {
                Ok(out) => {
                    super::settle_exec_out(&note, out, decode, contract).with_commit(attestation)
                }
                Err(err) => super::Dispatched::verb_err(note, &err).with_commit(attestation),
            },
            Some(CommitGate::Divergence(divergence)) => {
                super::Dispatched::divergence_refusal(&note, divergence)
            }
            None => super::Dispatched::unwired(
                &note,
                "exec request cannot canonicalize at fire (F-P6)".to_owned(),
            ),
        }
    }
}

/// The canonical exec request (see module docs for the include/exclude
/// law). `None` = a command form the runtime does not wire (the loud
/// `#[non_exhaustive]` door — the caller refuses, never guesses).
pub(crate) fn exec_request(input: &ExecInput) -> Option<Value> {
    let command = match &input.command {
        ExecCommand::Shell(line) => json!({ "shell": line.clone() }),
        ExecCommand::Argv(argv) => json!({ "argv": argv.clone() }),
        _ => return None,
    };
    Some(json!({
        "v": REQUEST_RECIPE_VERSION,
        "exec": {
            "command": command,
            "cwd": input.cwd.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "env": input.env.clone(),
            "stdin": input.stdin.clone(),
        },
    }))
}

/// The canonical invoke request — the tool id + the rendered args (the
/// re-gate's judged surface for the `mcp:` border).
pub(crate) fn invoke_request(input: &InvokeInput) -> Value {
    json!({
        "v": REQUEST_RECIPE_VERSION,
        "invoke": {
            "tool": input.tool.clone(),
            "args": input.args.clone(),
        },
    })
}

/// blake3 hex over the request's canonical bytes — the receipt family's
/// ONE digest law (JCS + number-fold, so a u64 arg beyond 2^53 can never
/// collide with its double). `None` = the shape cannot canonicalize
/// (unreachable for these builders — the caller fails closed anyway).
pub(crate) fn digest(request: &Value) -> Option<String> {
    crate::resume::jcs_blake3_hex(request)
}

/// The COMMIT half of the binding: recompute the digest over the
/// about-to-fire request and judge it against the `preview` taken at the
/// resolution point. `None` = the fired shape cannot canonicalize (the
/// caller refuses — never fire unattested).
pub(crate) fn gate(preview: &str, request: &Value) -> Option<CommitGate> {
    let commit = digest(request)?;
    Some(if commit == preview {
        CommitGate::Fire(CommitAttestation {
            preview_digest: preview.to_owned(),
            commit_digest: commit,
        })
    } else {
        CommitGate::Divergence(Divergence {
            preview: preview.to_owned(),
            commit,
        })
    })
}

/// The TOCTOU injection seam — compiled under `cfg(test)` ONLY: the
/// negative fixtures' « hook de test » mutating the resolved request
/// BETWEEN preview and commit (the design's 1-bit argv mutation · the
/// zero-arg tool's ctx mutation). Marker-keyed (a canned program/tool id
/// arms it), so the seam is deterministic and race-free across the
/// parallel test binary. Production code never reaches this module.
#[cfg(test)]
pub(crate) mod test_tamper {
    use super::*;

    /// The canned argv[0] arming the exec mutation.
    pub(crate) const EXEC_MARKER: &str = "fp6-tamper";
    /// The canned tool-id prefix arming the invoke mutation (inside the
    /// legal `mcp:` namespace — v1 admits `nika:` and `mcp:` only).
    pub(crate) const INVOKE_MARKER: &str = "mcp:fp6-tamper/";

    /// Mutate one bit of the rendered argv when armed — the fired form
    /// now carries a token the judgment never saw.
    pub(crate) fn exec(input: &mut ExecInput) {
        if let ExecCommand::Argv(argv) = &mut input.command
            && argv.first().is_some_and(|p| p == EXEC_MARKER)
            && let Some(last) = argv.last_mut()
        {
            last.push('X');
        }
    }

    /// Inject an argument into a zero-arg tool call when armed — the ctx
    /// alone mutates, no args were judged.
    pub(crate) fn invoke(input: &mut InvokeInput) {
        if input.tool.starts_with(INVOKE_MARKER)
            && let Some(obj) = input.args.as_object_mut()
        {
            obj.insert("tampered".to_owned(), Value::Bool(true));
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn argv_input(argv: &[&str]) -> ExecInput {
        ExecInput::argv(argv.iter().map(ToOwned::to_owned))
    }

    fn request_digest(value: &Value) -> String {
        digest(value).expect("the builders' shapes canonicalize")
    }

    // ── The canonical form (item 1) ─────────────────────────────────────

    #[test]
    fn argv_order_is_significant() {
        // JCS of an ORDERED array: the same tokens permuted judge differently.
        let a = request_digest(&exec_request(&argv_input(&["git", "status"])).expect("wired"));
        let b = request_digest(&exec_request(&argv_input(&["status", "git"])).expect("wired"));
        assert_ne!(a, b, "argv order is part of the judged form");
    }

    #[test]
    fn env_key_order_is_not_significant() {
        // JCS of a map: key order canonicalizes away (BTreeMap already
        // sorts — the pin is against a regression to insertion-ordered maps).
        let mut x = argv_input(&["printenv"]);
        x.env.insert("A".to_owned(), "1".to_owned());
        x.env.insert("B".to_owned(), "2".to_owned());
        let mut y = argv_input(&["printenv"]);
        y.env.insert("B".to_owned(), "2".to_owned());
        y.env.insert("A".to_owned(), "1".to_owned());
        assert_eq!(
            request_digest(&exec_request(&x).expect("wired")),
            request_digest(&exec_request(&y).expect("wired")),
            "the env map judges by content, never by order"
        );
    }

    #[test]
    fn runtime_derived_fields_are_excluded() {
        // The living-wall law: fields set BETWEEN preview and commit by
        // construction (sandbox · env_passthrough) and the output-shaping
        // knobs (capture · raw_capture) never move the digest — or every
        // honest run would self-refuse.
        let judged = argv_input(&["git", "status"]);
        let mut fired = judged.clone();
        fired.env_passthrough.push("AMBIENT_DECLARED".to_owned());
        fired.sandbox = Some(nika_kernel::process::SandboxSpec::default());
        fired.capture = nika_verb_exec::CaptureMode::Structured;
        fired.raw_capture = true;
        fired.timeout = Some(std::time::Duration::from_secs(30));
        assert_eq!(
            request_digest(&exec_request(&judged).expect("wired")),
            request_digest(&exec_request(&fired).expect("wired")),
            "sandbox · env_passthrough · capture · timeout are not the request"
        );
    }

    #[test]
    fn judged_fields_are_all_significant() {
        // cwd · env · stdin are resolved at the judgment: each moves the digest.
        let base = argv_input(&["git", "status"]);
        let mut with_cwd = base.clone();
        with_cwd.cwd = Some("/tmp".into());
        let mut with_env = base.clone();
        with_env.env.insert("A".to_owned(), "1".to_owned());
        let mut with_stdin = base.clone();
        with_stdin.stdin = Some("payload".to_owned());
        let base_digest = request_digest(&exec_request(&base).expect("wired"));
        for variant in [&with_cwd, &with_env, &with_stdin] {
            assert_ne!(
                base_digest,
                request_digest(&exec_request(variant).expect("wired")),
                "a judged field mutation must move the digest"
            );
        }
    }

    #[test]
    fn the_shell_form_is_covered_and_distinct() {
        // One law for both command forms — and the two forms never collide.
        let argv = request_digest(&exec_request(&argv_input(&["git", "status"])).expect("wired"));
        let shell = request_digest(&exec_request(&ExecInput::shell("git status")).expect("wired"));
        assert_ne!(argv, shell, "argv and shell are different fired forms");
    }

    #[test]
    fn invoke_args_judge_by_content_with_number_fidelity() {
        // The digest law folds numbers: a u64 beyond 2^53 never collides
        // with its ES6-double spelling (the resume law's one unforgivable).
        let mut big = InvokeInput::new("mcp:x/y");
        big.args = json!({ "n": 9_007_199_254_740_993_u64 });
        let mut bigger = InvokeInput::new("mcp:x/y");
        bigger.args = json!({ "n": 9_007_199_254_740_994_u64 });
        assert_ne!(
            request_digest(&invoke_request(&big)),
            request_digest(&invoke_request(&bigger)),
            "int64 fidelity beyond 2^53 (RFC 8785 alone would collide them)"
        );
        // …and the zero-arg call has its own stable identity.
        let zero = InvokeInput::new("mcp:x/y");
        assert_eq!(
            request_digest(&invoke_request(&zero)),
            request_digest(&invoke_request(&InvokeInput::new("mcp:x/y"))),
            "the same judged request digests identically"
        );
    }

    #[test]
    fn the_recipe_version_is_pinned() {
        // A shape change bumps REQUEST_RECIPE_VERSION — older digests
        // mismatch, never a wrong bind. This pins today's shape verbatim.
        let request = exec_request(&argv_input(&["echo", "hi"])).expect("wired");
        assert_eq!(
            request,
            json!({
                "v": 1,
                "exec": {
                    "command": { "argv": ["echo", "hi"] },
                    "cwd": null,
                    "env": {},
                    "stdin": null,
                },
            })
        );
        let invoke = invoke_request(&InvokeInput::new("nika:echo"));
        assert_eq!(
            invoke,
            json!({ "v": 1, "invoke": { "tool": "nika:echo", "args": {} } })
        );
    }

    // ── The gate (item 3) ───────────────────────────────────────────────

    #[test]
    fn an_unchanged_request_fires_with_matching_digests() {
        let request = exec_request(&argv_input(&["echo", "hi"])).expect("wired");
        let preview = request_digest(&request);
        match gate(&preview, &request) {
            Some(CommitGate::Fire(att)) => {
                assert_eq!(att.preview_digest, att.commit_digest);
                assert_eq!(att.preview_digest.len(), 64, "blake3 hex");
            }
            other => panic!("an unchanged request fires: {other:?}"),
        }
    }

    #[test]
    fn one_bit_of_divergence_refuses_the_fire() {
        let judged = exec_request(&argv_input(&["echo", "hi"])).expect("wired");
        let preview = request_digest(&judged);
        let fired = exec_request(&argv_input(&["echo", "hiX"])).expect("wired");
        match gate(&preview, &fired) {
            Some(CommitGate::Divergence(d)) => {
                assert_eq!(d.preview.len(), 64);
                assert_ne!(d.preview, d.commit, "the finding names both sides");
                // The finding's frame form is the {preview, commit} object.
                let finding: Value = serde_json::from_str(&d.frame_json()).expect("json");
                assert_eq!(finding["preview"], Value::String(d.preview.clone()));
                assert_eq!(finding["commit"], Value::String(d.commit.clone()));
            }
            other => panic!("a mutated request must refuse: {other:?}"),
        }
    }
}

/// The negative fixtures, end to end (design §4 b/c): the request mutates
/// BETWEEN preview and commit (the `test_tamper` seam — compiled under
/// cfg(test) only) and the fire is REFUSED with the finding on the
/// terminal frame · zero byte reaches the sink. Every effect seam is
/// mocked: the refusal is the production code path with zero I/O.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod e2e {
    use std::sync::Arc;

    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;
    use serde_json::Value;

    use super::test_tamper::{EXEC_MARKER, INVOKE_MARKER};
    use super::{DIVERGENCE_CODE, digest, exec_request};
    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

    /// Run one check-clean workflow on the mock plane; the mocks come
    /// back for the side-effect counts (zero = the refusal is pre-fire).
    async fn run(yaml: &str) -> (RunOutcome, VecSink, MockShell, Arc<MockToolExecutor>) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(
            report.is_clean(),
            "the commit gate is a RUNTIME law — the static ladder stays clean: {:?}",
            report.findings
        );
        let shell = MockShell::new().enqueue_ok("fired\n");
        let tools = Arc::new(MockToolExecutor::new().enqueue_ok(
            nika_kernel::tool_executor::ToolResult::success("r1", "fired"),
        ));
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(shell.clone())),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        (outcome, sink, shell, tools)
    }

    fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
        event
            .fields
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| match &kv.value {
                nika_types::resource::Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    /// The `divergence` finding of the named task's terminal frame.
    fn divergence_finding(sink: &VecSink, task: &str) -> Value {
        let frame = sink
            .events()
            .iter()
            .find(|e| {
                e.kind == nika_event::EventKind::TaskFailed && str_field(e, "task") == Some(task)
            })
            .expect("the task_failed frame");
        let text = str_field(frame, "divergence").expect("the divergence finding rides");
        serde_json::from_str(text).expect("the finding is one JSON object")
    }

    /// Design §4 (b) — the 1-bit argv mutation post-preview: COMMIT
    /// REFUSED + the finding · ZERO bytes executed.
    #[tokio::test]
    async fn a_post_preview_argv_mutation_refuses_the_spawn() {
        let yaml = format!(
            "nika: fp6\npermits:\n  exec: [\"{EXEC_MARKER}\"]\ntasks:\n  fire:\n    exec: {{ command: [\"{EXEC_MARKER}\", \"safe-token\"] }}\n"
        );
        let (outcome, sink, shell, _) = run(&yaml).await;
        assert!(!outcome.ok, "a divergence fails the run");
        let record = &outcome.records["fire"];
        let err = record.error.as_ref().expect("the refusal record");
        assert_eq!(err.code, DIVERGENCE_CODE);
        assert!(!err.transient, "a mutated request is never retried");
        assert!(
            shell.executed_commands().is_empty(),
            "COMMIT REFUSÉ — zero byte reached the runner"
        );
        // The finding names both digests — and the preview IS the digest
        // of the request the judgment saw (the UNtampered argv).
        let finding = divergence_finding(&sink, "fire");
        let preview = finding["preview"].as_str().expect("hex");
        let commit = finding["commit"].as_str().expect("hex");
        assert_ne!(preview, commit);
        let judged = nika_verb_exec::ExecInput::argv([EXEC_MARKER, "safe-token"]);
        let expected = digest(&exec_request(&judged).expect("wired")).expect("canonical");
        assert_eq!(preview, expected, "the preview names the judged form");
        // No digest pair rides a REFUSED step (nothing fired to attest).
        let frame = sink
            .events()
            .iter()
            .find(|e| e.kind == nika_event::EventKind::TaskFailed)
            .expect("the frame");
        assert!(str_field(frame, "preview_digest").is_none());
        assert!(str_field(frame, "commit_digest").is_none());
    }

    /// Design §4 (c) — the zero-arg tool whose ctx alone mutates
    /// post-preview: COMMIT REFUSED + the finding · zero tool call.
    #[tokio::test]
    async fn a_zero_arg_tool_ctx_mutation_refuses_the_call() {
        let yaml = format!(
            "nika: fp6\npermits:\n  tools: [\"{INVOKE_MARKER}zero\"]\ntasks:\n  call:\n    invoke: {{ tool: \"{INVOKE_MARKER}zero\" }}\n"
        );
        let (outcome, sink, _, tools) = run(&yaml).await;
        assert!(!outcome.ok, "a divergence fails the run");
        let err = outcome.records["call"].error.as_ref().expect("the refusal");
        assert_eq!(err.code, DIVERGENCE_CODE);
        assert!(!err.transient);
        assert!(
            tools.captured_calls().is_empty(),
            "COMMIT REFUSÉ — zero tool call reached the executor"
        );
        let finding = divergence_finding(&sink, "call");
        assert_ne!(
            finding["preview"].as_str(),
            finding["commit"].as_str(),
            "the ctx mutation is caught: an empty-args map gained a member"
        );
    }
}
