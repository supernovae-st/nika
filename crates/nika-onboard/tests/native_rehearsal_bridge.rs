// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A scripted author reaches the real observed room through the compile entry, on both routes
//! that still author a candidate: the source-anchored revision a hand-written base keeps (the
//! author states typed links, never the source; source-only CREATE is retired) and the semantic
//! sketch door a creation takes. The script is a provider double; the room and its Runtime are
//! real. All files are synthetic and the existing room receipt helper records each host call
//! before any assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    Role, StopReason, TokenUsage,
};
use nika_onboard::compile::rehearse::{RehearsalFuture, Rehearse};
use nika_onboard::compile::room::ObservedRoom;
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, CompileStatus, NativeMode,
    Strategy, compile_with_cognition_rehearsed,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const NATIVE: &str = concat!(
    module_path!(),
    "::a_native_candidate_rehearses_without_mutating_the_project"
);
const SEMANTIC: &str = concat!(
    module_path!(),
    "::a_semantic_candidate_rehearses_without_mutating_the_project"
);

/// The author's scripted answers, in order, to every call that is not a judge's closed choice;
/// a judge is approved, and a revision's links are read from the facts its opening states.
struct Author {
    answers: Vec<String>,
    calls: AtomicUsize,
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => Some(schema),
            _ => None,
        };
        let keys = schema.and_then(|schema| schema["properties"]["choice"]["enum"].as_array());
        let approved = ["faithful", "carried"]
            .into_iter()
            .find(|key| keys.is_some_and(|keys| keys.iter().any(|value| value == *key)));
        let answer = if let Some(choice) = approved {
            json!({"choice": choice}).to_string()
        } else {
            let at = self.calls.fetch_add(1, Ordering::SeqCst);
            if schema.is_some_and(|schema| schema["properties"]["supersedes"].is_object()) {
                links(&request)
            } else {
                self.answers.get(at).cloned().unwrap_or_default()
            }
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text: answer }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// A source revision's typed links, read from its opening: every change clause added, the new
/// destination written like the base's first one. The author never writes the source.
fn links(request: &InferRequest) -> String {
    let opening = request
        .messages
        .iter()
        .find(|message| matches!(message.role, Role::User))
        .and_then(|message| {
            message.content.iter().find_map(|block| match block {
                ContentBlock::Text { text } => serde_json::from_str::<serde_json::Value>(text).ok(),
                _ => None,
            })
        })
        .expect("HARNESS_INVALID: the revision opening is JSON");
    json!({"supersedes": [], "adds": opening["change_clauses"],
        "like": opening["base_destinations"][0], "notes": "copy the source again"})
    .to_string()
}

struct RecordedRoom {
    room: ObservedRoom,
    test: &'static str,
    calls: AtomicUsize,
}

impl Rehearse for RecordedRoom {
    fn bound(&self) -> Duration {
        self.room.bound()
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            let at = self.calls.fetch_add(1, Ordering::SeqCst);
            room_support::rehearsed(
                self.test,
                &format!("author-{at}"),
                &self.room,
                candidate,
                inputs,
                targets,
            )
            .await
        })
    }
}

/// A project holding the copy's source, a stale target and a witness, and the copy's own
/// lowered program, made once by the real compiler.
struct Project {
    world: room_support::World,
    prefix: String,
    source: String,
}

fn project() -> Project {
    let prefix = room_support::unique_prefix();
    let world = room_support::World::with_prefix(&prefix, &[("in/source.txt", room_support::BETA)]);
    world.put("out/copied.txt", "stale target");
    world.put("witness.txt", "unchanged witness");
    let source = room_support::lowered_copy(&prefix)
        .expect("HARNESS_INVALID: fixture program")
        .text;
    let _ = room_support::admitted_digest_of(&source);
    Project {
        world,
        prefix,
        source,
    }
}

fn policy(native: NativeMode) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(0)
}

/// `request` compiled with `author` and the project's real room, the project unchanged by it
/// (bytes and source permissions), the rehearsal of the exact final candidate passed in a room
/// that was cleaned with no effect, and the author and room each reached once.
async fn rehearsed(
    project: &Project,
    test: &'static str,
    request: &CompileRequest,
    author: &Author,
) -> CompileOutcome {
    let world = &project.world;
    let before = world.files();
    let permissions = std::fs::metadata(world.project_path("in/source.txt"))
        .unwrap()
        .permissions();
    let room = RecordedRoom {
        room: world.room(),
        test,
        calls: AtomicUsize::new(0),
    };
    let cognition = Cognition {
        provider: Some(author),
        seat: None,
    };
    let out = compile_with_cognition_rehearsed(request, cognition, Some(&room))
        .await
        .unwrap();
    assert_eq!(
        world.files(),
        before,
        "the source, old target, witness and scratch parent are unchanged"
    );
    let now = std::fs::metadata(world.project_path("in/source.txt"))
        .unwrap()
        .permissions();
    assert_eq!(now.readonly(), permissions.readonly());
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&now),
        std::os::unix::fs::PermissionsExt::mode(&permissions),
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(room.calls.load(Ordering::SeqCst), 1);
    let record = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["reports"][0];
    assert_eq!(record["outcome"]["kind"], "passed");
    assert_eq!(record["read_back"][0]["text"], room_support::BETA);
    assert_eq!(record["room"]["cleaned"], true);
    assert_eq!(record["ledger"]["drained"], true);
    for kind in ["network", "provider", "spawn", "prompt", "secret", "child"] {
        assert_eq!(record["effects"][kind], 0, "{kind}: {record}");
    }
    out
}

/// The revision a hand-written base keeps: the change adds one destination beside the copy's;
/// the author states the typed links, the compiler writes the revised source, and that exact
/// candidate is rehearsed as it is proposed. (Any other change in words keeps such a base unrun.)
#[tokio::test]
async fn a_native_candidate_rehearses_without_mutating_the_project() {
    let project = project();
    let world = &project.world;
    let author = Author {
        answers: Vec::new(),
        calls: AtomicUsize::new(0),
    };
    let second = world.path("out/second.txt");
    let change = format!("also keep a copy of the result in {second}");
    let request = CompileRequest::edit(project.source.clone(), change)
        .with_original_intent(room_support::copy_intent(&project.prefix))
        .with_authoring_policy(policy(NativeMode::Only));
    let out = rehearsed(&project, NATIVE, &request, &author).await;
    assert_eq!(
        out.provenance.strategy,
        Some(Strategy::Native),
        "HARNESS_INVALID: the author was not reached"
    );
    let candidate = out.candidate.as_deref().expect("the revised source");
    assert_ne!(candidate, project.source, "the revision changed the base");
    assert!(
        candidate.contains(&second) && candidate.contains(&world.path("out/copied.txt")),
        "{candidate}"
    );
    assert_eq!(
        author.calls.load(Ordering::SeqCst),
        1,
        "the links alone; the judge is approved apart"
    );
}

/// A creation takes the semantic sketch door: the author states the copy's structure, the
/// compiler writes its source, and that exact candidate is rehearsed.
#[tokio::test]
async fn a_semantic_candidate_rehearses_without_mutating_the_project() {
    let project = project();
    let world = &project.world;
    let graph = json!({"name": "copy", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read",
         "reads": [world.path("in/source.txt")], "purpose": "the source"},
        {"id": "write_copy", "verb": "invoke", "tool": "nika:write",
         "writes": [world.path("out/copied.txt")],
         "with": [{"name": "text", "from": "read_source"}], "purpose": "the copy"}],
        "questions": [], "gaps": [], "notes": "graph"});
    let author = Author {
        answers: vec![
            graph.to_string(),
            json!({"fills": [], "notes": "fills"}).to_string(),
        ],
        calls: AtomicUsize::new(0),
    };
    let request = CompileRequest::create(room_support::copy_intent(&project.prefix))
        .with_authoring_policy(policy(NativeMode::Sketch));
    let out = rehearsed(&project, SEMANTIC, &request, &author).await;
    let record = out.provenance.plan.as_ref().expect("the semantic record");
    assert_eq!(record["semantic_record"], 1, "{record:#}");
    assert_eq!(
        author.calls.load(Ordering::SeqCst),
        2,
        "the sketch and its fills"
    );
}
