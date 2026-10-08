// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Targeted revisions of rich documents through the public API, with their
//! two proofs asserted apart: the strict parser's typed reading of every
//! untouched construct, and the bytes outside the edit. Each proof has a
//! mutant only it catches, so neither stands in for the other.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use nika_schema::document::{Document, Edit, Path, Refusal};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use serde_json::json;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/document_fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("a fixture")
}

fn path(dotted: &str) -> Path {
    Path::dotted(dotted).expect("a dotted path")
}

/// The typed fields of a verb, compared through the AST's own types.
fn same_action(a: &RawAction, b: &RawAction) -> bool {
    match (a, b) {
        (RawAction::Infer(x), RawAction::Infer(y)) => {
            x.prompt == y.prompt
                && x.system == y.system
                && x.model == y.model
                && x.temperature == y.temperature
                && x.max_tokens == y.max_tokens
                && x.schema == y.schema
                && x.thinking == y.thinking
                && x.vision == y.vision
        }
        (RawAction::Exec(x), RawAction::Exec(y)) => {
            x.command.text_fragments() == y.command.text_fragments()
                && x.command.argv_program() == y.command.argv_program()
                && x.command.shell_str() == y.command.shell_str()
                && x.cwd == y.cwd
                && x.env == y.env
                && x.stdin == y.stdin
                && x.capture == y.capture
                && x.decode == y.decode
        }
        (RawAction::Invoke(x), RawAction::Invoke(y)) => {
            x.tool() == y.tool() && x.workflow() == y.workflow() && x.args == y.args
        }
        (RawAction::Agent(x), RawAction::Agent(y)) => {
            x.prompt == y.prompt
                && x.system == y.system
                && x.model == y.model
                && x.tools == y.tools
                && x.skills == y.skills
                && x.max_turns == y.max_turns
                && x.max_tokens_total == y.max_tokens_total
                && x.temperature == y.temperature
                && x.schema == y.schema
        }
        _ => false,
    }
}

/// Every field of a task, compared through the AST's own types.
fn same_task(a: &RawTask, b: &RawTask) -> bool {
    a.id == b.id
        && a.after == b.after
        && a.when == b.when
        && a.for_each == b.for_each
        && a.max_parallel == b.max_parallel
        && a.max_items == b.max_items
        && a.fail_fast == b.fail_fast
        && a.retry == b.retry
        && a.on_error == b.on_error
        && a.timeout == b.timeout
        && a.with == b.with
        && a.extract == b.extract
        && a.returns == b.returns
        && a.lift == b.lift
        && a.group == b.group
        && same_action(&a.action, &b.action)
}

/// The strict parser read `revised` as it read `base`, except the tasks in `edited`.
fn same_reading_except(base: &RawWorkflow, revised: &RawWorkflow, edited: &[&str]) -> bool {
    let task = |wf: &RawWorkflow, id: &str| {
        wf.tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .map(|t| t.value.clone())
    };
    base.workflow == revised.workflow
        && base.model == revised.model
        && base.inputs == revised.inputs
        && base.secrets == revised.secrets
        && base.run == revised.run
        && base.outputs == revised.outputs
        && base.tasks.len() == revised.tasks.len()
        && base.tasks.iter().all(|t| {
            let id = t.value.id.value.as_str();
            edited.contains(&id)
                || task(revised, id).is_some_and(|other| same_task(&t.value, &other))
        })
}

/// The bytes outside one replaced region: `revised` is `base` with
/// `base[start..end]` replaced, every other byte equal — found without the
/// engine's own splice.
fn outside_region_identical(base: &str, revised: &str, start: usize, end: usize) -> bool {
    let tail = base.len() - end;
    revised.len() >= start + tail
        && revised.as_bytes()[..start] == base.as_bytes()[..start]
        && revised.as_bytes()[revised.len() - tail..] == base.as_bytes()[end..]
}

#[test]
fn a_bound_value_changes_alone_with_meaning_and_bytes_proven_apart() {
    let base = fixture("rich-revision.nika");
    let document = Document::parse(base.clone()).expect("import");
    let at = path("const.window_hours");
    let span = document.node(&at).and_then(|n| n.span).expect("placed");
    assert_eq!(&base[span.clone()], "48");
    let applied = document
        .apply(&[Edit::set(at.clone(), json!(72))])
        .expect("48 to 72");
    let revised = applied.document();
    // Meaning: every untouched construct reads the same; the constant moved alone.
    assert!(same_reading_except(
        document.workflow(),
        revised.workflow(),
        &[]
    ));
    let mut predicted = document.literal().clone();
    *predicted.pointer_mut("/const/window_hours").unwrap() = json!(72);
    assert_eq!(revised.literal(), &predicted);
    // Bytes: everything outside the old value's span is the base's own.
    assert!(outside_region_identical(
        &base,
        revised.source(),
        span.start,
        span.end
    ));
    assert_eq!(&revised.source()[span.start..span.start + 2], "72");
    assert!(applied.bytes_preserved(&base));
}

#[test]
fn each_proof_has_a_mutant_only_it_catches() {
    let base = fixture("rich-revision.nika");
    let document = Document::parse(base.clone()).expect("import");
    let applied = document
        .apply(&[Edit::set(path("const.window_hours"), json!(72))])
        .expect("72");
    let good = applied.document().source().to_owned();
    let expected = applied.document().literal().clone();
    let span = document
        .node(&path("const.window_hours"))
        .and_then(|n| n.span)
        .expect("placed");
    let read = |text: &str| Document::parse(text.to_owned()).expect("a mutant still parses");
    // A dropped comment: the meaning is intact, only the byte proof sees it.
    let dropped = good.replacen("# header comment kept byte for byte\n", "", 1);
    let mutant = read(&dropped);
    assert_eq!(
        mutant.literal(),
        &expected,
        "the projection cannot see a comment"
    );
    assert!(same_reading_except(
        document.workflow(),
        mutant.workflow(),
        &[]
    ));
    assert!(!outside_region_identical(
        &base, &dropped, span.start, span.end
    ));
    // An explicit `required: false` dropped: the AST keeps `false`, the projection does not.
    let normalized = good.replacen("    required: false\n", "", 1);
    let mutant = read(&normalized);
    assert!(
        same_reading_except(document.workflow(), mutant.workflow(), &[]),
        "the AST reads an absent `required` as false"
    );
    assert_ne!(mutant.literal(), &expected, "the projection keeps presence");
    assert!(!outside_region_identical(
        &base,
        &normalized,
        span.start,
        span.end
    ));
    // argv rewritten as a shell string: the typed reading differs.
    let shelled = good.replacen(
        "      command: [\"echo\", \"[1, 2, 3]\"]\n",
        "      shell: \"echo [1, 2, 3]\"\n",
        1,
    );
    let mutant = read(&shelled);
    assert!(!same_reading_except(
        document.workflow(),
        mutant.workflow(),
        &[]
    ));
    assert_ne!(mutant.literal(), &expected);
}

#[test]
fn a_sampling_change_keeps_every_other_option_of_every_verb() {
    let base = fixture("verbs-full.nika");
    let document = Document::parse(base.clone()).expect("import");
    let applied = document
        .apply(&[
            Edit::set(path("tasks.research.infer.temperature"), json!(0.5)),
            Edit::set(path("tasks.fanout.for_each.max_items"), json!(12)),
        ])
        .expect("two option edits");
    let revised = applied.document();
    assert!(same_reading_except(
        document.workflow(),
        revised.workflow(),
        &["research", "fanout"]
    ));
    let task = |wf: &RawWorkflow, id: &str| {
        wf.tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .map(|t| t.value.clone())
            .expect("task")
    };
    let (before, after) = (
        task(document.workflow(), "research"),
        task(revised.workflow(), "research"),
    );
    let (RawAction::Infer(x), RawAction::Infer(y)) = (&before.action, &after.action) else {
        panic!("research infers");
    };
    assert_eq!(y.temperature.as_ref().map(|t| t.value), Some(0.5));
    assert!(x.thinking == y.thinking && x.vision == y.vision && x.schema == y.schema);
    assert!(x.system == y.system && x.prompt == y.prompt && x.max_tokens == y.max_tokens);
    let fanout = task(revised.workflow(), "fanout");
    assert_eq!(fanout.max_items.map(|m| m.value), Some(12));
    assert_eq!(fanout.max_parallel.map(|m| m.value), Some(2));
    assert_eq!(fanout.fail_fast.map(|f| f.value), Some(true));
    assert!(applied.bytes_preserved(&base));
    assert_eq!(
        revised.source().matches('#').count(),
        base.matches('#').count()
    );
}

#[test]
fn an_exact_component_is_inserted_line_for_line() {
    let base = fixture("rich-revision.nika");
    let document = Document::parse(base.clone()).expect("import");
    let component = "# reused: filter rows newer than the window\nafter: { persist: success }\ninvoke:\n  tool: \"nika:log\"   # kept\n  args: { message: \"done in ${{ const.window_hours }}h\" }\n";
    let applied = document
        .apply(&[Edit::insert_text(path("tasks"), "announce", component)])
        .expect("insert");
    let revised = applied.document().source();
    for line in component.lines() {
        assert!(
            revised.contains(&format!("    {line}\n")),
            "{line:?} kept verbatim"
        );
    }
    let after = applied.document().workflow();
    assert_eq!(after.tasks.len(), document.workflow().tasks.len() + 1);
    assert!(
        document
            .workflow()
            .tasks
            .iter()
            .all(|t| { after.tasks.iter().any(|u| same_task(&t.value, &u.value)) })
    );
    assert!(applied.bytes_preserved(&base));
    assert_eq!(applied.changed(), [path("tasks.announce")]);
}

#[test]
fn a_refused_edit_names_why_and_keeps_the_document() {
    let base = fixture("verbs-full.nika");
    let document = Document::parse(base.clone()).expect("import");
    let refusal = document
        .apply(&[Edit::set(
            path("tasks.build.exec.capture"),
            json!("everything"),
        )])
        .expect_err("a closed enum");
    assert!(matches!(refusal, Refusal::Language { .. }), "{refusal}");
    assert!(
        refusal.to_string().contains("/tasks/build/exec/capture"),
        "{refusal}"
    );
    assert_eq!(document.source(), base);
}
