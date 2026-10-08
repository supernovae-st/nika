// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge choice every door resolves alike: the first layer that says anything decides,
//! off and a source on one layer contradict, nothing said is the release this build embeds
//! (nothing read under the strategy `off`), and the door enters an admitted release and never
//! another, refuses a pack, attaches nothing when off.

#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::path::{Path, PathBuf};

use super::*;
use crate::compile::CompileRequest;
use crate::knowledge::fixture::{self, Payload};
use crate::knowledge::{ComponentCatalog, KnowledgeError, RefusalCode, Snapshot, bundled};

use super::KnowledgeLayer::{Environment, Explicit};

fn none() -> AuthoringSettings {
    AuthoringSettings::none()
}

fn snapshot(dir: &str, by: KnowledgeLayer) -> KnowledgeChoice {
    KnowledgeChoice::Named {
        source: KnowledgeSource::Snapshot {
            dir: PathBuf::from(dir),
            exclude_corpus: None,
            identity: None,
        },
        by,
    }
}

fn pack(file: &str, by: KnowledgeLayer) -> KnowledgeChoice {
    KnowledgeChoice::Named {
        source: KnowledgeSource::Pack {
            file: PathBuf::from(file),
        },
        by,
    }
}

fn choice(
    explicit: &AuthoringSettings,
    env: &AuthoringSettings,
) -> Result<KnowledgeChoice, ConfigError> {
    resolve(explicit, env).map(|config| config.choice)
}

/// The environment's own word: `NIKA_KNOWLEDGE=off` reaches the parser as that raw word.
#[test]
fn knowledge_off_in_the_environment_is_disabled_by_the_environment() {
    let env = none().with_knowledge(KNOWLEDGE_OFF, None);
    let config = resolve(&none(), &env).expect("resolves");
    assert_eq!(config.choice, KnowledgeChoice::Disabled { by: Environment });
    assert_eq!(config.knowledge, None);
    assert_eq!(config.choice.words(), "knowledge off (NIKA_KNOWLEDGE=off)");
}

/// Only the exact environment word is the off word; an explicit path named `off` is a directory,
/// and so is any other spelling in the environment.
#[test]
fn only_the_exact_environment_word_turns_the_knowledge_off() {
    assert_eq!(
        choice(&none().with_knowledge("off", None), &none()),
        Ok(snapshot("off", Explicit))
    );
    for spelled in ["./off", "OFF", "Off", " off", "off/"] {
        assert_eq!(
            choice(&none(), &none().with_knowledge(spelled, None)),
            Ok(snapshot(spelled, Environment)),
            "{spelled:?} names a directory"
        );
    }
}

/// The whole precedence table, each expectation written out: the explicit layer decides when it
/// says anything (a source or off), else the environment, else the embedded default.
#[test]
fn the_first_layer_that_says_anything_decides() {
    let explicit_layers = [
        ("absent", none()),
        ("off", none().with_knowledge_off()),
        ("snapshot", none().with_knowledge("/explicit/snap", None)),
        ("pack", none().with_knowledge_pack("/explicit/pack.json")),
    ];
    let env_layers = [
        ("absent", none()),
        ("flag off", none().with_knowledge_off()),
        ("word off", none().with_knowledge("off", None)),
        ("snapshot", none().with_knowledge("/env/snap", None)),
        ("pack", none().with_knowledge_pack("/env/pack.json")),
    ];
    let expected = |explicit: &str, env: &str| match (explicit, env) {
        ("absent", "absent") => KnowledgeChoice::Default,
        ("absent", "flag off" | "word off") => KnowledgeChoice::Disabled { by: Environment },
        ("absent", "snapshot") => snapshot("/env/snap", Environment),
        ("absent", "pack") => pack("/env/pack.json", Environment),
        ("off", _) => KnowledgeChoice::Disabled { by: Explicit },
        ("snapshot", _) => snapshot("/explicit/snap", Explicit),
        ("pack", _) => pack("/explicit/pack.json", Explicit),
        other => panic!("no expectation for {other:?}"),
    };
    for (explicit_name, explicit) in &explicit_layers {
        for (env_name, env) in &env_layers {
            assert_eq!(
                choice(explicit, env),
                Ok(expected(explicit_name, env_name)),
                "explicit {explicit_name} · environment {env_name}"
            );
        }
    }
}

#[test]
fn off_and_a_source_on_one_layer_contradict_and_only_the_deciding_layer_is_read() {
    let contradictions = [
        (
            none().with_knowledge_off().with_knowledge("/s", None),
            none(),
            Explicit,
        ),
        (
            none().with_knowledge_off().with_knowledge_pack("/p.json"),
            none(),
            Explicit,
        ),
        (
            none(),
            none().with_knowledge_off().with_knowledge("/s", None),
            Environment,
        ),
        (
            none(),
            none()
                .with_knowledge("off", None)
                .with_knowledge_pack("/p.json"),
            Environment,
        ),
    ];
    for (explicit, env, layer) in contradictions {
        let refused = resolve(&explicit, &env).expect_err("contradictory");
        assert_eq!(refused, ConfigError::ContradictoryKnowledge { layer });
        assert!(refused.to_string().contains("--no-knowledge"), "{refused}");
    }
    // Both kinds of off on one layer say one thing.
    assert_eq!(
        choice(
            &none(),
            &none().with_knowledge_off().with_knowledge("off", None)
        ),
        Ok(KnowledgeChoice::Disabled { by: Environment })
    );
    // The environment's own contradiction is not read when the explicit layer decides.
    let muddled = none()
        .with_knowledge("off", None)
        .with_knowledge_pack("/env/pack.json");
    assert_eq!(
        choice(&none().with_knowledge("/explicit/snap", None), &muddled),
        Ok(snapshot("/explicit/snap", Explicit))
    );
    assert_eq!(
        choice(&none().with_knowledge_off(), &muddled),
        Ok(KnowledgeChoice::Disabled { by: Explicit })
    );
}

#[test]
fn nothing_named_is_the_embedded_release_and_under_off_nothing_is_read() {
    let config = resolve(&none(), &none()).expect("resolves");
    assert_eq!(config.choice, KnowledgeChoice::Default);
    assert_eq!(
        config.knowledge,
        Some(KnowledgeSource::Embedded {
            exclude_corpus: None
        })
    );
    assert_eq!(config.choice.words(), "knowledge embedded (default)");
    // Under the strategy `off` nothing named reads nothing: no source, no refusal.
    let off = resolve(&none().with_strategy("off"), &none()).expect("resolves");
    assert_eq!((off.choice, off.knowledge), (KnowledgeChoice::Unread, None));
    assert_eq!(
        KnowledgeChoice::Unread.words(),
        "knowledge unread (strategy off)"
    );
}

#[test]
fn an_explicit_exclusion_guards_the_release_read_and_is_refused_where_none_is() {
    let explicit = none().with_knowledge_exclude("heldout");
    let refused = Err(ConfigError::ExclusionWithoutSnapshot {
        corpus: "heldout".to_owned(),
    });
    for env in [
        none().with_knowledge("off", None),
        none().with_knowledge_off(),
    ] {
        assert_eq!(
            resolve(&explicit, &env).map(|c| c.choice),
            refused,
            "{env:?}"
        );
    }
    assert_eq!(
        resolve(&explicit.clone().with_knowledge_off(), &none()).map(|c| c.choice),
        refused
    );
    // Nothing read under the strategy `off`: nothing to exclude from, refused too.
    assert_eq!(
        resolve(&explicit.clone().with_strategy("off"), &none()).map(|c| c.choice),
        refused
    );
    // Nothing named: it guards the release this build embeds, never dropped.
    assert_eq!(
        resolve(&explicit, &none()).map(|c| c.knowledge),
        Ok(Some(KnowledgeSource::Embedded {
            exclude_corpus: Some("heldout".to_owned()),
        }))
    );
    // The environment's own exclusion with the knowledge off has nothing to exclude.
    let ambient = none()
        .with_knowledge("off", None)
        .with_knowledge_exclude("heldout");
    assert_eq!(
        choice(&none(), &ambient),
        Ok(KnowledgeChoice::Disabled { by: Environment })
    );
    // An explicit exclusion still guards the snapshot the environment names.
    assert_eq!(
        resolve(&explicit, &none().with_knowledge("/env/snap", None)).map(|c| c.knowledge),
        Ok(Some(KnowledgeSource::Snapshot {
            dir: PathBuf::from("/env/snap"),
            exclude_corpus: Some("heldout".to_owned()),
            identity: None,
        }))
    );
}

#[test]
fn the_strategy_off_and_the_knowledge_off_are_distinct() {
    let strategy_off = none().with_strategy("off");
    for env in [none(), none().with_knowledge("off", None)] {
        let config = resolve(&strategy_off.clone().with_knowledge_off(), &env).expect("resolves");
        assert_eq!((config.strategy, config.knowledge), (NativeMode::Off, None));
    }
    let config = resolve(&strategy_off, &none().with_knowledge("off", None)).expect("resolves");
    assert_eq!(config.choice, KnowledgeChoice::Disabled { by: Environment });
    // Disabling authoring never erases a named source: it stays refused as unread.
    assert!(matches!(
        resolve(&strategy_off, &none().with_knowledge("/env/snap", None)),
        Err(ConfigError::KnowledgeUnread { .. })
    ));
}

#[test]
fn a_doors_flags_are_the_builders_words() {
    let flags = AuthoringSettings::from_flags(
        Some("only"),
        Some(Path::new("/flag/snap")),
        Some(Path::new("/flag/pack.json")),
        Some("heldout"),
        Some("max"),
        true,
    );
    let built = none()
        .with_strategy("only")
        .with_knowledge("/flag/snap", None)
        .with_knowledge_pack("/flag/pack.json")
        .with_knowledge_exclude("heldout")
        .with_reasoning("max")
        .with_knowledge_off();
    assert_eq!(flags, built);
    assert_eq!(
        AuthoringSettings::from_flags(None, None, None, None, None, false),
        none()
    );
}

// ── the door every caller crosses ──────────────────────────────────────────────────────────────

const INTENT: &str = fixture::INTENT;

fn attached(config: &AuthoringConfig) -> Result<CompileRequest, KnowledgeError> {
    config.with_knowledge(CompileRequest::create(INTENT), INTENT)
}

#[test]
fn knowledge_off_and_unread_attach_nothing() {
    for (explicit, env) in [
        (none(), none().with_knowledge("off", None)),
        (none().with_knowledge_off(), none()),
        (none().with_strategy("off"), none()),
    ] {
        let config = resolve(&explicit, &env).expect("resolves");
        let request = attached(&config).expect("nothing to refuse");
        assert!(request.authoring_knowledge.is_none(), "{:?}", config.choice);
    }
}

fn request_with_embedded_knowledge() -> CompileRequest {
    let intent = "Declare a typed output with a description";
    let config = resolve(&none(), &none()).expect("resolves");
    let mut request = config
        .with_knowledge(CompileRequest::create(intent), intent)
        .expect("embedded release admitted");
    let pack = request.authoring_knowledge.as_ref().expect("attached");
    let ids: Vec<_> = pack.references.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"construct:outputs"), "{ids:?}");
    assert!(ids.contains(&"block:typed-inputs-outputs"), "{ids:?}");
    request.answers.insert("column".into(), "total".into());
    request.workflow_id = Some("typed-report".into());
    request
        .with_knowledge(serde_json::json!({"observed": [{"columns": ["total"]}]}))
        .with_plan(serde_json::json!({"steps": []}))
        .with_original_intent(intent)
        .with_authoring_policy(AuthoringPolicy::new(
            "test-model",
            512,
            Duration::from_secs(7),
        ))
        .with_hot_policy(crate::compile::HotPolicy::Off)
        .with_admitted_money(std::iter::once(0..3).collect())
        .with_stated_money()
}

fn assert_only_authoring_knowledge_removed(before: &CompileRequest, after: &CompileRequest) {
    assert!(
        after.authoring_knowledge.is_none(),
        "a reused request must not retain its previous authoring pack"
    );
    match (&before.input, &after.input) {
        (nika_compile::surface::Input::Create(a), nika_compile::surface::Input::Create(b)) => {
            assert_eq!(a, b);
        }
        other => panic!("creation preserved: {other:?}"),
    }
    assert_eq!(before.answers, after.answers);
    assert_eq!(before.workflow_id, after.workflow_id);
    assert_eq!(before.hot, after.hot);
    assert_eq!(before.plan, after.plan);
    assert_eq!(before.knowledge, after.knowledge);
    assert_eq!(before.original_intent, after.original_intent);
    assert_eq!(before.money, after.money);
    assert_eq!(before.stated_money, after.stated_money);
    let a = before.authoring.as_ref().expect("policy before");
    let b = after.authoring.as_ref().expect("policy after");
    assert_eq!(
        (
            &a.model,
            a.max_tokens,
            a.initial_max_tokens,
            a.timeout,
            a.samples,
            a.native,
            a.repairs,
            a.reasoning
        ),
        (
            &b.model,
            b.max_tokens,
            b.initial_max_tokens,
            b.timeout,
            b.samples,
            b.native,
            b.repairs,
            b.reasoning
        )
    );
}

fn assert_reused_request_disabled(explicit: &AuthoringSettings, env: &AuthoringSettings) {
    let before = request_with_embedded_knowledge();
    let config = resolve(explicit, env).expect("resolves");
    let after = config.with_knowledge(before.clone(), INTENT).expect("off");
    assert_only_authoring_knowledge_removed(&before, &after);
}

#[test]
fn knowledge_off_clears_a_reused_request() {
    assert_reused_request_disabled(&none().with_knowledge_off(), &none());
}

#[test]
fn environment_off_clears_a_reused_request() {
    assert_reused_request_disabled(&none(), &none().with_knowledge("off", None));
}

#[test]
fn strategy_off_clears_a_reused_request() {
    assert_reused_request_disabled(&none().with_strategy("off"), &none());
}

#[test]
fn empty_intent_clears_a_reused_request() {
    let before = request_with_embedded_knowledge().with_replaced_input("  ");
    let config = resolve(&none(), &none()).expect("resolves");
    let after = config.with_knowledge(before.clone(), "  ").expect("empty");
    assert_only_authoring_knowledge_removed(&before, &after);
}

#[test]
fn enabled_knowledge_replaces_the_previous_intents_pack() {
    let intent = "Grant zero authority to a pure compute workflow";
    let before = request_with_embedded_knowledge().with_replaced_input(intent);
    let config = resolve(&none(), &none()).expect("resolves");
    let after = config
        .with_knowledge(before, intent)
        .expect("embedded release admitted");
    let pack = after.authoring_knowledge.expect("new pack attached");
    let ids: Vec<_> = pack.references.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"construct:permits"), "{ids:?}");
    assert!(ids.contains(&"block:run-deterministic"), "{ids:?}");
}

/// Nothing named attaches the release this build embeds, composed by the door for the intent as
/// the memory door composes it. A matching intent carries its pattern and realizing block.
#[test]
fn nothing_named_attaches_the_embedded_release_composed_for_the_intent() {
    let config = resolve(&none().with_knowledge_exclude("heldout"), &none()).expect("resolves");
    let intent = "Declare a typed output with a description";
    let request = config
        .with_knowledge(CompileRequest::create(intent), intent)
        .expect("admitted");
    let pack = request.authoring_knowledge.expect("attached");
    assert_eq!(
        pack.identity["snapshot_sha256"],
        "6476372aa7eedf02e3b718dcd1c51769d97450eb0ae825a62b33fcf10a2471af"
    );
    assert_eq!(pack.identity["verification"]["policy"]["id"], "policy-r2");
    let direct = bundled::admit(Some(&bundled::identity().unwrap()))
        .unwrap()
        .pack(intent, Some("heldout"))
        .unwrap();
    assert_eq!(pack, direct, "the door's pack is the memory door's");
    let ids: Vec<_> = pack.references.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"construct:outputs"), "{ids:?}");
    assert!(ids.contains(&"block:typed-inputs-outputs"), "{ids:?}");
    // The release's repair principles ride along by diagnostic code, never an empty entry.
    let odd: Vec<_> = (pack.repairs.iter())
        .filter(|(code, principles)| {
            code.trim().is_empty()
                || principles.is_empty()
                || principles.iter().any(|p| p.trim().is_empty())
        })
        .collect();
    assert!(odd.is_empty(), "{odd:?}");
    // An intent with no words composes nothing, as for any release.
    let request = config
        .with_knowledge(CompileRequest::create("  "), "  ")
        .expect("nothing to refuse");
    assert!(request.authoring_knowledge.is_none());
}

/// A named release the strict door refuses is refused, typed, under its own name: never replaced
/// by the release this build embeds.
#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn a_broken_named_source_refuses_and_never_falls_back_to_the_embedded_release() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-release");
    let identity = Payload::minimal()
        .identity()
        .expect("the fixture's identity");
    let config =
        resolve(&none().with_knowledge_release(&missing, identity), &none()).expect("resolves");
    assert!(matches!(
        config.choice,
        KnowledgeChoice::Named { by: Explicit, .. }
    ));
    match attached(&config) {
        Err(KnowledgeError::Unavailable { root, .. }) => assert_eq!(root, missing),
        other => panic!("refused under its own name: {other:?}"),
    }
}

#[test]
fn a_pack_composed_elsewhere_is_refused_before_it_is_read() {
    // The file does not exist: the refusal is the door's, not a read's.
    let config =
        resolve(&none().with_knowledge_pack("/absent/pack.json"), &none()).expect("resolves");
    match attached(&config) {
        Err(KnowledgeError::PackNotAdmitted { file }) => {
            assert_eq!(file, PathBuf::from("/absent/pack.json"));
        }
        other => panic!("refused before any read: {other:?}"),
    }
}

fn refusal(result: Result<CompileRequest, KnowledgeError>) -> RefusalCode {
    match result {
        Err(KnowledgeError::Unavailable { code, .. }) => code,
        other => panic!("refused: {other:?}"),
    }
}

#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn a_named_release_enters_only_through_the_strict_door_with_its_trusted_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("release");
    let payload = Payload::minimal();
    payload.write(&root).unwrap();
    let identity = payload.identity().expect("the fixture's identity");
    let host = none().with_knowledge_release(&root, identity.clone());
    let config = resolve(&host, &none()).expect("resolves");
    let request = attached(&config).expect("admitted");
    let direct = Snapshot::open(&root, Some(&identity))
        .unwrap()
        .pack(INTENT, None)
        .unwrap();
    assert_eq!(
        request.authoring_knowledge,
        Some(direct),
        "the door's pack is the reader's"
    );
    // The same root named by a flag or by the environment carries no identity: refused, typed.
    for (explicit, env) in [
        (none().with_knowledge(&root, None), none()),
        (none(), none().with_knowledge(&root, None)),
    ] {
        let config = resolve(&explicit, &env).expect("resolves");
        assert_eq!(refusal(attached(&config)), RefusalCode::Untrusted);
    }
    // Naming another directory drops the identity named for the first.
    let renamed = host.clone().with_knowledge(&root, None);
    assert_eq!(renamed.knowledge_identity, None);
    // The historical layout under a trusted identity is refused, typed, never read as a smaller
    // release.
    let legacy = dir.path().join("legacy");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(
        legacy.join("manifest.json"),
        r#"{"knowledge_version": "k"}"#,
    )
    .unwrap();
    let config =
        resolve(&none().with_knowledge_release(&legacy, identity), &none()).expect("resolves");
    assert_eq!(refusal(attached(&config)), RefusalCode::ManifestMissing);
}

// ── the release a door lends beside its pack ───────────────────────────────────────────────────

/// The evaluation corpus of the shared r2 base vector, and the two rows it marks.
const HOLDOUT: &str = "refeng-cases-v0";
const HELD_OUT: [&str; 2] = ["counterexample:total:R0:19c2e1a5", "example:total"];

/// The base payload of the shared r2 vectors written under `root`, and the identity it is
/// admitted against.
fn r2_base(root: &Path) -> TrustedIdentity {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/knowledge-r2/p01-base.json");
    let vector: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let bytes = |entry: &serde_json::Value| {
        if let Some(text) = entry["text"].as_str() {
            return text.as_bytes().to_vec();
        }
        let hex = entry["hex"].as_str().unwrap();
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    };
    let files = (vector["files"].as_object().unwrap().iter())
        .map(|(path, entry)| (path.clone(), bytes(entry)))
        .collect();
    fixture::write_files(root, &files).unwrap();
    TrustedIdentity::from_json(&vector["expected"]).unwrap()
}

/// A door lends the release its pack was composed from with the corpus that pack held out: the
/// catalogue of the request withholds exactly the held-out rows, as the pack does, and lends
/// every other entry; `with_knowledge` attaches the same pack; an intent without words opens no
/// release and lends none.
#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn the_release_a_pack_came_from_is_lent_with_the_corpus_it_held_out() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("release");
    let identity = r2_base(&root);
    let intent = "total the paid rows of a csv file";
    let named = none().with_knowledge_release(&root, identity);
    let config = resolve(&named.clone().with_knowledge_exclude(HOLDOUT), &none()).unwrap();
    let (request, lent) = config
        .with_knowledge_lent(CompileRequest::create(intent), intent)
        .expect("admitted");
    let (snapshot, exclude) = lent.expect("the release the pack came from");
    assert_eq!(exclude.as_deref(), Some(HOLDOUT));
    let pack = request.authoring_knowledge.expect("attached");
    let again = config.with_knowledge(CompileRequest::create(intent), intent);
    assert_eq!(again.unwrap().authoring_knowledge.as_ref(), Some(&pack));
    assert_eq!(snapshot.pack(intent, Some(HOLDOUT)).unwrap(), pack);
    let whole = snapshot.entries();
    let marked: Vec<&str> = (whole.iter())
        .filter(|row| row["corpus"] == HOLDOUT)
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(marked, HELD_OUT);
    let catalogue = snapshot.catalogue(exclude.as_deref());
    let listed = catalogue.entries();
    assert_eq!(listed.len() + HELD_OUT.len(), whole.len());
    for id in HELD_OUT {
        assert!(pack.references.iter().all(|r| r.id != id), "{id} packed");
        assert!(catalogue.reference(id).is_none(), "{id} referenced");
        assert!(listed.iter().all(|row| row["id"] != id), "{id} listed");
    }
    // Without the exclusion the same release lends those rows: the holdout is the request's.
    let config = resolve(&named, &none()).unwrap();
    let (_, lent) = config
        .with_knowledge_lent(CompileRequest::create(intent), intent)
        .expect("admitted");
    let (snapshot, exclude) = lent.expect("the release the pack came from");
    assert_eq!(exclude, None);
    for id in HELD_OUT {
        assert!(
            snapshot.catalogue(None).reference(id).is_some(),
            "{id} lent"
        );
    }
    let (request, lent) = config
        .with_knowledge_lent(CompileRequest::create("  "), "  ")
        .expect("nothing to refuse");
    assert!(request.authoring_knowledge.is_none());
    assert!(lent.is_none());
}

/// Source recovery is an explicit count the door's word or the environment names (`0..=3`), only
/// under a strategy whose sketch door opens, and the one policy carries it.
#[test]
fn source_recovery_carries_any_explicit_u32_count_on_a_compatible_strategy() {
    let named = |word: &str| {
        let mut settings = none();
        settings.source_recovery = Some(word.to_owned());
        settings
    };
    assert_eq!(resolve(&none(), &none()).map(|c| c.source_recovery), Ok(0));
    let config = resolve(&named("2"), &named("3")).expect("two rounds");
    assert_eq!(
        config.source_recovery, 2,
        "the door's word outranks the environment's"
    );
    assert_eq!(
        resolve(&none(), &named("1")).map(|c| c.source_recovery),
        Ok(1)
    );
    for word in ["4", "9", "4294967295"] {
        assert_eq!(
            resolve(&named(word), &none()).map(|c| c.source_recovery),
            word.parse::<u32>()
                .map_err(|_| ConfigError::SourceRecovery(word.into()))
        );
    }
    for word in ["4294967296", "-1", "two", ""] {
        let refused = ConfigError::SourceRecovery(word.to_owned());
        assert_eq!(resolve(&named(word), &none()), Err(refused), "{word:?}");
    }
    for strategy in ["off", "only"] {
        let typed = named("1").with_strategy(strategy);
        let refused = ConfigError::SourceRecovery("1".to_owned());
        assert_eq!(resolve(&typed, &none()), Err(refused), "{strategy}");
        let zero = named("0").with_strategy(strategy);
        assert_eq!(resolve(&zero, &none()).map(|c| c.source_recovery), Ok(0));
    }
    let policy = config.policy("mock/echo", 1024, Duration::from_secs(30));
    assert_eq!(policy.map(|p| p.source_recovery), Ok(2));
}
