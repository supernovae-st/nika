// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    reason = "required fixture and finding assertions"
)]

//! `nika:remove_file` at check: a removal is a mutation of its exact path,
//! so two mutations of one static key must be ordered by the precedence graph
//! (`with:` refs ∪ group folds ∪ `after:`, transitively) or refuse
//! `NIKA-SEC-012`; an `unwind` edge orders nothing. The static argument shape
//! and the authority laws apply before any run. Every fixture is parsed and
//! checked in memory: no path is opened, created or removed.

use nika_check::{CheckReport, analyze, check};
use nika_schema::{FileId, ParseMode, parse};

const PERMITS: &str = "permits:\n  tools: [\"nika:write\", \"nika:edit\", \"nika:log\", \
                       \"nika:remove_file\"]\n  fs: { write: [\"./out/**\"] }\n";

fn report(source: &str) -> CheckReport {
    check(&parse(source, FileId::new(0), ParseMode::Strict).expect("a parseable fixture"))
}

/// Every invalidating code, as the conformance harness joins them: the
/// `analyze()` tier (the static argument shape rides it) plus the check-only
/// surfaces (authority · races).
fn codes(source: &str) -> Vec<String> {
    let wf = parse(source, FileId::new(0), ParseMode::Strict).expect("a parseable fixture");
    let mut found: Vec<String> = analyze(&wf)
        .err()
        .unwrap_or_default()
        .iter()
        .map(|e| e.spec_code().to_string())
        .collect();
    found.extend(
        check(&wf)
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string),
    );
    found
}

/// A workflow with the shared permits and the given task block.
fn wf(tasks: &str) -> String {
    format!("nika: removal\n{PERMITS}tasks:\n{tasks}")
}

fn put(id: &str, path: &str, extra: &str) -> String {
    format!(
        "  {id}:\n{extra}    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{path}\", \
         content: \"draft\" }}\n"
    )
}

fn drop(id: &str, path: &str, extra: &str) -> String {
    format!(
        "  {id}:\n{extra}    invoke:\n      tool: \"nika:remove_file\"\n      args: {{ path: \"{path}\" }}\n"
    )
}

fn assert_ordered(source: &str) {
    let report = report(source);
    assert!(
        report.write_conflicts.is_empty(),
        "{:?}",
        report.write_conflicts
    );
    assert!(
        report.is_clean(),
        "{source}\n{:?}",
        report.extra_conformance_codes()
    );
}

fn assert_race(source: &str) {
    let report = report(source);
    assert!(
        !report.write_conflicts.is_empty(),
        "a race refuses:\n{source}"
    );
    assert!(
        codes(source).iter().any(|c| c == "NIKA-SEC-012"),
        "{source}"
    );
}

/// Spec `stdlib/builtins/057`: put → (group fold) → stamp → (with) → drop,
/// two spellings of one lexical key, no direct edge: transitive order holds.
const SPEC_057: &str = r#"nika: remove-file-ordered-mutations
permits:
  tools: ["nika:write", "nika:log", "nika:remove_file"]
  fs: { write: ["./out/**"] }
tasks:
  put:
    group: staged
    invoke:
      tool: "nika:write"
      args: { path: "./out/note.txt", content: "draft" }
  stamp:
    with: { legs: "${{ group.staged }}" }
    invoke:
      tool: "nika:log"
      args: { level: info, message: "staged ${{ with.legs }}" }
  drop:
    with: { logged: "${{ tasks.stamp.output }}" }
    invoke:
      tool: "nika:remove_file"
      args: { path: "out/note.txt" }
"#;

/// Spec `stdlib/builtins/067`: the removal hangs off the write as an `unwind`
/// cleanup; unwind never schedules, the two mutations stay incomparable.
const SPEC_067: &str = r#"nika: remove-file-unwind-is-not-order
permits:
  tools: ["nika:write", "nika:remove_file"]
  fs: { write: ["./out/**"] }
tasks:
  put:
    invoke:
      tool: "nika:write"
      args: { path: "./out/note", content: "draft" }
  drop:
    after: { put: unwind }
    invoke:
      tool: "nika:remove_file"
      args: { path: "./out/note" }
"#;

#[test]
fn spec_057_a_transitive_group_then_with_route_orders_the_removal() {
    assert_ordered(SPEC_057);
}

#[test]
fn spec_067_an_unwind_edge_is_not_precedence() {
    assert_race(SPEC_067);
    let race = &report(SPEC_067).write_conflicts[0];
    assert_eq!(
        (race.task.as_str(), race.other.as_deref()),
        ("put", Some("drop"))
    );
}

#[test]
fn a_direct_after_or_with_edge_orders_a_write_and_a_removal() {
    let after = "    after: { put: success }\n";
    assert_ordered(&wf(
        &(put("put", "out/note", "") + &drop("drop", "out/note", after))
    ));
    let with = "    with: { done: \"${{ tasks.put.output }}\" }\n";
    assert_ordered(&wf(
        &(put("put", "out/note", "") + &drop("drop", "out/note", with))
    ));
    let terminal = "    after: { put: terminal }\n";
    assert_ordered(&wf(
        &(put("put", "out/note", "") + &drop("drop", "out/note", terminal))
    ));
}

#[test]
fn unordered_mutations_of_one_key_race() {
    assert_race(&wf(
        &(put("put", "out/note", "") + &drop("drop", "out/note", ""))
    ));
    assert_race(&wf(
        &(drop("one", "out/note", "") + &drop("two", "out/note", ""))
    ));
    let edit = "  fix:\n    invoke:\n      tool: \"nika:edit\"\n      args: { path: \"out/note\", \
                find: a, replace: b }\n";
    assert_race(&wf(&(edit.to_owned() + &drop("drop", "out/note", ""))));
}

#[test]
fn lexical_spellings_of_one_path_are_one_key() {
    for (a, b) in [
        ("./out/note", "out/note"),
        ("out//note", "out/note"),
        ("out/d/../note", "out/note"),
        ("out/./note", "./out/note"),
    ] {
        assert_race(&wf(&(put("put", a, "") + &drop("drop", b, ""))));
    }
}

#[test]
fn distinct_or_dynamic_paths_make_no_static_claim() {
    assert_ordered(&wf(&(put("put", "out/a", "") + &drop("drop", "out/b", ""))));
    let fan = "    for_each: { items: [1, 2] }\n";
    assert_ordered(&wf(&drop("fan", "out/${{ index }}.txt", fan)));
    assert_ordered(&wf(&drop("fan", "out/${{ item }}.txt", fan)));
}

#[test]
fn const_and_input_default_resolve_into_the_same_key() {
    let consts = format!(
        "nika: removal\nconst:\n  victim: out/note\n{PERMITS}tasks:\n{}{}",
        put("put", "out/note", ""),
        drop("drop", "${{ const.victim }}", "")
    );
    assert_race(&consts);
    let inputs = format!(
        "nika: removal\ninputs:\n  victim: {{ type: string, default: out/note }}\n{PERMITS}tasks:\n{}{}",
        drop("one", "${{ inputs.victim }}", ""),
        drop("two", "${{ inputs.victim }}", "")
    );
    assert_race(&inputs);
}

#[test]
fn a_constant_removal_fan_races_its_own_iterations() {
    let fan = "    for_each: { items: [1, 2, 3] }\n";
    let source = wf(&drop("fan", "out/note", fan));
    assert_race(&source);
    let race = &report(&source).write_conflicts[0];
    assert_eq!((race.task.as_str(), race.other.as_deref()), ("fan", None));
}

#[test]
fn a_quoted_reference_is_text_not_an_edge() {
    let quoted = "    with: { label: \"tasks.put.output\" }\n";
    assert_race(&wf(
        &(put("put", "out/note", "") + &drop("drop", "out/note", quoted))
    ));
}

#[test]
fn the_static_shape_and_authority_laws_refuse_before_any_run() {
    for (args, why) in [
        (
            "{ path: \"out/\" }",
            "a trailing separator names a directory",
        ),
        ("{ path: \"out/..\" }", "a final dot-dot names a directory"),
        ("{ path: \"\" }", "an empty path names nothing"),
        ("{ path: 7 }", "a path is a string"),
        (
            "{ path: \"out/note\", recursive: true }",
            "no option beside the path",
        ),
        ("{}", "the path is required"),
    ] {
        let source = wf(&format!(
            "  drop:\n    invoke:\n      tool: \"nika:remove_file\"\n      args: {args}\n"
        ));
        let found = codes(&source);
        assert!(
            found.iter().any(|c| c == "NIKA-BUILTIN-001"),
            "{why}: {found:?}"
        );
    }
    let no_permits = format!("nika: removal\ntasks:\n{}", drop("drop", "out/note", ""));
    assert!(codes(&no_permits).iter().any(|c| c == "NIKA-AUTH-006"));
    let outside = wf(&drop("drop", "./elsewhere/note", ""));
    assert!(
        !report(&outside).is_clean(),
        "outside the write bound refuses"
    );
    let retired =
        wf("  drop:\n    invoke:\n      tool: \"nika:delete\"\n      args: { path: out/note }\n");
    assert!(
        !report(&retired).is_clean(),
        "`nika:delete` stays an unknown tool"
    );
}

/// One removal under a declared bound, with optional extra envelope keys.
fn bounded(extra: &str, bound: &str, path: &str) -> String {
    format!(
        "nika: removal\n{extra}permits:\n  tools: [\"nika:remove_file\"]\n  fs: {{ write: \
         [\"{bound}\"] }}\ntasks:\n{}",
        drop("drop", path, "")
    )
}

#[test]
fn a_statically_known_escape_of_the_write_bound_is_an_authority_refusal() {
    let const_escape = "const:\n  victim: \"../outside/note\"\n";
    for (source, why) in [
        (
            bounded(const_escape, "./out/**", "${{ const['victim'] }}"),
            "a bare indexed const is a check-time value",
        ),
        (
            bounded("", "./**", "/outside/note"),
            "a relative bound and an absolute path are different trees",
        ),
        (
            bounded("", "out/*.txt", "out/sub/note.txt"),
            "a single star stays inside one segment",
        ),
        (
            bounded("", "out/?.txt", "out/a.txt"),
            "a question mark is a literal character",
        ),
    ] {
        let found = codes(&source);
        assert!(
            found.iter().any(|c| c == "NIKA-SEC-004"),
            "{why}: {found:?}"
        );
    }
    let literal = bounded("", "out/?.txt", "out/?.txt");
    assert!(
        codes(&literal).is_empty(),
        "the literal name is inside its own bound"
    );
}

/// A removal whose path is the caller-replaceable `inputs.target` under the
/// write bound `./out/**` (the Spec authority 033/034 shape), with `typed`
/// as the input declaration.
fn untrusted_target(typed: &str) -> String {
    format!(
        "nika: removal\ninputs:\n  target: {typed}\npermits:\n  tools: [\"nika:remove_file\"]\n  \
         fs: {{ write: [\"./out/**\"] }}\ntasks:\n{}",
        drop("drop", "${{ inputs.target }}", "")
    )
}

#[test]
fn an_untrusted_default_escaping_the_write_bound_is_a_regate_refusal() {
    let source = untrusted_target("{ type: string, default: \"../../outside/note\" }");
    let report = report(&source);
    let taint = report
        .permit_taints
        .iter()
        .find(|t| t.task == "drop")
        .expect("the removal's untrusted path is re-gated at check");
    assert_eq!(taint.wire_code(), "NIKA-AUTH-008", "{taint:?}");
    assert!(
        taint.detail.contains("args.path (nika:remove_file)") && taint.detail.contains("fs.write"),
        "judged on the removal's path in the WRITE direction: {}",
        taint.detail
    );
    assert!(codes(&source).iter().any(|c| c == "NIKA-AUTH-008"));
}

#[test]
fn an_untrusted_default_inside_the_write_bound_stays_valid_without_a_read_grant() {
    let source = untrusted_target("{ type: string, default: \"out/part/../note\" }");
    let report = report(&source);
    assert!(
        report.permit_taints.is_empty(),
        "{:?}",
        report.permit_taints
    );
    assert!(codes(&source).is_empty(), "{:?}", codes(&source));
}

#[test]
fn an_untrusted_path_without_a_default_defers_to_the_runtime() {
    let source = untrusted_target("{ type: string }");
    let report = report(&source);
    assert!(
        report.permit_taints.is_empty(),
        "{:?}",
        report.permit_taints
    );
    assert!(codes(&source).is_empty(), "{:?}", codes(&source));
}
