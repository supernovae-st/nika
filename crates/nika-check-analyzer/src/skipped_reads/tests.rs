// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The skipped-read walk, judged through the real parser and analyzer. Each
//! runtime claim a test leans on was measured on `nika run` (NIK-156): a
//! binding · a `when:` · a `for_each:` raising on null fail the task even
//! under `on_error: recover`, a verb field raising on null is recovered.

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

/// The sites of a whole workflow, through the real analyzer.
fn sites_of(yaml: &str) -> Vec<Site> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let analyzed = crate::analyze(&wf).expect("fixture analyzes");
    scan(&wf, &analyzed.edges, &analyzed.topo_waves)
}

/// The sites of `tasks` (entries at two spaces) after the producer
/// `maybe`, whose gate lines are `gate`. `inputs.flag` is a value no
/// producer's gate reads: a guard on it stays undecided.
fn sites(gate: &str, tasks: &str) -> Vec<Site> {
    sites_of(&format!(
        "nika: t\ninputs:\n  go:\n    type: bool\n    default: false\n  flag:\n    type: bool\n    default: false\npermits:\n  tools: [nika:jq]\ntasks:\n  maybe:\n{gate}    invoke:\n      tool: nika:jq\n      args:\n        input: {{}}\n        expression: '{{url: \"https://example.com\"}}'\n{tasks}"
    ))
}

const CLOSED: &str = "    when: false\n";
const OPEN_ON_INPUT: &str = "    when: ${{ inputs.go == true }}\n";

/// A task `id` with `body` (its `with:` · `when:` · `after:` lines) and an
/// `invoke:` whose `input:` is `input`.
fn task(id: &str, body: &str, input: &str) -> String {
    format!(
        "  {id}:\n{body}    invoke:\n      tool: nika:jq\n      args:\n        input: {input}\n        expression: '.'\n"
    )
}

/// The reader every test names `r`.
fn reader(body: &str, input: &str) -> String {
    task("r", body, input)
}

/// The exact NIK-156 file: a deep read in a binding of a producer whose
/// `when:` is `false` — `check` said « run ready ✔ », the run died on
/// `NIKA-VAR-001`.
#[test]
fn the_issue_file_names_the_binding_the_producer_and_the_repair() {
    let yaml = "\
nika: skipped-ref
permits:
  tools: [nika:jq]
tasks:
  maybe:
    when: false
    invoke:
      tool: nika:jq
      args:
        input: {}
        expression: '{url: \"https://example.com\"}'
  after_maybe:
    when: ${{ with.u != null }}
    with:
      u: ${{ tasks.maybe.output.url }}
    invoke:
      tool: nika:jq
      args:
        input: {}
        expression: '\"ran\"'
";
    let found = sites_of(yaml);
    assert_eq!(found.len(), 1, "{found:?}");
    let (task, advice) = &found[0];
    assert_eq!(task, "after_maybe");
    for part in [
        "binding `with.u` of task `after_maybe`",
        "reads `tasks.maybe.output.url` beneath `tasks.maybe.output`",
        "`maybe` can be skipped (its `when:` is `false`, so it always skips)",
        "Bindings are evaluated BEFORE the task's own `when:`",
        "raises NIKA-VAR-001 and `after_maybe` fails instead of skipping",
        "`out: ${{ tasks.maybe.output }}` in `with:`",
        "`when: ${{ with.out != null }}`",
        "then `${{ with.out.url }}` in `args:`",
        "`${{ tasks.maybe.output != null ? … : null }}`",
    ] {
        assert!(advice.contains(part), "missing « {part} » in: {advice}");
    }
}

/// The repairs the row teaches are silent: the whole value bound and tested
/// in `when:` (by `!= null`, by `has()`, through a derived flag or the
/// status), then stepped into in the verb.
#[test]
fn the_whole_value_tested_in_when_then_read_in_the_verb_is_silent() {
    let whole = "    with:\n      o: ${{ tasks.maybe.output }}\n";
    for body in [
        format!("{whole}    when: ${{{{ with.o != null }}}}\n"),
        format!("{whole}    when: ${{{{ has(with.o.url) }}}}\n"),
        format!(
            "{whole}      ok: ${{{{ tasks.maybe.output != null }}}}\n    when: ${{{{ with.ok == true }}}}\n"
        ),
        format!(
            "{whole}      s: ${{{{ tasks.maybe.status }}}}\n    when: ${{{{ with.s == 'success' }}}}\n"
        ),
        format!(
            "{whole}      s: ${{{{ tasks.maybe.status }}}}\n    when: ${{{{ with.s in ['success'] }}}}\n"
        ),
    ] {
        let found = sites(OPEN_ON_INPUT, &reader(&body, "${{ with.o.url }}"));
        assert!(found.is_empty(), "{body}\n{found:?}");
    }
}

/// A binding that guards its own step is silent: `has()` on a member step, a
/// null test that short-circuits or picks the ternary branch, a status the
/// skip decides.
#[test]
fn a_binding_that_guards_its_own_step_is_silent() {
    for binding in [
        "has(tasks.maybe.output.url)",
        "has(tasks.maybe.output.url) ? tasks.maybe.output.url : null",
        "tasks.maybe.output != null ? tasks.maybe.output.url : null",
        "tasks.maybe.output == null ? null : tasks.maybe.output.url",
        "tasks.maybe.output != null && tasks.maybe.output.url != ''",
        "tasks.maybe.output == null || tasks.maybe.output.url == ''",
        "tasks.maybe.status == 'success' ? tasks.maybe.output.url : ''",
        "tasks.maybe.output in ['a']",
        "tasks.maybe.output",
    ] {
        let body = format!("    with:\n      u: \"${{{{ {binding} }}}}\"\n");
        let found = sites(OPEN_ON_INPUT, &reader(&body, "{}"));
        assert!(found.is_empty(), "{binding}\n{found:?}");
    }
}

/// A producer that cannot skip hands a value, never the null: no `when:` at
/// all, or `when: true`.
#[test]
fn a_producer_that_cannot_skip_is_silent() {
    let body = "    with:\n      u: ${{ tasks.maybe.output.url }}\n";
    for gate in ["", "    when: true\n"] {
        let found = sites(gate, &reader(body, "{}"));
        assert!(found.is_empty(), "gate {gate:?}: {found:?}");
    }
}

/// The skip itself cancels a reader that needs the producer's success,
/// directly or through a stage upstream: none of its reads runs.
#[test]
fn a_reader_the_skip_cancels_is_silent() {
    let body = "    with:\n      u: ${{ tasks.maybe.output.url }}\n";
    let direct = format!("    after: {{ maybe: success }}\n{body}");
    assert!(sites(CLOSED, &reader(&direct, "{}")).is_empty());
    let stage = task("mid", "    after: { maybe: success }\n", "{}");
    let behind = format!(
        "{stage}{}",
        reader(&format!("    after: {{ mid: success }}\n{body}"), "{}")
    );
    assert!(sites(CLOSED, &behind).is_empty(), "{behind}");
    // A `terminal` wait admits the skip: the read runs and raises.
    let observed = format!("    after: {{ maybe: terminal }}\n{body}");
    assert_eq!(sites(CLOSED, &reader(&observed, "{}")).len(), 1);
}

/// Each runtime code where it belongs: `has()` catches the member step's
/// `NIKA-VAR-001` only; an index, `size()`, a method, an ordering and a
/// boolean use raise `NIKA-VAR-006`, which goes through `has()`.
#[test]
fn every_use_of_the_null_that_raises_carries_its_code() {
    for (binding, code) in [
        ("tasks.maybe.output.url", VAR_001),
        ("tasks.maybe.output.items[0]", VAR_001),
        ("tasks.maybe.output['url']", VAR_006),
        ("has(tasks.maybe.output['url'])", VAR_006),
        ("tasks.maybe.output[0]", VAR_006),
        ("size(tasks.maybe.output)", VAR_006),
        ("tasks.maybe.output.size()", VAR_006),
        ("tasks.maybe.output.startsWith('h')", VAR_006),
        ("tasks.maybe.output > 3", VAR_006),
        ("'a' in tasks.maybe.output", VAR_006),
        ("tasks.maybe.output ? 'a' : 'b'", VAR_006),
        (
            "inputs.flag == true && tasks.maybe.output.url == 'x'",
            VAR_001,
        ),
    ] {
        let body = format!("    with:\n      u: \"${{{{ {binding} }}}}\"\n");
        let found = sites(OPEN_ON_INPUT, &reader(&body, "{}"));
        assert_eq!(found.len(), 1, "{binding}: {found:?}");
        assert!(
            found[0].1.contains(code),
            "{binding} → {code}: {}",
            found[0].1
        );
        let other = if code == VAR_001 { VAR_006 } else { VAR_001 };
        assert!(
            !found[0].1.contains(&format!("raises {other}")),
            "{binding}: {}",
            found[0].1
        );
    }
}

/// Every way a producer skips is named: a gate that can close, an empty
/// fan-out, an `on_error: skip`; a named binding reads null like the output.
#[test]
fn every_way_to_skip_is_named_and_a_named_binding_reads_null_too() {
    let body = "    with:\n      u: ${{ tasks.maybe.output.url }}\n";
    let gated = sites(OPEN_ON_INPUT, &reader(body, "{}"));
    assert!(gated[0].1.contains("(its `when:` can close)"), "{gated:?}");
    let fan = "    for_each: { items: [1, 2] }\n";
    let fanned = sites(fan, &reader(body, "{}"));
    assert!(
        fanned[0]
            .1
            .contains("its `for_each:` collection can be empty"),
        "{fanned:?}"
    );
    let skip = "    on_error: { skip: true }\n";
    let skipping = sites(skip, &reader(body, "{}"));
    assert!(
        skipping[0]
            .1
            .contains("its `on_error: skip` settles a failure as a skip"),
        "{skipping:?}"
    );
    let named = "    extract:\n      link: '.url'\n    when: ${{ inputs.go == true }}\n";
    let read = "    with:\n      u: ${{ tasks.maybe.link.host }}\n";
    let found = sites(named, &reader(read, "{}"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .1
            .contains("`tasks.maybe.link.host` beneath `tasks.maybe.link`"),
        "{}",
        found[0].1
    );
}

/// Past the bindings: a whole value stepped into by the verb, the gate or the
/// fan-out, with no `when:` that rules the null out.
#[test]
fn the_body_reads_through_a_whole_binding_unless_the_gate_rules_null_out() {
    let whole = "    with:\n      o: ${{ tasks.maybe.output }}\n";
    let bare = sites(CLOSED, &reader(whole, "${{ with.o.url }}"));
    assert_eq!(bare.len(), 1, "{bare:?}");
    for part in [
        "the `args:` of task `r` reads `with.o.url` beneath `with.o`",
        "`with.o` binds the value of `maybe`",
        "`r` has no `when:` to stop it",
        "raises NIKA-VAR-001",
        "`when: ${{ with.o != null }}`",
    ] {
        assert!(
            bare[0].1.contains(part),
            "missing « {part} »: {}",
            bare[0].1
        );
    }
    let undecided = format!("{whole}    when: ${{{{ inputs.flag == true }}}}\n");
    let open = sites(OPEN_ON_INPUT, &reader(&undecided, "${{ with.o.url }}"));
    assert!(
        open[0]
            .1
            .contains("the `when:` of `r` does not rule null out"),
        "{open:?}"
    );
    // The producer's own gate, repeated: false whenever it skipped.
    let shared = format!("{whole}    when: ${{{{ inputs.go == true }}}}\n");
    assert!(sites(OPEN_ON_INPUT, &reader(&shared, "${{ with.o.url }}")).is_empty());
    let gate_reads = format!("{whole}    when: ${{{{ with.o.url != null }}}}\n");
    let gate = sites(OPEN_ON_INPUT, &reader(&gate_reads, "{}"));
    assert_eq!(gate.len(), 1, "{gate:?}");
    assert!(
        gate[0]
            .1
            .contains("the `when:` of task `r` reads `with.o.url`"),
        "{}",
        gate[0].1
    );
    let fans = format!("{whole}    for_each: {{ items: \"${{{{ with.o }}}}\" }}\n");
    let fan = sites(OPEN_ON_INPUT, &reader(&fans, "{}"));
    assert_eq!(fan.len(), 1, "{fan:?}");
    assert!(
        fan[0]
            .1
            .contains("the `for_each:` of task `r` uses `with.o` as its collection")
            && fan[0].1.contains("NIKA-VAR-006"),
        "{}",
        fan[0].1
    );
    // A whole value handed on as it is never steps into the null.
    assert!(sites(OPEN_ON_INPUT, &reader(whole, "${{ with.o }}")).is_empty());
}

/// A verb read the task's `on_error:` covers is the author's to handle
/// (measured: `recover:` settles it a success) — unless `on_codes:` leaves
/// its code out. The gate and the collection are never recovered.
#[test]
fn a_verb_read_the_task_recovers_is_silent_and_nothing_else_is() {
    let whole = "    with:\n      o: ${{ tasks.maybe.output }}\n";
    let recovered = format!("{whole}    on_error: {{ recover: none }}\n");
    assert!(sites(CLOSED, &reader(&recovered, "${{ with.o.url }}")).is_empty());
    let filtered = format!("{whole}    on_error: {{ recover: none, on_codes: [NIKA-VAR-006] }}\n");
    assert_eq!(
        sites(CLOSED, &reader(&filtered, "${{ with.o.url }}")).len(),
        1,
        "VAR-001 is not in the filter"
    );
    let gate = format!("{recovered}    when: ${{{{ with.o.url != null }}}}\n");
    assert_eq!(sites(CLOSED, &reader(&gate, "{}")).len(), 1);
}

/// One row per producer: two skippable producers read by one binding are two
/// defects with two repairs.
#[test]
fn each_skippable_producer_gets_its_own_row() {
    let other = task("other", "    when: ${{ inputs.go == false }}\n", "{}");
    let body =
        "    with:\n      u: \"${{ tasks.maybe.output.url }}-${{ tasks.other.output.url }}\"\n";
    let found = sites(OPEN_ON_INPUT, &format!("{other}{}", reader(body, "{}")));
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].1.contains("`maybe` can be skipped"),
        "{}",
        found[0].1
    );
    assert!(
        found[1].1.contains("`other` can be skipped"),
        "{}",
        found[1].1
    );
}

/// The repeated consent guard (the release-radar shape): the producer skips
/// exactly when `with.go == true` is false, and a reader that tests the same
/// `tasks.approve.output` first — under any binding name — never reaches the
/// read. Tested in the other order, the read runs first and raises.
#[test]
fn a_guard_the_producer_shares_reads_as_its_skip() {
    let approve = task("approve", "", "{}");
    let fresh = task(
        "fresh",
        "    with:\n      go: ${{ tasks.approve.output }}\n    when: ${{ with.go == true }}\n",
        "{}",
    );
    let digest = |name: &str, gate: &str| {
        task(
            "digest",
            &format!(
                "    with:\n      {name}: ${{{{ tasks.approve.output }}}}\n      fresh: ${{{{ tasks.fresh.output }}}}\n    when: ${{{{ {gate} }}}}\n"
            ),
            "{}",
        )
    };
    let flow = |reader: &str| {
        sites_of(&format!(
            "nika: t\npermits:\n  tools: [nika:jq]\ntasks:\n{approve}{fresh}{reader}"
        ))
    };
    for (name, gate) in [
        ("go", "with.go == true && size(with.fresh) > 0"),
        ("ok", "with.ok == true && size(with.fresh) > 0"),
        ("go", "true == with.go && size(with.fresh) > 0"),
        ("go", "with.go != true || size(with.fresh) > 0"),
    ] {
        assert!(flow(&digest(name, gate)).is_empty(), "{gate}");
    }
    let late = flow(&digest("go", "size(with.fresh) > 0 && with.go == true"));
    assert_eq!(late.len(), 1, "{late:?}");
    assert!(
        late[0]
            .1
            .contains("the `when:` of task `digest` uses `size(with.fresh)`"),
        "{}",
        late[0].1
    );
}

/// A guard that only MATCHES BY NAME proves nothing: `with.go` written as a
/// rendered string is each task's own value, never the producer's fact.
#[test]
fn an_ungrounded_guard_proves_nothing() {
    let rendered = "    with:\n      go: \"v=${{ tasks.approve.output }}\"\n";
    let fresh = task(
        "fresh",
        &format!("{rendered}    when: ${{{{ with.go == 'v=true' }}}}\n"),
        "{}",
    );
    let digest = task(
        "digest",
        &format!(
            "{rendered}      fresh: ${{{{ tasks.fresh.output }}}}\n    when: ${{{{ with.go == 'v=true' && size(with.fresh) > 0 }}}}\n"
        ),
        "{}",
    );
    let found = sites_of(&format!(
        "nika: t\npermits:\n  tools: [nika:jq]\ntasks:\n{}{fresh}{digest}",
        task("approve", "", "{}")
    ));
    assert_eq!(found.len(), 1, "{found:?}");
}

/// The release-notes shape: a reader whose binding certainly raises FAILS in
/// the skipped world, so a task waiting on its success is never admitted
/// there — one row, not two. Repaired, the first reader skips instead, and
/// still nothing behind it runs; a `terminal` wait is admitted either way.
#[test]
fn a_task_waiting_on_a_failing_reader_is_not_named_twice() {
    let read = "    with:\n      h: ${{ tasks.maybe.output.url }}\n";
    let announce = |wait: &str| {
        task(
            "announce",
            &format!("{read}    after: {{ changelog: {wait} }}\n"),
            "{}",
        )
    };
    let raw = task("changelog", read, "{}");
    let found = sites(OPEN_ON_INPUT, &format!("{raw}{}", announce("success")));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "changelog");
    let repaired = task(
        "changelog",
        "    with:\n      out: ${{ tasks.maybe.output }}\n    when: ${{ with.out != null }}\n",
        "${{ with.out.url }}",
    );
    assert!(sites(OPEN_ON_INPUT, &format!("{repaired}{}", announce("success"))).is_empty());
    let both = sites(
        OPEN_ON_INPUT,
        &format!("{repaired}{}", announce("terminal")),
    );
    assert_eq!(both.len(), 1, "{both:?}");
    assert_eq!(both[0].0, "announce");
}

/// The printed read is the author's expression, parenthesized only where an
/// operator nests.
#[test]
fn a_read_prints_as_written() {
    let expr = |src: &str| nika_schema::expression::parse_expression(src).expect("parses");
    for src in [
        "tasks.maybe.output.url",
        "tasks.maybe.output['url']",
        "size(tasks.maybe.output)",
        "tasks.maybe.output.startsWith('h')",
        "has(with.o.url) && with.n > 0",
        "(a || b) && !c",
        "x ? [1, 2.5, null] : \"it's\"",
    ] {
        assert_eq!(show(&expr(src)), src);
    }
}
