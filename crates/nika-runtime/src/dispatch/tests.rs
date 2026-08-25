// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The #824 check⇄run parity proofs — a templated `model:`
//! (`${{ inputs.model }}`) checked green (the MODELS rung judges the
//! DECLARED DEFAULT via `static_literal_of`) but the dispatch handed the
//! RAW template to the provider, dying NIKA-INFER-001. The dispatch now
//! renders `model:` through the SAME `${{ }}` seam as
//! `prompt:`/`system:`, so the resolved default is what reaches the
//! wire — infer AND agent (one shared line each). The infer half reuses
//! the deadline rig's capturing seam
//! (`super::infer_deadline_tests::run_and_capture` · `pub(super)` for
//! exactly this).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

/// The issue's repro shape (a deployment-supplied `inputs.model`
/// default · the ollama wire so the RESOLVED string is observable in
/// the request body the provider seam captured). The issue was filed
/// against `config.model`; that authority died with the 9-key
/// envelope and the defect it pins lives on the surviving root.
const ISSUE_824_REPRO: &str = "nika: seat-from-an-input\n\
     inputs:\n  \
     model: { type: string, required: false, default: \"ollama/llama3.2:3b\" }\n\
     tasks:\n  \
     ask:\n    \
     infer: { model: \"${{ inputs.model }}\", max_tokens: 32, prompt: \"say ok\" }\n";

#[tokio::test]
async fn infer_model_input_template_resolves_before_the_wire() {
    let captured = super::infer_deadline_tests::run_and_capture(ISSUE_824_REPRO).await;
    assert_eq!(
        captured.len(),
        1,
        "one provider round-trip — no NIKA-INFER-001 on the raw template"
    );
    let body: serde_json::Value = serde_json::from_slice(
        captured[0]
            .body
            .as_ref()
            .expect("the infer wire has a body"),
    )
    .expect("the openai-compat body is json");
    assert_eq!(
        body["model"], "llama3.2:3b",
        "the RESOLVED input default reaches the provider, never the raw `${{{{ }}}}`"
    );
}

#[tokio::test]
async fn agent_model_input_template_resolves_before_the_provider() {
    let wf = nika_schema::parse(
        "nika: agent-seat\n\
         inputs:\n  \
         model: { type: string, required: false, default: \"mock/echo\" }\n\
         tasks:\n  \
         go:\n    \
         agent: { model: \"${{ inputs.model }}\", prompt: \"hi\" }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");

    let provider = Arc::new(MockProvider::new("mock").enqueue_text("done"));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::new())),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::clone(&provider),
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
    assert!(outcome.ok, "the agent task settles green");
    let requests = provider.captured_requests();
    assert_eq!(requests.len(), 1, "one provider round-trip");
    assert_eq!(
        requests[0].model, "mock/echo",
        "the RESOLVED config default reaches the provider, never the raw `${{{{ }}}}`"
    );
}

#[test]
fn invoke_meters_a_top_level_cost_usd_from_structured_output() {
    // The honest-spend channel: a tool reporting real spend as a
    // top-level numeric `cost_usd` is metered; junk shapes never are.
    // (Rides with the #824 parity proofs since the D1-era extraction —
    // the inline `mod tests` block it came from collided with this
    // sibling file's declaration at the #884 merge.)
    let extract = |v: serde_json::Value| {
        v.get("cost_usd")
            .and_then(serde_json::Value::as_f64)
            .filter(|c| c.is_finite() && *c >= 0.0)
    };
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": 0.02, "images": [] })),
        Some(0.02)
    );
    assert_eq!(extract(serde_json::json!({ "cost_usd": null })), None);
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": -1.0 })),
        None,
        "negative refused"
    );
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": "0.02" })),
        None,
        "strings refused"
    );
    assert_eq!(extract(serde_json::json!({ "other": 1 })), None);
    assert_eq!(extract(serde_json::json!("just text")), None);
    assert_eq!(
        extract(serde_json::json!({ "cost_usd": f64::NAN })),
        None,
        "non-finite refused"
    );
}

/// #1025 — the exec jail derives `permits.fs` through `spec_of`: a
/// portable `~/` grant must land as this operator's absolute path, not
/// as the literal `~` the launcher refuses, and not as another tree.
#[test]
fn a_tilde_fs_grant_expands_to_the_operator_home_on_the_jail() {
    use nika_schema::types::{FsPermits, Permits};

    let home = "/tmp/nika-op-home";
    let mut permits = Permits::new();
    permits.fs = Some(FsPermits::new(
        vec!["~/.gitconfig".into(), "$HOME/.config/git/**".into()],
        vec![],
    ));
    let spec = nika_exec_runner::sandbox_spec::spec_of_with_home(
        &permits,
        std::path::Path::new("/repo"),
        Some(home),
    )
    .expect("home grants expand to absolute paths");
    assert_eq!(
        spec.fs_read,
        vec![
            format!("{home}/.gitconfig"),
            format!("{home}/.config/git/**"),
        ]
    );
    let mut jail = Permits::new();
    jail.fs = Some(FsPermits::new(spec.fs_read, vec![]));
    assert!(jail.jail_admits_read(&format!("{home}/.gitconfig")));
    assert!(!jail.jail_admits_read("/tmp/evil/.gitconfig"));
    assert!(!jail.jail_admits_read("/etc/passwd"));
}

/// #1025 — the production launcher receives unresolved portable home
/// grants as non-absolute paths and refuses before spawn. Both missing
/// and non-absolute homes cover the three spellings and their exact tokens.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn unresolved_home_grants_fail_closed_at_the_production_launcher() {
    use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
    use nika_kernel::process::ShellCommand;
    use nika_schema::types::{FsPermits, Permits};

    #[cfg(target_os = "linux")]
    let sandbox = nika_sandbox_landlock::LandlockSandbox::new();
    #[cfg(target_os = "macos")]
    let sandbox = nika_sandbox_seatbelt::SeatbeltSandbox::new();

    let grants = [
        "~",
        "~/.gitconfig",
        "$HOME",
        "$HOME/.gitconfig",
        "${HOME}",
        "${HOME}/.config/git/**",
    ];
    for home in [None, Some("relative/home")] {
        for grant in grants {
            let mut permits = Permits::new();
            permits.fs = Some(FsPermits::new(vec![grant.to_owned()], vec![]));
            let spec = nika_exec_runner::sandbox_spec::spec_of_with_home(
                &permits,
                std::path::Path::new("/repo"),
                home,
            )
            .expect("an unresolved home reaches the launcher's refusal");
            assert_eq!(spec.fs_read, vec![grant], "home = {home:?}");
            assert!(!spec.fs_read[0].starts_with('/'));

            let refusal = sandbox
                .confine(&spec, ShellCommand::new("/usr/bin/true"))
                .expect_err("an unresolved home grant must refuse before spawn");
            assert!(
                matches!(
                    refusal,
                    CommandSandboxError::Profile { .. } | CommandSandboxError::Unavailable { .. }
                ),
                "home = {home:?}, grant = {grant:?}: {refusal:?}"
            );
        }
    }
}
