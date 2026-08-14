// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The skill-compose proofs (#473 · spec 02 §agent skills) — split out
//! of the crate-root `tests.rs` at the C2 wall (the 1500-LOC file
//! ratchet) to live beside their subject.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel::provider::Role;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::dispatch::system_with_skills;
use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

#[test]
fn skills_section_shape_is_deterministic() {
    let docs = vec![
        nika_schema::SkillDoc::new("alpha", "First skill.", "\n# Alpha\n\nDo alpha things.\n"),
        nika_schema::SkillDoc::new("beta", "Second skill.", ""),
    ];
    // With an authored system — the section appends after ONE blank line.
    let with_system = system_with_skills(Some("You are helpful.".to_owned()), &docs);
    assert_eq!(
        with_system,
        "You are helpful.\n\n## Skills\n\n### alpha\n\nFirst skill.\n\n# Alpha\n\nDo alpha things.\n\n### beta\n\nSecond skill.",
        "the injection bytes are the documented shape"
    );
    // Without one — the section IS the system prompt.
    let bare = system_with_skills(None, &docs[..1]);
    assert!(bare.starts_with("## Skills\n\n### alpha"), "{bare}");
}

const SKILL_MD: &str =
    "---\nname: reviewer\ndescription: Review with care.\n---\n\nAlways review twice.\n";

fn wf_with_skill() -> nika_schema::raw::RawWorkflow {
    nika_schema::parse(
            "nika: w\nmodel: mock/echo\ntasks:\n  go:\n    agent:\n      system: \"Base system.\"\n      prompt: \"hello\"\n      skills: [\"skills/reviewer/SKILL.md\"]\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
}

fn runtime_with(
    provider: MockProvider,
    skills: BTreeMap<String, String>,
) -> Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
> {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_skills(skills)
}

#[tokio::test]
async fn resolved_skills_reach_the_provider_system_message() {
    let wf = wf_with_skill();
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the static ladder is fs-free — clean");
    let provider = MockProvider::new("mock").enqueue_text("done");
    let probe = provider.clone();
    let runtime = runtime_with(
        provider,
        BTreeMap::from([("skills/reviewer/SKILL.md".to_owned(), SKILL_MD.to_owned())]),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("run settles");
    assert!(outcome.ok, "the mock loop settles green");

    let requests = probe.captured_requests();
    assert_eq!(requests.len(), 1, "one provider turn");
    let system = &requests[0].messages[0];
    assert!(matches!(system.role, Role::System), "system leads");
    let text = match &system.content[0] {
        nika_kernel::provider::ContentBlock::Text { text } => text.clone(),
        other => panic!("system is text: {other:?}"),
    };
    assert!(
        text.starts_with("Base system.\n\n## Skills\n\n### reviewer\n\nReview with care."),
        "the authored system + the normative section: {text}"
    );
    assert!(
        text.contains("Always review twice."),
        "the skill BODY rides along: {text}"
    );
}

#[tokio::test]
async fn unresolved_skill_fails_the_task_with_the_check_code() {
    // An embedder that skipped `with_skills` — the task fails loudly
    // with the same code `nika check` teaches (NIKA-AGENT-003), and
    // NO provider call is ever made (fail BEFORE spend).
    let wf = wf_with_skill();
    let report = nika_check::check(&wf);
    let provider = MockProvider::new("mock").enqueue_text("never reached");
    let probe = provider.clone();
    let runtime = runtime_with(provider, BTreeMap::new());
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("run settles");
    assert!(!outcome.ok, "the task fails");
    let record = outcome.records.get("go").expect("record exists");
    let error = record.error.as_ref().expect("failure carries the error");
    assert_eq!(error.code, "NIKA-AGENT-003");
    assert!(error.message.contains("skills/reviewer/SKILL.md"));
    assert!(!error.transient, "a composition defect never retries");
    assert!(
        probe.captured_requests().is_empty(),
        "no token is spent on a broken composition"
    );
}

#[tokio::test]
async fn invalid_skill_text_fails_the_task_with_the_defect_code() {
    // A text that reaches dispatch but is NOT a valid Agent Skill —
    // the NIKA-AGENT-004 voice (same defect wording as nika check).
    let wf = wf_with_skill();
    let report = nika_check::check(&wf);
    let provider = MockProvider::new("mock").enqueue_text("never reached");
    let runtime = runtime_with(
        provider,
        BTreeMap::from([(
            "skills/reviewer/SKILL.md".to_owned(),
            "# no frontmatter here\n".to_owned(),
        )]),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("run settles");
    assert!(!outcome.ok);
    let error = outcome.records["go"].error.as_ref().expect("the error");
    assert_eq!(error.code, "NIKA-AGENT-004");
    assert!(error.message.contains("frontmatter"), "{}", error.message);
}
