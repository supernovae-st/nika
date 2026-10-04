// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use super::*;
use crate::compile::{CompileRequest, compile};
use crate::knowledge::RefusalCode;
use crate::knowledge::fixture::{self, Payload};

/// An outcome the deterministic core really produced, with a decision record to stamp into.
fn outcome() -> CompileOutcome {
    let mut out = compile(&CompileRequest::create("chain")).expect("compiles");
    out.provenance.decision.get_or_insert_with(|| json!({}));
    out
}

#[test]
fn only_the_native_doors_calls_read_knowledge() {
    for call in [
        "native",
        "native-repair",
        "sketch",
        "sketch-repair",
        "fill",
        "fill-repair",
    ] {
        assert!(reads_knowledge(call), "{call}");
    }
    for call in ["plan", "repair", "transform", "natives"] {
        assert!(!reads_knowledge(call), "{call}");
    }
}

#[test]
fn a_carried_record_says_it_was_carried_and_keeps_the_rest() {
    let kept = json!({"pack_sha256": "abc", "presented": true});
    let carried = carried_record(&kept);
    assert_eq!(carried["carried"], true);
    assert_eq!(carried["pack_sha256"], "abc");
    assert_eq!(carried["presented"], true);
    // A value that is not a record is carried unchanged, never invented into one.
    assert_eq!(carried_record(&json!("x")), json!("x"));
}

#[test]
fn only_a_presented_pack_is_what_an_answer_round_carries() {
    let mut out = outcome();
    out.provenance.decision = None;
    assert_eq!(
        presented_knowledge(&out),
        None,
        "no record: nothing to carry"
    );
    for (presented, expect) in [(true, true), (false, false)] {
        out.provenance.decision = Some(json!({"session": {"authoring": {"knowledge": {
            "presented": presented,
            "pack_sha256": "abc",
        }}}}));
        assert_eq!(presented_knowledge(&out).is_some(), expect, "{presented}");
    }
}

#[test]
fn the_stamp_names_the_strategy_its_source_and_keeps_the_decision_seat() {
    let mut out = outcome();
    out.provenance.decision =
        Some(json!({"session": {"decision_seat": {"model": "m"}}, "kept": 1}));
    let knowledge = json!({"pack_sha256": "abc"});
    stamp(&mut out, "escalate", "environment", Some(&knowledge));
    let decision = out.provenance.decision.expect("stamped");
    assert_eq!(decision["session"]["authoring"]["strategy"], "escalate");
    assert_eq!(decision["session"]["authoring"]["source"], "environment");
    assert_eq!(
        decision["session"]["authoring"]["knowledge"]["pack_sha256"],
        "abc"
    );
    assert_eq!(decision["session"]["decision_seat"]["model"], "m");
    assert_eq!(
        decision["kept"], 1,
        "the compiler's own record stays beside the stamp"
    );
    let mut bare = outcome();
    bare.provenance.decision = None;
    stamp(&mut bare, "off", "default", None);
    let decision = bare.provenance.decision.expect("created");
    assert!(decision["session"]["authoring"]["knowledge"].is_null());
    assert!(decision["session"].get("decision_seat").is_none());
}

#[test]
fn the_observed_record_counts_columns_and_never_carries_their_names() {
    let world = json!({"observed": [
        {"path": "data/orders.csv", "state": "present", "kind": "csv",
         "columns": ["customer", "amount", "status"]},
        {"path": "notes.md", "state": "absent", "kind": null},
    ]});
    let out = observed_in(outcome(), Some(&world));
    let record = &out.provenance.decision.expect("decision")["session"]["observed"];
    assert_eq!(record["attached"], true);
    assert_eq!(record["presented"], false, "no native call read a pack");
    assert_eq!(record["rows"][0]["columns"], 3);
    assert_eq!(record["rows"][1]["columns"], 0);
    assert!(!record.to_string().contains("customer"), "{record}");
    // C10: the identity of the whole world attached, beside the names-free summary.
    assert_eq!(record["world_sha256"], json!(world_sha256(&world)));
    // Nothing observed: the outcome's record is untouched.
    let untouched = outcome();
    let before = untouched.provenance.decision.clone();
    assert_eq!(observed_in(untouched, None).provenance.decision, before);
}

#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn a_pin_names_its_identity_in_words_and_refuses_what_is_no_snapshot() {
    let words = KnowledgePin::words(Some("v1"), None, &"a".repeat(64), &"b".repeat(64));
    assert_eq!(
        words,
        "v1 (declared digest none · manifest aaaaaaaaaaaa · rows bbbbbbbbbbbb)"
    );
    assert_eq!(short("0123456789abcdef"), "0123456789ab");
    let dir = tempfile::tempdir().expect("an empty directory");
    let refused = KnowledgePin::open(dir.path().to_path_buf(), None, None);
    assert!(
        matches!(
            refused,
            Err(KnowledgeError::Unavailable {
                code: RefusalCode::Untrusted,
                ..
            })
        ),
        "without a trusted identity nothing is pinned: {refused:?}"
    );
    let refused = KnowledgePin::open(
        dir.path().to_path_buf(),
        None,
        Payload::minimal().identity(),
    );
    assert!(
        matches!(
            refused,
            Err(KnowledgeError::Unavailable {
                code: RefusalCode::ManifestMissing,
                ..
            })
        ),
        "a directory that is no release is never pinned: {refused:?}"
    );
}

/// C10 · D-K · a pin names both identities when the release moved under it: rows it did not
/// admit, a manifest re-sealed over the same rows; the same release has not moved.
#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn a_pin_says_when_its_snapshot_moved_under_it() {
    let dir = tempfile::tempdir().expect("a release");
    let root = dir.path().join("release");
    Payload::minimal().write(&root).expect("written");
    let identity = Payload::minimal()
        .identity()
        .expect("the fixture's identity");
    let pin = KnowledgePin::open(root.clone(), None, Some(identity.clone())).expect("pinned");
    let now = |identity: &TrustedIdentity| {
        Snapshot::open(&root, Some(identity)).expect("admitted against its own identity")
    };
    assert_eq!(
        pin.moved(&now(&identity)),
        None,
        "the same release has not moved"
    );
    assert!(
        pin.reopen().is_ok(),
        "the pinned release reopens against its identity"
    );
    // Another pattern, re-sealed: refused against the pin's identity, admitted only on its own,
    // and not the release pinned.
    let mut edited = Payload::minimal();
    edited.row("pattern:summarize").expect("a pattern")["notes"] = json!("another note");
    std::fs::remove_dir_all(&root).expect("replaced");
    edited.write(&root).expect("written");
    assert!(
        matches!(
            pin.reopen(),
            Err(KnowledgeError::Unavailable {
                code: RefusalCode::IdentityMismatch,
                ..
            })
        ),
        "the pin's identity refuses the edited release"
    );
    let theirs = edited.identity().expect("its own identity");
    let (pinned, found) = pin
        .moved(&now(&theirs))
        .expect("rows it did not admit moved it");
    assert!(pinned.starts_with(fixture::VERSION), "{pinned}");
    assert_ne!(pinned, found, "the rows differ in words");
    // The same rows under a manifest re-sealed by another producer: its own bytes moved it.
    let mut files = Payload::minimal().render();
    let mut manifest = Payload::manifest(&files);
    manifest["tool"]["sha256"] = json!("d".repeat(64));
    files.insert(
        fixture::manifest_path().to_owned(),
        fixture::manifest_bytes(&manifest),
    );
    std::fs::remove_dir_all(&root).expect("replaced");
    fixture::write_files(&root, &files).expect("written");
    let resealed = now(&fixture::identity_of(&files).expect("its own identity"));
    let (_, found) = pin.moved(&resealed).expect("a re-sealed manifest moved it");
    assert!(
        found.contains(&format!("manifest {}", short(resealed.manifest_sha256()))),
        "{found}"
    );
    assert_eq!(resealed.rows_sha256(), pin.rows_sha256, "the same rows");
}

/// The release this build embeds, pinned: its origin and its record name no path, it reopens
/// through the strict memory door against the same identity and has not moved, pinning it again
/// pins the same, and it is what a configuration naming nothing pins. Without its identity the
/// memory door refuses it, typed.
#[test]
fn an_embedded_pin_reopens_from_memory_with_the_same_identity_and_record() {
    let pin = KnowledgePin::embedded(Some("heldout".to_owned())).expect("pinned");
    assert_eq!(pin.origin, KnowledgeOrigin::Embedded);
    assert_eq!(
        pin.manifest_sha256,
        "b787fc53d6858db43d55958daaf02539fadcad4feeacc17b63c5aefcb92cc32b"
    );
    assert_eq!(pin.version.as_deref(), Some("knowledge-0.122.0-r3"));
    let reopened = pin.reopen().expect("admitted again in memory");
    assert_eq!(pin.moved(&reopened), None, "the same release has not moved");
    let again = KnowledgePin::embedded(Some("heldout".to_owned())).expect("pinned again");
    assert_eq!(again, pin);
    let record = pin.record();
    assert_eq!(again.record(), record);
    assert_eq!(record["source"], "embedded");
    assert!(record.get("dir").is_none(), "no path: {record}");
    assert_eq!(record["snapshot_sha256"], pin.manifest_sha256.as_str());
    assert_eq!(record["policy"]["id"], "policy-r");
    assert_eq!(record["admission"], ADMISSION_PROFILE);
    assert_eq!(record["exclude_corpus"], "heldout");
    assert!(
        pin.status_words().ends_with(" · admitted · embedded"),
        "{}",
        pin.status_words()
    );
    let none = crate::compile_config::AuthoringSettings::none();
    let config = crate::compile_config::resolve(&none, &none).expect("resolves");
    assert_eq!(
        KnowledgePin::of_config(&config).expect("pinned"),
        Some(KnowledgePin::embedded(None).expect("pinned"))
    );
    let mut untrusted = pin;
    untrusted.identity = None;
    assert!(matches!(
        untrusted.reopen(),
        Err(KnowledgeError::Unavailable {
            code: RefusalCode::Untrusted,
            ..
        })
    ));
}

/// C10 · the decision seat's receipt sits beside the compiler's own record of the same
/// questions, created when there is none, and the session's later stamp keeps it.
#[test]
fn the_seat_receipt_is_stamped_beside_the_record_and_kept_by_the_stamp() {
    let mut out = outcome();
    out.provenance.decision = Some(json!({"kept": 1}));
    stamp_seat(&mut out, json!({"model": "typesafe/jev"}));
    stamp(&mut out, "escalate", "environment", None);
    let decision = out.provenance.decision.expect("stamped");
    assert_eq!(
        decision["session"]["decision_seat"]["model"],
        "typesafe/jev"
    );
    assert_eq!(decision["session"]["authoring"]["strategy"], "escalate");
    assert_eq!(decision["kept"], 1, "the compiler's own record stays");
    let mut bare = outcome();
    bare.provenance.decision = None;
    stamp_seat(&mut bare, json!({"model": "m"}));
    let created = bare.provenance.decision.expect("created");
    assert_eq!(created["session"]["decision_seat"]["model"], "m");
}

/// C11 · a pin's identity in the words the status line says: the declared version, the
/// release's `SNAPSHOT_SHA256` and the rows' digest cut at twelve, and its admission; the record a
/// receipt carries names the same identity in full.
#[test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn a_pin_says_its_identity_as_the_status_line_reads_it() {
    let dir = tempfile::tempdir().expect("a release");
    let root = dir.path().join("release");
    Payload::minimal().write(&root).expect("written");
    let pin = KnowledgePin::open(root, None, Payload::minimal().identity()).expect("pinned");
    assert_eq!(
        pin.status_words(),
        format!(
            "knowledge {} · snapshot {} · rows {} · admitted",
            fixture::VERSION,
            short(&pin.manifest_sha256),
            short(&pin.rows_sha256)
        )
    );
    let record = pin.record();
    assert_eq!(record["snapshot_sha256"], json!(pin.manifest_sha256));
    assert_eq!(record["source"], "disk");
    assert!(
        record["dir"].is_string(),
        "a release on disk names its root"
    );
    assert_eq!(record["admission"], ADMISSION_PROFILE);
    assert_eq!(record["policy"]["id"], fixture::POLICY_ID);
    assert_eq!(
        record["digest"],
        Value::Null,
        "a release declares no digest"
    );
}

/// C11 · B19 · the `/details` words of the knowledge record, descended here from the session byte
/// for byte: each branch (none attached, fields absent, presented with its calls and a direct
/// seat, carried with usage unreported, a subscription seat, no seat, not presented with and
/// without its reason) reads exactly what the session's own lines read before the move, as
/// captured from the pre-descent code.
#[test]
fn the_knowledge_lines_read_each_branch_as_the_session_read_it() {
    const CASES: [(&str, &str, &str); 10] = [
        (
            "none_attached",
            r#"{"strategy": "escalate", "source": "default", "knowledge": null}"#,
            "\n  authoring strategy: escalate (default)\n  knowledge: none attached",
        ),
        (
            "record_without_fields",
            "{}",
            "\n  authoring strategy: unknown (unknown)\n  knowledge: none attached",
        ),
        (
            "presented_direct_usage_reported",
            r#"{"strategy": "native", "source": "host", "knowledge": {"identity": {"version": "v1.2", "digest": "sha256:abcdef0123456789", "manifest_sha256": "0123456789abcdef0123", "rows_sha256": "fedcba9876543210fedc"}, "pack_builder": "nika-pack@1", "pack_sha256": "aaaabbbbccccddddeeeeffff0000111122223333444455556666777788889999", "references": [{"kind": "pattern", "id": "p-1", "bytes": 120, "sha256": "1111222233334444aaaa"}, {"kind": "example", "id": "e-2", "bytes": 30, "sha256": "5555666677778888bbbb"}], "repairs": 1, "presented": true, "why": null, "calls": [{"call": "native", "instruction_sha256": "9999aaaa"}, {"call": "native-repair-1", "instruction_sha256": "8888bbbb"}], "seat": {"model": "deepseek/deepseek-v4-pro", "calls": 2, "input_tokens": 300, "output_tokens": 90, "elapsed_ms": 1500, "backend": {"kind": "direct_api", "provider": "deepseek", "host": "api.deepseek.com"}}, "carried": false}}"#,
            "\n  authoring strategy: native (host)\n  knowledge: v1.2 · declared digest sha256:abcde · manifest 0123456789ab · rows fedcba987654 · 2 references · 150 B · nika-pack@1\n  pack sha256 aaaabbbbccccddddeeeeffff0000111122223333444455556666777788889999\n    pattern p-1 · 120 B · sha256 111122223333\n    example e-2 · 30 B · sha256 555566667777\n  presented to the seat in 2 calls\n    native · instruction sha256 9999aaaa\n    native-repair-1 · instruction sha256 8888bbbb\n    by deepseek/deepseek-v4-pro · host api.deepseek.com · 2 calls in that round · 300 in / 90 out tokens · 1500 ms",
        ),
        (
            "carried_one_call_usage_unreported",
            r#"{"strategy": "native", "source": "environment", "knowledge": {"identity": {"version": "v2", "digest": "d", "manifest_sha256": "m", "rows_sha256": "r"}, "pack_builder": "nika-pack@1", "pack_sha256": "p", "references": [{"kind": "block", "id": "b-9", "bytes": 7, "sha256": "0000"}], "presented": true, "why": null, "calls": [{"call": "sketch", "instruction_sha256": "7777"}], "seat": {"model": "mistral/mistral-large-latest", "calls": 1, "input_tokens": null, "output_tokens": null, "elapsed_ms": 42, "backend": {"kind": "direct_api", "host": "api.mistral.ai"}}, "carried": true}}"#,
            "\n  authoring strategy: native (environment)\n  knowledge: v2 · declared digest d · manifest m · rows r · 1 reference · 7 B · nika-pack@1\n  pack sha256 p\n    block b-9 · 7 B · sha256 0000\n  presented to the seat in 1 call of the round that authored this candidate (this answer round replayed it and presented the pack to no call)\n    sketch · instruction sha256 7777\n    by mistral/mistral-large-latest · host api.mistral.ai · 1 call in that round · usage not reported by the provider · 42 ms",
        ),
        (
            "presented_subscription_seat",
            r#"{"strategy": "native", "source": "default", "knowledge": {"identity": {"version": "v3"}, "pack_builder": "nika-pack@1", "pack_sha256": "q", "references": [], "presented": true, "calls": [{"call": "fill-1", "instruction_sha256": null}], "seat": {"calls": 3, "backend": {"kind": "harness_infer", "adapter": "claude-code"}}, "carried": false}}"#,
            "\n  authoring strategy: native (default)\n  knowledge: v3 · declared digest none · manifest none · rows none · 0 references · 0 B · nika-pack@1\n  pack sha256 q\n  presented to the seat in 1 call\n    fill-1 · instruction sha256 none\n    by subscription claude-code · 3 call(s) · responding identities in backend receipt · cost unknown",
        ),
        (
            "presented_no_seat_no_calls",
            r#"{"strategy": "native", "source": "default", "knowledge": {"identity": {}, "presented": true, "seat": null}}"#,
            "\n  authoring strategy: native (default)\n  knowledge: unversioned · declared digest none · manifest none · rows none · 0 references · 0 B · unknown builder\n  pack sha256 none\n  presented to the seat in 0 calls",
        ),
        (
            "not_presented_with_why",
            r#"{"strategy": "escalate", "source": "host", "knowledge": {"identity": {"version": "v1.2", "digest": "sha256:abcdef0123456789", "manifest_sha256": "0123456789abcdef0123", "rows_sha256": "fedcba9876543210fedc"}, "pack_builder": "nika-pack@1", "pack_sha256": "abc", "references": [{"kind": "skill", "id": "s-1", "bytes": 2048, "sha256": "cafe"}], "presented": false, "why": "the request settled on the recipe path; only the native door reads knowledge", "calls": [], "seat": null, "carried": false}}"#,
            "\n  authoring strategy: escalate (host)\n  knowledge: v1.2 · declared digest sha256:abcde · manifest 0123456789ab · rows fedcba987654 · 1 reference · 2048 B · nika-pack@1\n  pack sha256 abc\n    skill s-1 · 2048 B · sha256 cafe\n  not presented: the request settled on the recipe path; only the native door reads knowledge",
        ),
        (
            "not_presented_without_fields",
            r#"{"strategy": "native", "source": "default", "knowledge": {"references": [{}], "presented": false}}"#,
            "\n  authoring strategy: native (default)\n  knowledge: unversioned · declared digest none · manifest none · rows none · 1 reference · 0 B · unknown builder\n  pack sha256 none\n    ? ? · 0 B · sha256 none\n  not presented: the native door did not read it",
        ),
        (
            "direct_seat_without_fields",
            r#"{"strategy": "native", "source": "default", "knowledge": {"identity": {"digest": 42}, "presented": true, "calls": [{}], "seat": {"backend": {"kind": "direct_api"}}}}"#,
            "\n  authoring strategy: native (default)\n  knowledge: unversioned · declared digest none · manifest none · rows none · 0 references · 0 B · unknown builder\n  pack sha256 none\n  presented to the seat in 1 call\n    ? · instruction sha256 none\n    by unknown model · host unknown · 0 calls in that round · usage not reported by the provider · 0 ms",
        ),
        (
            "presented_absent",
            r#"{"strategy": "native", "source": "default", "knowledge": {"identity": {"version": "v4", "digest": "e", "manifest_sha256": "n", "rows_sha256": "s"}, "pack_builder": "nika-pack@1", "pack_sha256": "t", "references": [{"kind": "pattern", "id": "p-4", "bytes": 3, "sha256": "abcd"}], "calls": [{"call": "native", "instruction_sha256": "1234"}], "seat": {"model": "deepseek/deepseek-v4-pro", "calls": 1, "input_tokens": 1, "output_tokens": 2, "elapsed_ms": 3, "backend": {"host": "api.deepseek.com"}}}}"#,
            "\n  authoring strategy: native (default)\n  knowledge: v4 · declared digest e · manifest n · rows s · 1 reference · 3 B · nika-pack@1\n  pack sha256 t\n    pattern p-4 · 3 B · sha256 abcd\n  not presented: the native door did not read it",
        ),
    ];
    for (name, record, words) in CASES {
        let record: Value = serde_json::from_str(record).expect(name);
        let mut text = String::new();
        knowledge_lines(&record, &mut text);
        assert_eq!(text, words, "{name}");
    }
}

/// The private Plan door's seat for the tests below: every call answered with `reply` (any
/// text: a malformed or empty return is still a return), or failed when `reply` is `None`.
struct PlanSeat {
    reply: Option<&'static str>,
}

impl nika_kernel::ai::provider::ProviderInferDyn for PlanSeat {
    async fn infer(
        &self,
        _request: nika_kernel::ai::provider::InferRequest,
    ) -> Result<nika_kernel::ai::provider::InferResponse, nika_kernel::ai::provider::ProviderError>
    {
        use nika_kernel::ai::provider::{
            ContentBlock, InferResponse, ProviderError, StopReason, TokenUsage,
        };
        let Some(text) = self.reply else {
            return Err(ProviderError::Other {
                reason: "unavailable".to_owned(),
            });
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: text.to_owned(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

const PLAN_INTENT: &str = fixture::INTENT;

/// A pinned release on disk, the pack it composes for the request as the session composes it
/// (reopened under the pin), the world a host attached, and a real compile through the private
/// Plan door (`NativeMode::Off`, an explicit provider) with both attached.
async fn planned(
    reply: Option<&'static str>,
) -> (KnowledgePin, AuthoringKnowledge, Value, CompileOutcome) {
    use crate::compile::{AuthoringPolicy, NativeMode, compile_with_provider};
    let dir = tempfile::tempdir().expect("a release");
    let root = dir.path().join("release");
    Payload::minimal().write(&root).expect("written");
    let identity = Payload::minimal()
        .identity()
        .expect("the fixture's identity");
    let pin = KnowledgePin::open(root, None, Some(identity)).expect("pinned");
    let pack = (pin.reopen().expect("admitted"))
        .pack(PLAN_INTENT, None)
        .expect("composed");
    assert!(
        !pack.references.is_empty(),
        "the release recalls references"
    );
    let world = json!({"observed": [{"path": "./tickets.json", "state": "present", "kind": "json",
        "columns": ["id", "body"]}]});
    let policy = AuthoringPolicy::new("mock/authoring", 1024, std::time::Duration::from_secs(2))
        .with_native(NativeMode::Off);
    let request = CompileRequest::create(PLAN_INTENT)
        .with_authoring_policy(policy)
        .with_authoring_knowledge(pack.clone())
        .with_knowledge(world.clone());
    let out = compile_with_provider(&request, &PlanSeat { reply })
        .await
        .expect("compiles");
    (pin, pack, world, out)
}

fn plan_calls(out: &mut CompileOutcome) -> Vec<&mut Value> {
    (out.provenance
        .authoring
        .as_mut()
        .expect("journaled")
        .context
        .iter_mut())
    .filter(|call| call["call"] == "plan" || call["call"] == "repair")
    .collect()
}

/// The Plan door's attested return of this very pack is what Onboard records as
/// presented, and the world it was prepared over is observed as presented; the call is named.
/// A malformed or empty return is a return: it never means a candidate.
#[tokio::test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
async fn a_returned_plan_call_carrying_this_pack_is_recorded_as_presented() {
    for reply in ["not json at all", ""] {
        let (pin, pack, world, out) = planned(Some(reply)).await;
        let record = composed_record(&pin, &pack, &out);
        assert_eq!(record["presented"], true, "{reply:?}: {record:#}");
        assert!(record.get("why").is_none_or(Value::is_null), "{record:#}");
        let calls = record["calls"].as_array().expect("calls");
        assert!(calls.iter().any(|c| c["call"] == "plan"), "{record:#}");
        let observed = observed_in(out, Some(&world));
        let decision = observed.provenance.decision.expect("decision");
        assert_eq!(decision["session"]["observed"]["presented"], true);
    }
}

/// Negatives: every mutation of the receipt that removes the attestation of this pack,
/// this world or the return leaves the pack not presented, in words that never claim delivery.
#[tokio::test]
#[cfg(unix)] // the disk form is defined for Unix descriptors only
async fn a_plan_call_without_an_attested_return_of_this_pack_presents_nothing() {
    type Mutation = fn(&mut Value);
    let (pin, pack, world, base) = planned(Some("{}")).await;
    let cases: [(&str, Mutation, &str); 7] = [
        (
            "marker removed",
            |c| drop(c.as_object_mut().map(|m| m.remove("semantic_context"))),
            "native door",
        ),
        (
            "pack digest changed",
            |c| c["semantic_context"]["pack_sha256"] = json!("0".repeat(64)),
            "expected context is not attested",
        ),
        (
            "references dropped",
            |c| c["references"] = json!([]),
            "expected context is not attested",
        ),
        (
            "result stripped",
            |c| drop(c.as_object_mut().map(|m| m.remove("result"))),
            "return is not attested",
        ),
        (
            "result contradictory",
            |c| c["result"]["failure_kind"] = json!("timeout"),
            "return is not attested",
        ),
        (
            "admission refused",
            |c| c["result"] = json!({"failure_kind": "admission_refused"}),
            "admission refused, no response observed",
        ),
        (
            "timeout",
            |c| c["result"] = json!({"failure_kind": "timeout"}),
            "delivery and cost unknown",
        ),
    ];
    for (name, mutate, why) in cases {
        let mut out = base.clone();
        for call in plan_calls(&mut out) {
            mutate(call);
        }
        let record = composed_record(&pin, &pack, &out);
        assert_eq!(record["presented"], false, "{name}: {record:#}");
        let said = record["why"].as_str().unwrap_or_default();
        assert!(said.contains(why), "{name}: {said}");
        assert!(
            record["calls"].as_array().is_none_or(Vec::is_empty),
            "{name}: {record:#}"
        );
    }
    // A world other than the one the call was prepared over is not observed as presented.
    let mut other = world.clone();
    other["observed"][0]["path"] = json!("./other.json");
    let decision = observed_in(base, Some(&other))
        .provenance
        .decision
        .expect("decision");
    assert_eq!(decision["session"]["observed"]["presented"], false);
    // A real provider failure: the context was prepared, no response observed.
    let (pin, pack, world, failed) = planned(None).await;
    let record = composed_record(&pin, &pack, &failed);
    assert_eq!(record["presented"], false, "{record:#}");
    let said = record["why"].as_str().unwrap_or_default();
    assert!(
        said.contains("no response observed, delivery and cost unknown"),
        "{said}"
    );
    let decision = observed_in(failed, Some(&world))
        .provenance
        .decision
        .expect("decision");
    assert_eq!(decision["session"]["observed"]["presented"], false);
}

/// A semantic record is closed and keeps no observation: for it alone the session's record
/// discloses the exact observation attached, beside its identity; any other outcome keeps only
/// the names-free summary and the identity.
#[test]
fn only_a_semantic_outcome_discloses_the_exact_observation_it_read() {
    let world = json!({"observed": [{"path": "inventory.json", "state": "observed",
        "kind": "json", "columns": ["sku", "stock"]}]});
    let mut semantic = outcome();
    semantic.provenance.plan = Some(json!({"semantic_record": 1}));
    let out = observed_in(semantic, Some(&world));
    let record = &out.provenance.decision.expect("decision")["session"]["observed"];
    assert_eq!(record["world"], world);
    assert_eq!(record["world_sha256"], json!(world_sha256(&world)));
    let out = observed_in(outcome(), Some(&world));
    let record = &out.provenance.decision.expect("decision")["session"]["observed"];
    assert!(record.get("world").is_none(), "{record}");
    assert!(!record.to_string().contains("stock"), "{record}");
}
