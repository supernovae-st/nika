// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The session's authoring context through its public surface: the same parser and the same
//! knowledge door as `nika compile`, a configuration that cannot be honored refused explicitly
//! with a typed error, a snapshot pinned when the configuration is resolved and verified again
//! at every seated use, and a deterministic seat that composes and presents none of it.

mod common;

use common::{Foundry, INTENT, SEAT_MODEL};
use nika_cli_host::compile::config::{AuthoringSettings, ConfigError};
use nika_cli_host::compile::knowledge::{KnowledgeError, Snapshot};
use nika_onboard::compile::{CompileRequest, NativeMode};
use nika_session::authoring::{
    AuthoringContext, AuthoringContextError, AuthoringError, AuthoringRound, AuthoringSeat,
    compile_in,
};

fn pinned(foundry: &Foundry, strategy: &str) -> AuthoringContext {
    AuthoringContext::from_settings(
        &AuthoringSettings::none()
            .with_strategy(strategy)
            .with_knowledge(&foundry.snapshot, None),
        &AuthoringSettings::none(),
    )
}

fn seat() -> AuthoringSeat {
    AuthoringSeat::Provider {
        model: SEAT_MODEL.to_owned(),
    }
}

/// A seated compile under this context is refused with exactly this typed reason, before any
/// call (no seat is reachable here: the model's server is never contacted).
fn refused_before_any_call(context: &AuthoringContext) -> AuthoringContextError {
    match compile_in(&seat(), context, &CompileRequest::create(INTENT), INTENT) {
        Err(AuthoringError::Context(reason)) => reason,
        other => panic!("refused before any call: {other:?}"),
    }
}

#[test]
fn nothing_named_is_the_cli_default_and_a_host_names_its_own() {
    let default = AuthoringContext::default();
    assert_eq!(
        default.strategy(),
        NativeMode::Escalate,
        "the CLI's default"
    );
    assert!(default.knowledge().is_none() && default.refusal().is_none());
    let none =
        AuthoringContext::from_settings(&AuthoringSettings::none(), &AuthoringSettings::none());
    assert_eq!(none, default);
    assert_eq!(none.source(), "default");
    let sketch = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_strategy("sketch"),
        &AuthoringSettings::none().with_strategy("only"),
    );
    assert_eq!(
        sketch.strategy(),
        NativeMode::Sketch,
        "explicit over ambient"
    );
    assert_eq!(sketch.source(), "host");
    let ambient = AuthoringContext::from_settings(
        &AuthoringSettings::none(),
        &AuthoringSettings::none().with_strategy("only"),
    );
    assert_eq!(ambient.strategy(), NativeMode::Only);
    assert_eq!(ambient.source(), "environment");
    assert!(
        ambient.line().contains("strategy only (environment)"),
        "{}",
        ambient.line()
    );
}

#[test]
fn a_snapshot_is_pinned_at_open_and_its_pack_is_the_cli_doors_pack() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "only");
    assert!(context.refusal().is_none(), "{:?}", context.refusal());
    let pin = context.knowledge().expect("pinned");
    assert_eq!(pin.version.as_deref(), Some(common::VERSION));
    assert_eq!(pin.digest.as_deref(), Some("digest-s03-a"));
    assert_eq!(pin.rows_sha256.len(), 64);
    let line = context.line();
    assert!(
        line.contains("knowledge knowledge-s03") && line.contains("digest digest-s03-a"),
        "{line}"
    );
    // One door: the session composes exactly the pack `nika compile --knowledge` composes.
    let session_pack = context.compose(INTENT).unwrap().expect("a pack");
    let cli_pack = Snapshot::open(&foundry.snapshot)
        .unwrap()
        .pack(INTENT, None)
        .unwrap();
    assert_eq!(session_pack, cli_pack);
    let ids: Vec<&str> = session_pack
        .references
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "pattern:s03-transform-text",
            "block:s03-transform",
            "example:s03-rewrite",
            "skill:s03-rewrite"
        ],
        "{:#}",
        session_pack.selection
    );
    assert_eq!(session_pack.selection["files"]["verified"], 3);
    assert_eq!(session_pack.repairs["NIKA-PARSE-022"].len(), 1);
}

#[test]
fn a_held_out_corpus_named_by_the_host_guards_the_environments_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    // The only example of the fixture belongs to the corpus `dev`.
    let context = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_exclude("dev"),
        &AuthoringSettings::none().with_knowledge(&foundry.snapshot, None),
    );
    assert_eq!(
        context
            .knowledge()
            .and_then(|p| p.exclude_corpus.as_deref()),
        Some("dev")
    );
    let pack = context.compose(INTENT).unwrap().expect("a pack");
    assert!(
        pack.references.iter().all(|r| r.kind != "example"),
        "the held-out corpus is never recalled: {:#}",
        pack.selection
    );
    // Named with no snapshot anywhere, the exclusion is refused, never dropped.
    let unguarded = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_exclude("dev"),
        &AuthoringSettings::none(),
    );
    assert_eq!(
        unguarded.refusal(),
        Some(&AuthoringContextError::Config(
            ConfigError::ExclusionWithoutSnapshot {
                corpus: "dev".to_owned()
            }
        ))
    );
}

#[test]
fn knowledge_named_under_off_is_refused_and_a_seated_compile_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "off");
    let why = context.refusal().expect("refused").clone();
    assert!(
        matches!(
            why,
            AuthoringContextError::Config(ConfigError::KnowledgeUnread { .. })
        ),
        "{why:?}"
    );
    assert!(why.to_string().contains("never reads it"), "{why}");
    assert!(context.line().contains("refused"), "{}", context.line());
    assert_eq!(refused_before_any_call(&context), why);
    // An unknown strategy word is refused the same way.
    let unknown = AuthoringContext::from_settings(
        &AuthoringSettings::none(),
        &AuthoringSettings::none().with_strategy("native"),
    );
    assert_eq!(
        unknown.refusal(),
        Some(&AuthoringContextError::Config(
            ConfigError::UnknownStrategy("native".to_owned())
        ))
    );
}

#[test]
fn a_pack_for_one_request_and_a_directory_that_is_no_snapshot_are_refused_at_open() {
    let dir = tempfile::tempdir().unwrap();
    let pack = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_pack(dir.path().join("pack.json")),
        &AuthoringSettings::none(),
    );
    assert!(
        matches!(
            pack.refusal(),
            Some(AuthoringContextError::PackForOneRequest { .. })
        ),
        "{pack:?}"
    );
    let nothing = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge(dir.path(), None),
        &AuthoringSettings::none(),
    );
    assert!(
        matches!(
            nothing.refusal(),
            Some(AuthoringContextError::Knowledge(
                KnowledgeError::NotASnapshot { .. }
            ))
        ),
        "{nothing:?}"
    );
    assert!(
        matches!(
            refused_before_any_call(&nothing),
            AuthoringContextError::Knowledge(KnowledgeError::NotASnapshot { .. })
        ),
        "never authored without the knowledge the session was told to read"
    );
}

#[test]
fn a_stale_snapshot_is_refused_before_a_byte_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "only");
    assert!(context.compose(INTENT).unwrap().is_some());
    foundry.edit_block_after_export();
    match context.compose(INTENT) {
        Err(AuthoringContextError::Knowledge(KnowledgeError::Stale { file, version, .. })) => {
            assert_eq!(file, "blocks/s03-transform.nika");
            assert_eq!(version, common::VERSION);
        }
        other => panic!("stale: {other:?}"),
    }
    assert!(
        matches!(
            refused_before_any_call(&context),
            AuthoringContextError::Knowledge(KnowledgeError::Stale { .. })
        ),
        "a stale byte is never sent"
    );
    // Opened now, the same snapshot pins (its rows still match): the stale file is found when a
    // pack reads it.
    let reopened = pinned(&foundry, "only");
    assert!(reopened.refusal().is_none());
    assert!(reopened.compose(INTENT).is_err());
}

#[test]
fn a_snapshot_replaced_under_the_session_is_refused_as_changed() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "escalate");
    // A new export lands in the same directory: consistent, but not the identity pinned.
    foundry.pin("knowledge-s03-b", "digest-s03-b");
    match context.compose(INTENT) {
        Err(AuthoringContextError::Changed { pinned, found }) => {
            assert!(
                pinned.starts_with("knowledge-s03 (declared digest digest-s03-a"),
                "{pinned}"
            );
            assert!(
                found.starts_with("knowledge-s03-b (declared digest digest-s03-b"),
                "{found}"
            );
        }
        other => panic!("changed: {other:?}"),
    }
    // A session opened now pins the new export.
    let fresh = pinned(&foundry, "escalate");
    assert_eq!(
        fresh.knowledge().and_then(|p| p.version.as_deref()),
        Some("knowledge-s03-b")
    );
    assert!(fresh.compose(INTENT).unwrap().is_some());
}

/// The review's counterexample: a presented file and its manifest pin changed TOGETHER, under
/// the same declared version and digest and the same row files. Opened alone the snapshot is
/// consistent; it is still not the snapshot this session pinned — its manifest's own bytes
/// differ — and the declared digest is never taken for computed integrity.
#[test]
fn a_file_repinned_under_the_same_declared_identity_breaks_the_session_pin() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "only");
    let pin = context.knowledge().expect("pinned").clone();
    foundry.edit_block_after_export();
    foundry.pin(common::VERSION, "digest-s03-a");
    let snapshot = Snapshot::open(&foundry.snapshot).expect("consistent on its own");
    assert!(
        snapshot.pack(INTENT, None).is_ok(),
        "every byte matches its new pin"
    );
    assert_eq!(snapshot.version(), pin.version.as_deref());
    assert_eq!(
        snapshot.digest(),
        pin.digest.as_deref(),
        "the same declared digest"
    );
    assert_eq!(snapshot.rows_sha256(), pin.rows_sha256, "the same rows");
    assert_ne!(snapshot.manifest_sha256(), pin.manifest_sha256);
    match context.compose(INTENT) {
        Err(AuthoringContextError::Changed { pinned, found }) => {
            assert_ne!(pinned, found);
            assert!(
                pinned.contains(&format!("manifest {}", &pin.manifest_sha256[..12])),
                "{pinned}"
            );
        }
        other => panic!("the pin binds the manifest's bytes: {other:?}"),
    }
    assert!(
        matches!(
            refused_before_any_call(&context),
            AuthoringContextError::Changed { .. }
        ),
        "never presented under the pinned identity"
    );
    assert!(
        pin.record()["digest_is"]
            .as_str()
            .is_some_and(|w| w.contains("not recomputed")),
        "{}",
        pin.record()
    );
}

#[test]
fn a_deterministic_seat_composes_and_presents_nothing_and_calls_no_one() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    // Resolving the configuration opens the snapshot to pin it, whatever the seat; a compile
    // under a deterministic seat never opens it again, composes no pack and calls no one.
    let context = pinned(&foundry, "only");
    let refused = pinned(&foundry, "off");
    std::fs::remove_dir_all(&foundry.snapshot).unwrap();
    let deterministic = AuthoringSeat::Deterministic { why: None };
    for context in [&context, &refused] {
        let request = CompileRequest::create("Read ./notes/brief.md and write it to ./out/copy.md");
        let out = compile_in(&deterministic, context, &request, "unused").expect("compiles");
        assert_eq!(
            out.status,
            nika_onboard::compile::CompileStatus::Ready,
            "{:?}",
            out.diagnostics
        );
        assert!(out.provenance.authoring.is_none(), "no call");
        assert!(
            out.provenance
                .decision
                .as_ref()
                .is_none_or(|d| d.get("session").is_none()),
            "nothing attached, nothing stamped: {:?}",
            out.provenance.decision
        );
    }
    // A round compiles the same way through the deterministic seat.
    let round = AuthoringRound::new(INTENT);
    let out = round.compile(&deterministic, &context).expect("compiles");
    assert!(out.provenance.authoring.is_none());
}
