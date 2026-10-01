// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The strict door admits a release on disk only on Unix descriptors (the r1 contract §3.2),
// and these scenarios stand on one; the onboard tests keep the resolver's own choices on
// every platform.
#![cfg(unix)]

//! The session's authoring context through its public surface: the same parser and the same
//! knowledge door as `nika compile`, a configuration that cannot be honored refused explicitly
//! with a typed error, a snapshot pinned when the configuration is resolved and verified again
//! at every seated use, and a deterministic seat that composes and presents none of it.

mod common;

use common::{Foundry, INTENT, SEAT_MODEL};
use nika_cli_host::compile::config::{
    self, AuthoringSettings, ConfigError, KnowledgeChoice, KnowledgeLayer, NO_DEFAULT,
};
use nika_cli_host::compile::knowledge::{KnowledgeError, RefusalCode, Snapshot, TrustedIdentity};
use nika_onboard::compile::{CompileRequest, NativeMode};
use nika_session::authoring::{
    AuthoringContext, AuthoringContextError, AuthoringError, AuthoringRound, AuthoringSeat,
    compile_in,
};

fn pinned(foundry: &Foundry, strategy: &str) -> AuthoringContext {
    AuthoringContext::from_settings(
        &AuthoringSettings::none()
            .with_strategy(strategy)
            .with_knowledge_release(&foundry.snapshot, foundry.identity()),
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
    assert_eq!(pin.digest, None, "a release declares no digest");
    assert_eq!(pin.rows_sha256.len(), 64);
    let line = context.line();
    assert!(
        line.contains("knowledge knowledge-s03 · snapshot ") && line.contains("· admitted"),
        "{line}"
    );
    // One door: the session composes exactly the pack `nika compile --knowledge` composes.
    let session_pack = context.compose(INTENT).unwrap().expect("a pack");
    let cli_pack = Snapshot::open(&foundry.snapshot, Some(&foundry.identity()))
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
        ["pattern:s03-transform-text", "block:s03-transform"],
        "no example and no skill under policy R: {:#}",
        session_pack.selection
    );
    assert_eq!(session_pack.selection["files"]["verified"], 1);
    assert_eq!(session_pack.repairs["NIKA-PARSE-022"].len(), 1);
}

#[test]
fn a_held_out_corpus_named_by_the_environment_guards_the_hosts_release() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    // The environment names a release too, but carries no identity: refused, never pinned.
    let ambient = AuthoringContext::from_settings(
        &AuthoringSettings::none(),
        &AuthoringSettings::none().with_knowledge(&foundry.snapshot, None),
    );
    assert!(
        matches!(
            ambient.refusal(),
            Some(AuthoringContextError::Knowledge(
                KnowledgeError::Unavailable {
                    code: RefusalCode::Untrusted,
                    ..
                }
            ))
        ),
        "{ambient:?}"
    );
    // The only example of the fixture belongs to the corpus `dev`.
    let context = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_release(&foundry.snapshot, foundry.identity()),
        &AuthoringSettings::none().with_knowledge_exclude("dev"),
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
    let untrusted = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge(dir.path(), None),
        &AuthoringSettings::none(),
    );
    assert!(
        matches!(
            untrusted.refusal(),
            Some(AuthoringContextError::Knowledge(
                KnowledgeError::Unavailable {
                    code: RefusalCode::Untrusted,
                    ..
                }
            ))
        ),
        "{untrusted:?}"
    );
    let identity = TrustedIdentity::new(&"0".repeat(64), "policy-r", &"a".repeat(64)).unwrap();
    let nothing = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_release(dir.path(), identity),
        &AuthoringSettings::none(),
    );
    let unavailable = |why: &AuthoringContextError| {
        matches!(
            why,
            AuthoringContextError::Knowledge(KnowledgeError::Unavailable {
                code: RefusalCode::ManifestMissing,
                ..
            })
        )
    };
    assert!(nothing.refusal().is_some_and(unavailable), "{nothing:?}");
    assert!(
        unavailable(&refused_before_any_call(&nothing)),
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
    let mismatch = |why: &AuthoringContextError| match why {
        AuthoringContextError::Knowledge(KnowledgeError::Unavailable { code, detail, .. }) => {
            *code == RefusalCode::PinMismatch && detail.starts_with("blocks/s03-transform.nika")
        }
        _ => false,
    };
    let composed = context
        .compose(INTENT)
        .expect_err("a byte edited after sealing");
    assert!(mismatch(&composed), "{composed:?}");
    assert!(
        mismatch(&refused_before_any_call(&context)),
        "a byte the manifest does not pin is never sent"
    );
    // Opened now, the strict door refuses the release whole, at open: never a pack of the rest.
    let reopened = pinned(&foundry, "only");
    assert!(reopened.refusal().is_some_and(mismatch), "{reopened:?}");
    assert!(reopened.knowledge().is_none());
}

#[test]
fn a_snapshot_replaced_under_the_session_is_refused_as_changed() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "escalate");
    // A new release lands in the same directory: admitted, but not the identity pinned.
    foundry.reseal("knowledge-s03-b", "fixture-producer");
    match context.compose(INTENT) {
        Err(AuthoringContextError::Changed { pinned, found }) => {
            assert!(
                pinned.starts_with("knowledge-s03 (declared digest none"),
                "{pinned}"
            );
            assert!(
                found.contains("the manifest reads sha256"),
                "the strict door's words for bytes the pinned identity no longer names: {found}"
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

/// The review's counterexample, for a release: the same rows under the same version, sealed by
/// another manifest. Admitted alone, it is still not the release this session pinned — its
/// manifest's own bytes differ — and a declared field is never taken for computed integrity.
#[test]
fn a_manifest_resealed_under_the_same_version_breaks_the_session_pin() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let context = pinned(&foundry, "only");
    let pin = context.knowledge().expect("pinned").clone();
    foundry.reseal(common::VERSION, "another-producer");
    let snapshot = Snapshot::open(&foundry.snapshot, Some(&foundry.identity()))
        .expect("admitted on its own identity");
    assert!(
        snapshot.pack(INTENT, None).is_ok(),
        "every byte matches its new pin"
    );
    assert_eq!(snapshot.version(), pin.version.as_deref());
    assert_eq!(snapshot.digest(), None, "a release declares no digest");
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

/// Knowledge off and no default are said, typed, never a silent card alone; off beside a
/// source on one layer is refused; the explicit layer's off wins over the environment's release.
#[test]
fn knowledge_off_and_no_default_are_stated_and_a_contradiction_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    let env_word = AuthoringSettings::none().with_knowledge("off", None);
    let off = AuthoringContext::from_settings(&AuthoringSettings::none(), &env_word);
    assert_eq!(
        off.knowledge_choice(),
        &KnowledgeChoice::Disabled {
            by: KnowledgeLayer::Environment
        }
    );
    assert!(
        off.knowledge().is_none() && off.refusal().is_none(),
        "{off:?}"
    );
    assert!(
        off.line().contains("knowledge off (NIKA_KNOWLEDGE=off)"),
        "{}",
        off.line()
    );
    let explicit = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_knowledge_off(),
        &AuthoringSettings::none().with_knowledge(&foundry.snapshot, None),
    );
    assert_eq!(
        explicit.knowledge_choice(),
        &KnowledgeChoice::Disabled {
            by: KnowledgeLayer::Explicit
        }
    );
    assert!(
        explicit.knowledge().is_none(),
        "the environment's release is never pinned"
    );
    assert!(
        explicit.line().contains("knowledge off (explicit)"),
        "{}",
        explicit.line()
    );
    let none = AuthoringContext::default();
    assert_eq!(none.knowledge_choice(), &KnowledgeChoice::NoDefault);
    assert!(none.line().contains(NO_DEFAULT), "{}", none.line());
    let contradiction = AuthoringContext::from_settings(
        &AuthoringSettings::none(),
        &env_word.with_knowledge_pack(dir.path().join("pack.json")),
    );
    assert_eq!(
        contradiction.refusal(),
        Some(&AuthoringContextError::Config(
            ConfigError::ContradictoryKnowledge {
                layer: KnowledgeLayer::Environment
            }
        ))
    );
    assert!(matches!(
        refused_before_any_call(&contradiction),
        AuthoringContextError::Config(_)
    ));
}

/// One strict admission behind every door: a release refused by the compile door is refused by
/// the session with the same typed cause, and an admitted one is composed alike.
#[test]
fn the_session_and_the_compile_door_admit_and_refuse_alike() {
    let dir = tempfile::tempdir().unwrap();
    let foundry = Foundry::create(dir.path());
    std::fs::write(foundry.snapshot.join("knowledge/extra.jsonl"), "").unwrap();
    let named =
        AuthoringSettings::none().with_knowledge_release(&foundry.snapshot, foundry.identity());
    let door = config::resolve(&named, &AuthoringSettings::none()).expect("resolves");
    let request = nika_onboard::compile::CompileRequest::create(INTENT);
    let compile_cause = match door.with_knowledge(request, INTENT) {
        Err(KnowledgeError::Unavailable { code, .. }) => code,
        other => panic!("refused by the compile door: {other:?}"),
    };
    let session = AuthoringContext::from_settings(&named, &AuthoringSettings::none());
    match session.refusal() {
        Some(AuthoringContextError::Knowledge(KnowledgeError::Unavailable { code, .. })) => {
            assert_eq!(
                (*code, compile_cause),
                (RefusalCode::ExtraFile, RefusalCode::ExtraFile)
            );
        }
        other => panic!("refused by the session: {other:?}"),
    }
}
