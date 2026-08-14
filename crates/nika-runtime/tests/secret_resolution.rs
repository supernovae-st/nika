// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Workflow `secrets:` resolution at run time (MINOR-B · spec 01 §secrets).
//!
//! The composer injects a [`WorkflowSecretResolver`] (env/file · the
//! sanctioned store boundary). This battery proves the end-to-end runtime
//! contract over mock seams ·
//!
//! - a resolved `secrets.X` reaches a SANCTIONED sink (the IFC cleared it),
//! - the value is NEVER written to the event stream (the masking contract),
//! - an UNRESOLVED secret (store miss) fails the task cleanly with
//!   NIKA-1702 — never a panic, never a silent "null".

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_check::check;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{
    DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, SecretResolveError, TaskStatus,
    VecSink, WorkflowSecretResolver,
};
use nika_schema::types::SecretRef;
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// A scripted secret resolver (the composer's role · maps `name → value`
/// without touching env/files). A miss returns the typed error the runtime
/// treats as « leave this secret unbound » (fail-closed).
struct MapSecrets(BTreeMap<String, String>);

impl WorkflowSecretResolver for MapSecrets {
    fn resolve(&self, name: &str, _reference: &SecretRef) -> Result<String, SecretResolveError> {
        self.0.get(name).cloned().ok_or_else(|| SecretResolveError {
            name: name.to_owned(),
            reason: "absent".to_owned(),
        })
    }
}

/// Run `yaml` over the mock seams with `resolver` injected — asserts the
/// fixture is statically clean (the sanction is the egress, not a hole).
async fn run_with_secrets(
    yaml: &str,
    shell: MockShell,
    resolver: Arc<dyn WorkflowSecretResolver>,
) -> (RunOutcome, Vec<nika_event::Event>) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the static ladder: {report:?}"
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
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
    )
    .with_secret_resolver(resolver);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run completes (a broken secret fails the TASK, not the run)");
    (outcome, sink.into_events())
}

/// `secrets.X` resolves at run time when the composer injected a resolver —
/// the value reaches a SANCTIONED sink but is NEVER written to the event
/// stream (the masking contract · spec 04 §secrets).
#[tokio::test]
async fn workflow_secret_resolves_into_a_sanctioned_sink_only() {
    // The secret is sanctioned to `exec` (egress · so check passes) and used
    // in the exec command. The resolver provides its value at run start.
    let yaml = r#"
nika: secret-run
permits: { exec: true }
secrets:
  token:
    source: env
    key: MY_TOKEN
    egress:
      - to: "exec"
tasks:
  use_it:
    exec: { command: ["deploy", "--auth", "${{ secrets.token }}"] }
"#;
    let shell = MockShell::new().enqueue_ok("deployed\n");
    let resolver = Arc::new(MapSecrets(BTreeMap::from([(
        "token".to_owned(),
        "sk-SECRET-VALUE".to_owned(),
    )])));
    let (outcome, events) = run_with_secrets(yaml, shell.clone(), resolver).await;

    // 1 · the resolved value REACHED the exec command (the sanctioned sink).
    assert!(outcome.ok);
    assert_eq!(outcome.records["use_it"].status, TaskStatus::Success);
    let commands = shell.executed_commands();
    assert_eq!(commands.len(), 1, "the exec ran once");
    // argv form (0.103): the secret rides its own argument, never the
    // program — assert over the whole argv line.
    let argv_line = std::iter::once(commands[0].program.as_str())
        .chain(commands[0].args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        argv_line.contains("sk-SECRET-VALUE"),
        "the resolved secret reached the sanctioned exec sink: {argv_line}"
    );

    // 2 · the secret value NEVER appears in the event stream (every field of
    //     every emitted event) — the masking contract.
    for event in &events {
        for f in &event.fields {
            if let FieldValue::String(s) = &f.value {
                assert!(
                    !s.contains("sk-SECRET-VALUE"),
                    "secret leaked into event {:?} field `{}`: {s}",
                    event.kind,
                    f.key
                );
            }
        }
    }
}

/// An absent env-var secret stays UNRESOLVED → the referencing task fails
/// cleanly with NIKA-1702 (a typed error · never a panic · never a silent
/// "null" substituted into the command).
#[tokio::test]
async fn absent_workflow_secret_is_a_clean_typed_error() {
    let yaml = r#"
nika: secret-missing
permits: { exec: true }
secrets:
  token:
    source: env
    key: ABSENT
    egress:
      - to: "exec"
tasks:
  use_it:
    exec: { command: ["deploy", "--auth", "${{ secrets.token }}"] }
"#;
    // The resolver returns nothing (the env var is absent) → the secret is
    // unbound → the reference is the loud unresolved class. Its WIRE code is
    // the spec-plane NIKA-VAR-001, never the engine-internal NIKA-1702
    // (spec 05 §142 · internal codes never reach tasks.X.error).
    let resolver = Arc::new(MapSecrets(BTreeMap::new()));
    let (outcome, _events) = run_with_secrets(yaml, MockShell::new(), resolver).await;
    assert!(!outcome.ok);
    let err = outcome.records["use_it"]
        .error
        .as_ref()
        .expect("the task failed cleanly");
    assert_eq!(
        err.code, "NIKA-VAR-001",
        "an unresolved secret is the loud unresolved class (NIKA-VAR-001), not a silent null"
    );
    assert!(
        !err.code.contains("1702"),
        "no engine-internal code leaks: {}",
        err.code
    );
}

/// With NO resolver injected (the default `NoSecrets`), a `secrets.X`
/// reference is unbound — fail-closed exactly as before MINOR-B.
#[tokio::test]
async fn no_resolver_keeps_secrets_unbound_fail_closed() {
    let yaml = r#"
nika: secret-none
permits: { exec: true }
secrets:
  token:
    source: env
    key: MY_TOKEN
    egress:
      - to: "exec"
tasks:
  use_it:
    exec: { command: ["deploy", "--auth", "${{ secrets.token }}"] }
"#;
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
    let report = check(&wf);
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    // Runtime::new defaults to NoSecrets — no with_secret_resolver call.
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
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
        .expect("run completes");
    assert!(
        !outcome.ok,
        "no resolver → the secret is unbound → the task fails"
    );
    assert_eq!(
        outcome.records["use_it"]
            .error
            .as_ref()
            .expect("typed error")
            .code,
        "NIKA-VAR-001",
        "an unbound secret is the unresolved-reference class · wire code \
         NIKA-VAR-001, never the engine-internal NIKA-1702 (spec 05 §142)"
    );
}
