// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `resume` test proofs — the unit half and the REAL-run trace-carry
//! half. Split out of `resume.rs` at the C2 wall (the 1500-LOC file
//! ratchet) and migrated to the four-authority family (inputs · config ·
//! const); `super::super` resolves to the `resume` module from the unit
//! half, so the moved cases read their subject unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod unit {
    use super::super::*;
    use crate::record::{TaskRecord, TaskStatus};

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn no_secrets() -> ResumeContext {
        ResumeContext {
            markers: BTreeMap::new(),
            secret_values: Vec::new(),
            default_model: None,
            skills: BTreeMap::new(),
            child_closures: BTreeMap::new(),
            access_pin: None,
        }
    }

    fn stamp_of(
        yaml: &str,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
    ) -> Option<ResumeStamp> {
        let wf = parse(yaml);
        let ctx = no_secrets();
        stamp(&wf.tasks[0].value, records, vars, &BTreeMap::new(), &ctx)
    }

    /// The `const:`-riding twin — the map lands in the consts slot (the
    /// fan-out fixtures read `${{ const.X }}`, never `${{ inputs.X }}`).
    fn stamp_const_of(
        yaml: &str,
        records: &BTreeMap<String, TaskRecord>,
        consts: &BTreeMap<String, Value>,
    ) -> Option<ResumeStamp> {
        let wf = parse(yaml);
        let ctx = no_secrets();
        stamp(&wf.tasks[0].value, records, &BTreeMap::new(), consts, &ctx)
    }

    fn success_record(output: Value) -> TaskRecord {
        let mut rec = TaskRecord::unran(TaskStatus::Success, crate::record::TerminalCause::Normal);
        rec.output = output;
        rec
    }

    const BASE: &str = "nika: t\ninputs:\n  topic: { type: string, required: false, default: \"news\" }\ntasks:\n  ask:\n    infer: { prompt: \"about ${{ inputs.topic }}\" }\n";

    #[test]
    fn key_is_stable_across_recomputation() {
        let records = BTreeMap::new();
        let vars = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let a = stamp_of(BASE, &records, &vars).expect("eligible");
        let b = stamp_of(BASE, &records, &vars).expect("eligible");
        assert_eq!(a, b, "same task + same scope → same stamp, forever");
        assert_eq!(a.def_hash.len(), 64, "blake3 hex");
        assert_eq!(a.input_hash.len(), 64, "blake3 hex");
    }

    #[test]
    fn definition_edit_changes_the_definition_hash() {
        // Trap 6: an edited task NEVER silently skips.
        let edited = BASE.replace("about ${{ inputs.topic }}", "summarize ${{ inputs.topic }}");
        let records = BTreeMap::new();
        let vars = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let a = stamp_of(BASE, &records, &vars).expect("eligible");
        let b = stamp_of(&edited, &records, &vars).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash, "prompt edit → new definition");
        // The rendered input changed too (the prompt text renders in).
        assert_ne!(a.input_hash, b.input_hash);
    }

    #[test]
    fn var_change_changes_the_input_hash_not_the_definition() {
        let records = BTreeMap::new();
        let news = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let rust = BTreeMap::from([("topic".to_owned(), json!("rust"))]);
        let a = stamp_of(BASE, &records, &news).expect("eligible");
        let b = stamp_of(BASE, &records, &rust).expect("eligible");
        assert_eq!(a.def_hash, b.def_hash, "the task text is unchanged");
        assert_ne!(a.input_hash, b.input_hash, "the resolved value changed");
    }

    #[test]
    fn upstream_output_change_cascades_into_the_input_hash() {
        const DOWNSTREAM: &str = "nika: t\ntasks:\n  use:\n    exec: { command: [\"echo\", \"${{ tasks.up.output }}\"] }\n";
        let vars = BTreeMap::new();
        let r1 = BTreeMap::from([("up".to_owned(), success_record(json!("v1")))]);
        let r2 = BTreeMap::from([("up".to_owned(), success_record(json!("v2")))]);
        let a = stamp_of(DOWNSTREAM, &r1, &vars).expect("eligible");
        let b = stamp_of(DOWNSTREAM, &r2, &vars).expect("eligible");
        assert_eq!(a.def_hash, b.def_hash);
        assert_ne!(
            a.input_hash, b.input_hash,
            "ancestor invalidation rides the rendered value"
        );
    }

    /// Secrets participate BY NAME (declared reference identity), never
    /// by value: rotating the value leaves the key unchanged (ADR-099's
    /// documented sharp edge — `--from` is the override), while renaming
    /// the reference re-keys.
    #[test]
    fn secret_value_never_participates_the_reference_identity_does() {
        const WITH_SECRET: &str = "nika: t\nsecrets:\n  tok: { source: env, key: MY_TOKEN }\ntasks:\n  call:\n    exec: { command: [\"curl\", \"-H\", \"'x:\", \"${{ secrets.tok }}'\"] }\n";
        let wf = parse(WITH_SECRET);
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let ctx_v1 = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-1"))]),
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        );
        let ctx_v2 = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-2"))]),
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        );
        let a = stamp(
            &wf.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx_v1,
        )
        .expect("eligible");
        let b = stamp(
            &wf.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx_v2,
        )
        .expect("eligible");
        assert_eq!(a, b, "a rotated secret VALUE does not re-key (by-name)");

        // A different declared reference (key path) IS a different identity.
        let rekeyed = WITH_SECRET.replace("MY_TOKEN", "OTHER_TOKEN");
        let wf2 = parse(&rekeyed);
        let ctx2 = ResumeContext::of(
            &wf2,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-1"))]),
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        );
        let c = stamp(
            &wf2.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx2,
        )
        .expect("eligible");
        assert_ne!(a.input_hash, c.input_hash, "reference identity re-keys");
    }

    /// #409 · the EFFECTIVE default model is part of a model-less
    /// infer task's DEFINITION identity: swapping the envelope `model:`
    /// (or supplying `--model`) re-runs it — a resume must never serve
    /// output produced by a different model than the file now declares.
    #[test]
    fn default_model_swap_rekeys_a_modelless_infer() {
        const MODELLESS: &str = "nika: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  summary:\n    infer: { prompt: \"hi\" }\n";
        // A task that PINS its own `model:` · an exec task — the two
        // no-re-key controls (items live at scope top · lint law).
        const PINNED: &str = "nika: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  summary:\n    infer: { prompt: \"hi\", model: \"mock/echo\" }\n";
        const EXEC: &str = "nika: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  run:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let stamp_with = |yaml: &str, over: Option<&str>| {
            let wf = parse(yaml);
            let ctx = ResumeContext::of(
                &wf,
                &BTreeMap::new(),
                over,
                &BTreeMap::new(),
                &BTreeMap::new(),
                None,
            );
            stamp(&wf.tasks[0].value, &records, &vars, &BTreeMap::new(), &ctx).expect("eligible")
        };

        // The issue's exact repro: edit ONLY the envelope `model:` line.
        let a = stamp_with(MODELLESS, None);
        let swapped = MODELLESS.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b");
        let b = stamp_with(&swapped, None);
        assert_ne!(
            a.def_hash, b.def_hash,
            "the envelope model re-keys the model-less infer"
        );

        // A `--model` override replaces the resolved default → re-keys too.
        let o = stamp_with(MODELLESS, Some("mistral/small"));
        assert_ne!(a.def_hash, o.def_hash, "a --model override re-keys");
        // …and the SAME override is stable (no churn).
        assert_eq!(
            o.def_hash,
            stamp_with(MODELLESS, Some("mistral/small")).def_hash
        );

        // A task that PINS its own `model:` ignores the envelope — the
        // default cannot affect what it runs, so no needless re-run.
        let p1 = stamp_with(PINNED, None);
        let p2 = stamp_with(
            &PINNED.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b"),
            None,
        );
        assert_eq!(
            p1.def_hash, p2.def_hash,
            "a pinned per-task model ignores the envelope"
        );

        // An exec task never reads the default model — stable across it.
        let e1 = stamp_with(EXEC, None);
        let e2 = stamp_with(
            &EXEC.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b"),
            None,
        );
        assert_eq!(e1.def_hash, e2.def_hash, "exec ignores the model line");
    }

    /// R-1 (P3 · the #409 precedent's ACCESS twin — pin half): a run
    /// pinned `codex-acp` resumed under `api` must RE-RUN the infer/agent
    /// task (envelope fidelity differs by access class — never serve the
    /// other path's cached output). An exec task's identity never reads
    /// the pin. (The chosen-access half lands with the B6 registry — its
    /// trigger, >1 access per provider, is unreachable before it.)
    #[test]
    fn the_access_pin_rekeys_infer_and_agent_only() {
        const AGENT: &str =
            "nika: t\nmodel: mock/echo\ntasks:\n  go:\n    agent: { prompt: \"hi\" }\n";
        // An exec control (items live at scope top · the lint law).
        const EXEC: &str = "nika: t\nmodel: mock/echo\ntasks:\n  run:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let stamp_with = |yaml: &str, pin: Option<&str>| {
            let wf = parse(yaml);
            let ctx = ResumeContext::of(
                &wf,
                &BTreeMap::new(),
                None,
                &BTreeMap::new(),
                &BTreeMap::new(),
                pin,
            );
            stamp(&wf.tasks[0].value, &records, &vars, &BTreeMap::new(), &ctx).expect("eligible")
        };
        let unpinned = stamp_with(AGENT, None);
        let pinned = stamp_with(AGENT, Some("codex-acp"));
        assert_ne!(
            unpinned.def_hash, pinned.def_hash,
            "a pin joins the identity — resuming under one re-runs"
        );
        let other_pin = stamp_with(AGENT, Some("api"));
        assert_ne!(
            pinned.def_hash, other_pin.def_hash,
            "a DIFFERENT pin re-keys (the R-1 repro: codex-acp vs api)"
        );
        assert_eq!(
            pinned.def_hash,
            stamp_with(AGENT, Some("codex-acp")).def_hash,
            "the same pin is stable (no churn)"
        );

        // Exec never reads the pin.
        assert_eq!(
            stamp_with(EXEC, None).def_hash,
            stamp_with(EXEC, Some("codex-acp")).def_hash,
            "an exec task's identity ignores the pin"
        );
    }

    /// #473 · an agent task's `skills:` participate in its DEFINITION
    /// identity by TEXT (spec 02 §agent skills · the ADR-099 law): an
    /// edited SKILL.md re-runs the task; the same text is stable; a
    /// different PATH with the same text re-keys (the path list is part
    /// of the verb body as written).
    #[test]
    fn skill_edit_rekeys_the_agent_definition() {
        const WITH_SKILL: &str = "nika: t\nmodel: mock/echo\ntasks:\n  go:\n    agent: { prompt: \"hi\", skills: [\"s/SKILL.md\"] }\n";
        // The skill-less control (items live at scope top · lint law).
        const PLAIN: &str =
            "nika: t\nmodel: mock/echo\ntasks:\n  go:\n    agent: { prompt: \"hi\" }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let stamp_with = |yaml: &str, skills: &BTreeMap<String, String>| {
            let wf = parse(yaml);
            let ctx =
                ResumeContext::of(&wf, &BTreeMap::new(), None, skills, &BTreeMap::new(), None);
            stamp(&wf.tasks[0].value, &records, &vars, &BTreeMap::new(), &ctx)
        };
        let v1 = BTreeMap::from([(
            "s/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv1 body\n".to_owned(),
        )]);
        let v2 = BTreeMap::from([(
            "s/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv2 body\n".to_owned(),
        )]);

        let a = stamp_with(WITH_SKILL, &v1).expect("eligible");
        let b = stamp_with(WITH_SKILL, &v2).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash, "an edited skill re-runs the task");

        // Same text → stable (no churn).
        let a2 = stamp_with(WITH_SKILL, &v1).expect("eligible");
        assert_eq!(a, a2, "unchanged skill text → same stamp");

        // A different path carrying the SAME text still re-keys (the
        // path list is the verb body as written · ADR-099 §1).
        let repathed = WITH_SKILL.replace("s/SKILL.md", "other/SKILL.md");
        let moved = BTreeMap::from([(
            "other/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv1 body\n".to_owned(),
        )]);
        let c = stamp_with(&repathed, &moved).expect("eligible");
        assert_ne!(a.def_hash, c.def_hash, "the path is part of the body");

        // A referenced skill the composer did not resolve → NOT eligible
        // (records no key · never skips · the honest degrade).
        assert!(
            stamp_with(WITH_SKILL, &BTreeMap::new()).is_none(),
            "unresolved skill text → no resume claim"
        );

        // A skill-less agent ignores the map entirely (control).
        let p1 = stamp_with(PLAIN, &v1).expect("eligible");
        let p2 = stamp_with(PLAIN, &BTreeMap::new()).expect("eligible");
        assert_eq!(p1, p2, "no skills: → the map never participates");
    }

    /// A resolved secret value that leaked into the rendered inputs via
    /// an UPSTREAM record disqualifies the stamp — the trace never
    /// carries secret-derived material, not even inside a hash.
    #[test]
    fn secret_leaked_through_an_upstream_record_disables_the_stamp() {
        const DOWNSTREAM: &str = "nika: t\nsecrets:\n  tok: { source: env, key: T }\ntasks:\n  use:\n    exec: { command: [\"echo\", \"${{ tasks.up.output }}\"] }\n";
        let wf = parse(DOWNSTREAM);
        let ctx = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("hunter2-secret"))]),
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        );
        let vars = BTreeMap::new();
        let leaked = BTreeMap::from([(
            "up".to_owned(),
            success_record(json!("prefix hunter2-secret suffix")),
        )]);
        assert!(
            stamp(&wf.tasks[0].value, &leaked, &vars, &BTreeMap::new(), &ctx,).is_none(),
            "a secret value in the rendered inputs → not resume-eligible"
        );
        let clean = BTreeMap::from([("up".to_owned(), success_record(json!("safe")))]);
        assert!(
            stamp(&wf.tasks[0].value, &clean, &vars, &BTreeMap::new(), &ctx,).is_some(),
            "the same task without the leak stays eligible"
        );
    }

    /// Trap 5 — map order + serializer whitespace never reach the hash:
    /// two `with:` blocks with the same entries in different authored
    /// order produce the SAME stamp (JCS sorts keys at every depth).
    #[test]
    fn with_declaration_order_is_canonicalized_away() {
        const AB: &str = "nika: t\ntasks:\n  t:\n    with: { a: \"1\", b: \"2\" }\n    exec: { command: [\"echo\", \"${{ with.a }}\", \"${{ with.b }}\"] }\n";
        const BA: &str = "nika: t\ntasks:\n  t:\n    with: { b: \"2\", a: \"1\" }\n    exec: { command: [\"echo\", \"${{ with.a }}\", \"${{ with.b }}\"] }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let a = stamp_of(AB, &records, &vars).expect("eligible");
        let b = stamp_of(BA, &records, &vars).expect("eligible");
        assert_eq!(a, b, "authored map order is not behavior");
    }

    /// The ES6-double trap: two int64 values beyond 2^53 that a plain
    /// RFC 8785 number serialization would COLLAPSE must produce distinct
    /// input hashes (the number pre-fold carries full i64 fidelity).
    #[test]
    fn int64_beyond_2p53_never_collide() {
        const WF: &str = "nika: t\ninputs:\n  id: { type: integer, required: false, default: 1 }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"${{ inputs.id }}\"] }\n";
        let records = BTreeMap::new();
        // 2^53 + 1 and 2^53 + 2 are the SAME f64 — distinct i64s.
        let a_vars = BTreeMap::from([("id".to_owned(), json!(9_007_199_254_740_993_i64))]);
        let b_vars = BTreeMap::from([("id".to_owned(), json!(9_007_199_254_740_994_i64))]);
        let a = stamp_of(WF, &records, &a_vars).expect("eligible");
        let b = stamp_of(WF, &records, &b_vars).expect("eligible");
        assert_ne!(a.input_hash, b.input_hash, "int64 fidelity survives JCS");
    }

    /// No float FIELDS in the key: `temperature:` rides as its display
    /// string and still distinguishes values.
    #[test]
    fn temperature_rides_as_a_string_and_distinguishes() {
        const T7: &str = "nika: t\ntasks:\n  t:\n    infer: { prompt: \"x\", temperature: 0.7 }\n";
        let t8 = T7.replace("0.7", "0.8");
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let a = stamp_of(T7, &records, &vars).expect("eligible");
        let b = stamp_of(&t8, &records, &vars).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash);
    }

    /// The fan-out collection participates in the input hash (a changed
    /// item set re-runs). Deep `item.field` navigation is eligible: the
    /// stand-in is shaped from the collection so the render cannot miss,
    /// and the real values ride in `items` (never a wrong skip).
    #[test]
    fn fan_out_collection_participates_and_deep_item_nav_is_eligible() {
        const SHALLOW: &str = "nika: t\nconst:\n  urls: [\"a\", \"b\"]\ntasks:\n  fan:\n    for_each: { items: \"${{ const.urls }}\" }\n    exec: { command: [\"echo\", \"${{ item }}\"] }\n";
        const DEEP: &str = "nika: t\nconst:\n  rows: [{ url: \"a\" }]\ntasks:\n  fan:\n    for_each: { items: \"${{ const.rows }}\" }\n    exec: { command: [\"echo\", \"${{ item.url }}\"] }\n";
        let records = BTreeMap::new();
        let ab = BTreeMap::from([("urls".to_owned(), json!(["a", "b"]))]);
        let ac = BTreeMap::from([("urls".to_owned(), json!(["a", "c"]))]);
        let a = stamp_const_of(SHALLOW, &records, &ab).expect("shallow item is eligible");
        let b = stamp_const_of(SHALLOW, &records, &ac).expect("shallow item is eligible");
        assert_ne!(a.input_hash, b.input_hash, "the item set is an input");

        let rows_a = BTreeMap::from([("rows".to_owned(), json!([{ "url": "a" }]))]);
        let rows_b = BTreeMap::from([("rows".to_owned(), json!([{ "url": "b" }]))]);
        let deep_a = stamp_const_of(DEEP, &records, &rows_a).expect("item.field is eligible");
        let deep_a2 = stamp_const_of(DEEP, &records, &rows_a).expect("stable");
        let deep_b = stamp_const_of(DEEP, &records, &rows_b).expect("item.field is eligible");
        assert_eq!(deep_a, deep_a2, "same collection → same stamp");
        assert_eq!(
            deep_a.def_hash, deep_b.def_hash,
            "the task text is unchanged"
        );
        assert_ne!(
            deep_a.input_hash, deep_b.input_hash,
            "a changed item field re-keys the fan"
        );
    }

    /// The recipe version participates: a bumped `KEY_VERSION` re-keys
    /// everything (old traces re-run · honest · never a wrong match).
    #[test]
    fn key_version_participates_in_both_hashes() {
        let base = ResumeKey::new("t".into(), "exec".into(), json!({}), json!({}));
        let mut bumped = base.clone();
        bumped.v = KEY_VERSION + 1;
        assert_ne!(base.definition_hash(), bumped.definition_hash());
        assert_ne!(base.input_hash(), bumped.input_hash());
    }

    /// The number fold is total + structural (arrays/objects recurse).
    #[test]
    fn fold_numbers_tags_every_number_at_every_depth() {
        let folded = fold_numbers(&json!({ "a": [1, { "b": 2.5 }], "c": "s" }));
        assert_eq!(
            folded,
            json!({
                "a": [format!("{MARK}num:1{MARK}"), { "b": format!("{MARK}num:2.5{MARK}") }],
                "c": "s"
            })
        );
    }

    /// Spec 14 law 10 (`def_hash` tier) · a `workflow:` call's DEFINITION
    /// identity covers the composer-resolved child closure digest: a
    /// changed digest re-keys the call (an edited child re-runs) · the
    /// same digest is stable · a MISSING entry makes the task
    /// non-eligible (never a wrong skip) · a `tool:` invoke never
    /// consults the map.
    #[test]
    fn child_closure_rekeys_the_call_and_absence_disqualifies() {
        const CALLER: &str = "nika: t\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika.yaml\", args: { name: \"x\" } }\n";
        const TOOL: &str = "nika: t\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: \"confirm\", message: \"go?\", default: true } }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let stamp_with = |yaml: &str, closures: &BTreeMap<String, String>| {
            let wf = parse(yaml);
            let ctx = ResumeContext::of(
                &wf,
                &BTreeMap::new(),
                None,
                &BTreeMap::new(),
                closures,
                None,
            );
            stamp(&wf.tasks[0].value, &records, &vars, &BTreeMap::new(), &ctx)
        };
        let d1 = BTreeMap::from([("./child.nika.yaml".to_owned(), "digest-one".to_owned())]);
        let d2 = BTreeMap::from([("./child.nika.yaml".to_owned(), "digest-two".to_owned())]);

        let a = stamp_with(CALLER, &d1).expect("eligible");
        let b = stamp_with(CALLER, &d2).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash, "an edited child re-keys the call");

        let a2 = stamp_with(CALLER, &d1).expect("eligible");
        assert_eq!(a, a2, "unchanged closure → same stamp");

        assert!(
            stamp_with(CALLER, &BTreeMap::new()).is_none(),
            "no closure entry → not eligible (re-runs, never wrong-skips)"
        );

        // A tool invoke is untouched by the map (same stamp either way).
        let t1 = stamp_with(TOOL, &BTreeMap::new()).expect("eligible");
        let t2 = stamp_with(TOOL, &d1).expect("eligible");
        assert_eq!(t1, t2, "tool invokes never consult the closure map");
    }
}

/// The trace-carry proof: a REAL run through the runtime (mock seams)
/// stamps `def_hash` + `input_hash` + `output` onto every success
/// `task_completed` frame — the artifact `--resume` later reads.
mod trace_carry {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nika_event::EventKind;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_types::resource::Value as FieldValue;
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;
    use serde_json::Value;

    use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

    fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
        event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
            if let FieldValue::String(s) = &kv.value {
                Some(s.as_str())
            } else {
                None
            }
        })
    }

    #[tokio::test]
    async fn success_task_completed_carries_the_resume_fields() {
        const WORKFLOW: &str = "nika: carry\npermits: { exec: [\"echo\"] }\ntasks:\n  say:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let wf = nika_schema::parse(
            WORKFLOW,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean());

        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("said\n"))),
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
        assert!(outcome.ok);

        let completed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted)
            .expect("a task_completed frame");
        let def = str_field(completed, crate::resume::fields::DEF_HASH).expect("def_hash rides");
        let input =
            str_field(completed, crate::resume::fields::INPUT_HASH).expect("input_hash rides");
        assert_eq!(def.len(), 64, "blake3 hex");
        assert_eq!(input.len(), 64, "blake3 hex");
        // The output field parses back to EXACTLY the record's output —
        // the rehydration contract.
        let output_text =
            str_field(completed, crate::resume::fields::OUTPUT).expect("output rides");
        let rehydrated: Value = serde_json::from_str(output_text).expect("output is JSON");
        assert_eq!(rehydrated, outcome.records["say"].output);

        // Recompute the stamp from the same coordinates — it matches the
        // journaled fields (the skip predicate `--resume` evaluates).
        let ctx = crate::resume::ResumeContext::of(
            &wf,
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        );
        let stamp = crate::resume::stamp(
            &wf.tasks[0].value,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &ctx,
        )
        .expect("eligible");
        assert_eq!(stamp.def_hash, def);
        assert_eq!(stamp.input_hash, input);
    }

    const TWO_TASKS: &str = "nika: resume\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"one\"] }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"two\", \"${{ with.prev }}\"] }\n";

    /// Run [`TWO_TASKS`] over mock seams with an optional resume plan —
    /// returns the outcome + the emitted stream.
    async fn run_two_tasks(
        shell: MockShell,
        plan: Option<crate::resume::ResumePlan>,
    ) -> (crate::RunOutcome, VecSink) {
        let wf = nika_schema::parse(
            TWO_TASKS,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean());
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let mut runtime = Runtime::new(
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
        );
        if let Some(plan) = plan {
            runtime = runtime.with_resume_plan(plan);
        }
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        (outcome, sink)
    }

    /// Run an arbitrary workflow YAML over mock seams with an optional resume
    /// plan — the generic twin of [`run_two_tasks`] (used by the semantic-
    /// cache-hit proof, which needs two DIFFERENT spellings).
    async fn run_yaml(
        yaml: &str,
        shell: MockShell,
        plan: Option<crate::resume::ResumePlan>,
    ) -> (crate::RunOutcome, VecSink) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean());
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let mut runtime = Runtime::new(
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
        );
        if let Some(plan) = plan {
            runtime = runtime.with_resume_plan(plan);
        }
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        (outcome, sink)
    }

    /// **The semantic-cache HIT** (spec 15 · unblocks 14 §law 10's `owed`):
    /// a result computed for ONE spelling is REUSED for a DIFFERENT,
    /// semantically-equal spelling — the reuse is keyed on the canonical
    /// semantic identity (the `def_hash`/`input_hash` that JCS-canonicalizes
    /// authored `with:` order away · the W6 semantic hash generalizes this),
    /// NEVER on the source bytes. This is a genuine PROVEN reuse: the second
    /// run's shell has NO response queued for `a`, so a cache MISS would
    /// starve it — the green run is the demonstration that `a` never
    /// dispatched, its prior result served on semantic identity alone.
    #[tokio::test]
    async fn a_result_is_reused_across_a_semantically_equal_respelling() {
        // Two spellings that MEAN the same workflow: `with:` map order is not
        // behavior (spec 15 · proven canonical in `with_declaration_order_is_
        // canonicalized_away`). `b` is a live downstream so the run is not
        // trivially all-cache-hit — `a`'s reuse is the claim under test.
        const SPELL_A: &str = "nika: sem\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    with: { x: \"1\", y: \"2\" }\n    exec: { command: [\"echo\", \"${{ with.x }}${{ with.y }}\"] }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"done\", \"${{ with.prev }}\"] }\n";
        const SPELL_B: &str = "nika: sem\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    with: { y: \"2\", x: \"1\" }\n    exec: { command: [\"echo\", \"${{ with.x }}${{ with.y }}\"] }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"done\", \"${{ with.prev }}\"] }\n";

        // 1. Run spelling A — harvest a's journaled semantic identity + output.
        let (first, sink) = run_yaml(
            SPELL_A,
            MockShell::new().enqueue_ok("12\n").enqueue_ok("done 12\n"),
            None,
        )
        .await;
        assert!(first.ok);
        assert!(first.cache_hits.is_empty(), "a fresh run never cache-hits");
        let completed_a = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("a"))
            .expect("a completed");
        let plan = crate::resume::ResumePlan::from([(
            "a".to_owned(),
            crate::resume::PriorSuccess::new(
                str_field(completed_a, crate::resume::fields::DEF_HASH)
                    .expect("def_hash")
                    .to_owned(),
                str_field(completed_a, crate::resume::fields::INPUT_HASH)
                    .expect("input_hash")
                    .to_owned(),
                serde_json::from_str(
                    str_field(completed_a, crate::resume::fields::OUTPUT).expect("output"),
                )
                .expect("output parses"),
            ),
        )]);

        // 2. Resume the OTHER spelling with A's plan. Only b's response is
        //    queued — a MUST cache-hit on semantic identity, never dispatch.
        let (resumed, sink) = run_yaml(
            SPELL_B,
            MockShell::new().enqueue_ok("done 12\n"),
            Some(plan),
        )
        .await;
        assert!(resumed.ok);
        assert_eq!(
            resumed.cache_hits,
            vec!["a".to_owned()],
            "the respelled task reuses the prior result on semantic identity"
        );
        let kinds_for = |task: &str| {
            sink.events()
                .iter()
                .filter(|e| str_field(e, "task") == Some(task))
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        };
        assert!(
            kinds_for("a").contains(&EventKind::TaskCacheHit),
            "the reuse is VISIBLE (task_cache_hit)"
        );
        assert!(
            !kinds_for("a").contains(&EventKind::TaskStarted),
            "a never re-executed — the different spelling did not defeat the cache"
        );
        assert!(
            kinds_for("b").contains(&EventKind::TaskStarted),
            "b ran live"
        );
        // Rehydration parity: the reused output equals the first run's.
        assert_eq!(resumed.records["a"].output, first.records["a"].output);
    }

    /// The full ADR-099 fold: run → read the journaled identity → resume
    /// with a plan → the matched task CACHE-HITS (visible `task_cache_hit`
    /// · rehydrated output · no `task_started`) and the rest runs live.
    #[tokio::test]
    async fn resume_plan_skips_matching_tasks_and_runs_the_rest() {
        // First run — both tasks execute; harvest a's journaled identity.
        let (first, sink) = run_two_tasks(
            MockShell::new().enqueue_ok("one\n").enqueue_ok("two\n"),
            None,
        )
        .await;
        assert!(first.ok);
        assert!(first.cache_hits.is_empty(), "a fresh run never cache-hits");
        let completed_a = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("a"))
            .expect("a completed");
        let plan = crate::resume::ResumePlan::from([(
            "a".to_owned(),
            crate::resume::PriorSuccess::new(
                str_field(completed_a, crate::resume::fields::DEF_HASH)
                    .expect("def_hash")
                    .to_owned(),
                str_field(completed_a, crate::resume::fields::INPUT_HASH)
                    .expect("input_hash")
                    .to_owned(),
                serde_json::from_str(
                    str_field(completed_a, crate::resume::fields::OUTPUT).expect("output"),
                )
                .expect("output parses"),
            ),
        )]);

        // Resume — ONLY b's shell response is queued: a must never dispatch.
        let (resumed, sink) =
            run_two_tasks(MockShell::new().enqueue_ok("two\n"), Some(plan.clone())).await;
        assert!(resumed.ok);
        assert_eq!(resumed.cache_hits, vec!["a".to_owned()]);
        let kinds_for = |task: &str| {
            sink.events()
                .iter()
                .filter(|e| str_field(e, "task") == Some(task))
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        };
        assert!(
            kinds_for("a").contains(&EventKind::TaskCacheHit),
            "the skip is VISIBLE"
        );
        assert!(
            !kinds_for("a").contains(&EventKind::TaskStarted),
            "a never re-executed"
        );
        assert!(
            kinds_for("b").contains(&EventKind::TaskStarted),
            "b ran live"
        );
        // Rehydration parity: a's record output matches the first run's.
        assert_eq!(resumed.records["a"].output, first.records["a"].output);

        // A stale identity (input hash mismatch) refuses the skip — both
        // hashes MUST match (ADR-099 §1).
        let mut stale = plan;
        if let Some(entry) = stale.get_mut("a") {
            entry.input_hash = "0".repeat(64);
        }
        let (rerun, _) = run_two_tasks(
            MockShell::new().enqueue_ok("one\n").enqueue_ok("two\n"),
            Some(stale),
        )
        .await;
        assert!(
            rerun.cache_hits.is_empty(),
            "hash mismatch → re-run, never skip"
        );
    }

    /// A `for_each` body that navigates `item.field` stamps and
    /// cache-hits on `--resume` — the paid-replay class (a prompt of
    /// `${{ item.stem }}` used to drop the key, so a later `--from`
    /// downstream re-ran every infer). The collection is the identity.
    #[tokio::test]
    async fn for_each_item_field_resume_cache_hits() {
        const FAN: &str = "nika: fan\nconst:\n  rows:\n    - { url: a }\n    - { url: b }\npermits: { exec: [\"echo\"] }\ntasks:\n  fan:\n    for_each: { items: \"${{ const.rows }}\" }\n    exec: { command: [\"echo\", \"${{ item.url }}\"] }\n  after:\n    with: { prev: \"${{ tasks.fan.output }}\" }\n    exec: { command: [\"echo\", \"done\"] }\n";
        let (first, sink) = run_yaml(
            FAN,
            MockShell::new()
                .enqueue_ok("a\n")
                .enqueue_ok("b\n")
                .enqueue_ok("done\n"),
            None,
        )
        .await;
        assert!(first.ok, "fresh fan run");
        assert!(first.cache_hits.is_empty());
        let completed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("fan"))
            .expect("fan completed");
        let def = str_field(completed, crate::resume::fields::DEF_HASH)
            .expect("item.field fan stamps def_hash");
        let input = str_field(completed, crate::resume::fields::INPUT_HASH)
            .expect("item.field fan stamps input_hash");
        let plan = crate::resume::ResumePlan::from([(
            "fan".to_owned(),
            crate::resume::PriorSuccess::new(
                def.to_owned(),
                input.to_owned(),
                serde_json::from_str(
                    str_field(completed, crate::resume::fields::OUTPUT).expect("output"),
                )
                .expect("output parses"),
            ),
        )]);
        let (resumed, sink) =
            run_yaml(FAN, MockShell::new().enqueue_ok("done\n"), Some(plan)).await;
        assert!(resumed.ok);
        assert_eq!(
            resumed.cache_hits,
            vec!["fan".to_owned()],
            "the item.field fan reuses the prior wave"
        );
        let kinds: Vec<_> = sink
            .events()
            .iter()
            .filter(|e| str_field(e, "task") == Some("fan"))
            .map(|e| e.kind)
            .collect();
        assert!(kinds.contains(&EventKind::TaskCacheHit));
        assert!(!kinds.contains(&EventKind::TaskStarted));
        assert_eq!(resumed.records["fan"].output, first.records["fan"].output);
    }

    /// `referenced_upstreams` sees BOTH edge kinds — the boundary
    /// (`with:` refs · `after:` targets) and raw `${{ tasks.<id> }}`
    /// template text (the `--from` transitive-downstream walk rides it).
    #[test]
    fn referenced_upstreams_collects_explicit_and_template_edges() {
        let wf = nika_schema::parse(
            TWO_TASKS,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let ups = crate::resume::referenced_upstreams(&wf.tasks[1].value);
        assert!(ups.contains("a"), "boundary edge + template ref both seen");
        assert!(crate::resume::referenced_upstreams(&wf.tasks[0].value).is_empty());
    }
}
