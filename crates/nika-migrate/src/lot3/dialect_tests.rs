// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crate::{Lot3Outcome, lot3};

fn changed(source: &str) -> String {
    match lot3(source) {
        Lot3Outcome::Changed { source, .. } => source,
        other => panic!("expected repair, got {other:?}"),
    }
}
fn stopped(source: &str) {
    assert!(matches!(lot3(source), Lot3Outcome::Stop(_)), "{}", source);
}

#[test]
fn dead_verb_fields_repair_block_and_flow_values_only_at_grammar_positions() {
    for (before, after) in [
        (
            "    invoke:\n      tool: nika:log\n      params: {message: 'params: stays'} # note\n",
            "    invoke:\n      tool: nika:log\n      args: {message: 'params: stays'} # note\n",
        ),
        (
            "    invoke: {tool: nika:log, 'params': {message: 'a,b:#'}}\n",
            "    invoke: {tool: nika:log, 'args': {message: 'a,b:#'}}\n",
        ),
        (
            "    exec: {argv: [echo, 'hi; argv: still data']} # note\n",
            "    exec: {command: [echo, 'hi; argv: still data']} # note\n",
        ),
        (
            "    exec:\n      argv:\n        - echo\n        - 'a b'\n",
            "    exec:\n      command:\n        - echo\n        - 'a b'\n",
        ),
        (
            "    invoke:\n      params:\n        message: hi\n      tool: nika:log\n",
            "    invoke:\n      args:\n        message: hi\n      tool: nika:log\n",
        ),
    ] {
        let before = format!("nika: w\ntasks:\n  say:\n{before}");
        let expected = format!("nika: w\ntasks:\n  say:\n{after}");
        let actual = changed(&before);
        assert_eq!(actual, expected);
        assert_eq!(lot3(&actual), Lot3Outcome::Clean);
    }
    let before = "nika: w\ntasks:\n  say: {invoke: {tool: nika:log, params: {message: hi}}}\n";
    assert_eq!(changed(before), before.replace("params:", "args:"));
}

#[test]
fn grammar_lookalikes_inside_payloads_and_prompts_remain_byte_identical() {
    let source = "nika: w\nconst:\n  old: {exec: {argv: [echo, hi]}}\ntasks:\n  say:\n    invoke:\n      tool: nika:log\n      args:\n        params: keep\n        argv: keep\n        for_each: keep\n  think:\n    infer:\n      prompt: |\n        exec:\n          argv: [keep]\n";
    assert_eq!(lot3(source), Lot3Outcome::Clean);
}

#[test]
fn conflicting_verb_fields_after_dedented_comments_stop_the_whole_lot3_pass() {
    for (verb, old, new, value) in [
        ("invoke", "params", "args", "{}"),
        ("exec", "argv", "command", "[echo]"),
    ] {
        for padding in ["", "  ", "    ", "      "] {
            for (first, second) in [(old, new), (new, old)] {
                let source = format!(
                    "nika: w\ntasks:\n  earlier:\n    invoke: {{tool: nika:log, params: {{message: hi}}}}\n  conflict:\n    {verb}:\n      {first}: {value}\n{padding}# YAML ignores this indentation\n      {second}: {value}\n"
                );
                stopped(&source);
            }
        }
    }
    stopped("nika: w\ntasks:\n  a:\n    exec: {argv: [echo], shell: hi}\n");
    stopped("nika: w\ntasks:\n  a:\n    invoke: {'params': {}, \"args\": {}}\n");
}

#[test]
fn an_argv_scalar_never_becomes_an_implicit_shell() {
    for value in ["'echo hi; touch sentinel'", "echo", "*unknown", "null"] {
        stopped(&format!(
            "nika: w\ntasks:\n  a:\n    exec: {{argv: {value}}}\n"
        ));
    }
}

#[test]
fn scalar_and_list_for_each_wrap_without_knobs_and_keep_comments_and_crlf() {
    for value in ["${{ const.items }}", "[one, two]", "'${{ const.items }}'"] {
        for eol in ["\n", "\r\n"] {
            let before = format!(
                "nika: w\ntasks:\n  a:\n    for_each: {value} # keep\n    infer: {{prompt: hi}}\n"
            )
            .replace('\n', eol);
            let expected = format!("nika: w\ntasks:\n  a:\n    for_each:\n      items: {value} # keep\n    infer: {{prompt: hi}}\n").replace('\n', eol);
            let actual = changed(&before);
            assert_eq!(actual, expected);
            assert_eq!(lot3(&actual), Lot3Outcome::Clean);
        }
    }
}

#[test]
fn outer_and_inner_fanout_knobs_conflict_instead_of_creating_duplicate_keys() {
    for knob in ["max_parallel", "fail_fast"] {
        stopped(&format!(
            "nika: w\ntasks:\n  a:\n    for_each:\n      items: [one]\n    # keep mapping open\n      {knob}: 1\n    {knob}: 2\n    infer: {{prompt: hi}}\n"
        ));
    }
}

#[test]
fn unvisited_canonical_flow_maps_are_not_new_migration_failures() {
    for source in [
        "nika: w\ntasks: {say: {exec: {command: [echo, hi]}}}\n",
        "nika: w\ntasks:\n  say: {\n    exec: {command: [echo, hi]}\n  }\n",
    ] {
        assert_eq!(lot3(source), Lot3Outcome::Clean);
    }
}

#[test]
fn an_unvisited_multiline_context_is_preserved_beside_a_proven_repair() {
    let second = "  untouched: {\n    invoke: {tool: nika:log, params: {}, args: {}}\n  }\n";
    let first = "nika: w\ntasks:\n  known:\n    invoke: {tool: nika:log, params: {message: hi}}\n";
    let actual = changed(&format!("{first}{second}"));
    assert_eq!(
        actual,
        format!("{}{second}", first.replace("params:", "args:"))
    );
}

#[test]
fn escaped_or_opaque_mapping_keys_cannot_hide_a_collision() {
    for (verb, old, new, value) in [
        ("invoke", "params", r#""ar\u0067s""#, "{}"),
        ("exec", "argv", r#""comm\u0061nd""#, "[echo]"),
        ("exec", "argv", r#""sh\u0065ll""#, "[echo]"),
        ("invoke", "params", "!!str args", "{}"),
    ] {
        stopped(&format!(
            "nika: w\ntasks:\n  a:\n    {verb}: {{{old}: {value}, {new}: {value}}}\n"
        ));
        stopped(&format!(
            "nika: w\ntasks:\n  a:\n    {verb}:\n      {old}: {value}\n      {new}: {value}\n"
        ));
    }
    stopped("nika: w\ntasks:\n  a:\n    invoke:\n      params: {}\n      ? args\n      : {}\n");
    stopped(
        "nika: w\ntasks:\n  a:\n    for_each:\n      items: [one]\n      \"max_\\u0070arallel\": 1\n    max_parallel: 2\n    infer: {prompt: hi}\n",
    );
}

#[test]
fn scalar_task_and_tasks_payloads_are_not_mapping_contexts() {
    let payload = "  text: |\n    invoke:\n      tool: nika:log\n      params: {message: keep}\n";
    let source = format!("nika: w\ntasks:\n{payload}");
    assert_eq!(lot3(&source), Lot3Outcome::Clean);
    let source = format!("nika: w\ntasks: |\n{payload}");
    assert_eq!(lot3(&source), Lot3Outcome::Clean);
    let known = "  known:\n    invoke: {tool: nika:log, params: {message: repair}}\n";
    assert_eq!(
        changed(&format!("nika: w\ntasks:\n{payload}{known}")),
        format!(
            "nika: w\ntasks:\n{payload}{}",
            known.replace("params:", "args:")
        )
    );
    assert_eq!(
        lot3("nika: w\ntasks:\n  task: *unknown\n"),
        Lot3Outcome::Clean
    );
}

#[test]
fn for_each_continuations_stop_and_canonical_aliases_stay_unchanged() {
    stopped("nika: w\ntasks:\n  a:\n    for_each: hello\n      world\n    infer: {prompt: hi}\n");
    stopped(
        "nika: w\ntasks:\n  a:\n    for_each: |\n      keep the collection spelling\n    infer: {prompt: hi}\n",
    );
    let source = "nika: w\nconst: {loop: &loop {items: [one]}}\ntasks:\n  a:\n    for_each: *loop\n    infer: {prompt: hi}\n";
    assert_eq!(lot3(source), Lot3Outcome::Clean);
}

#[test]
fn inline_for_each_wraps_only_a_proven_collection_shape() {
    let source = "nika: w\ntasks:\n  a: {for_each: [one, two], infer: {prompt: hi}}\n";
    let actual = changed(source);
    assert!(actual.contains("for_each: { items: [one, two] }"));
    assert_eq!(lot3(&actual), Lot3Outcome::Clean);
    let alias = "nika: w\nconst: {loop: &loop {items: [one]}}\ntasks:\n  a: {for_each: *loop, infer: {prompt: hi}}\n";
    assert_eq!(lot3(alias), Lot3Outcome::Clean);
}

#[test]
fn comments_and_blank_lines_do_not_become_opaque_fanout_keys() {
    for eol in ["\n", "\r\n"] {
        let source = "nika: w\ntasks:\n  a:\n    for_each:\n      # keep\n      \n      items: [one]\n    max_parallel: 4 # outer\n    infer: {prompt: hi}\n".replace('\n', eol);
        let expected = "nika: w\ntasks:\n  a:\n    for_each:\n      max_parallel: 4 # outer\n      # keep\n      \n      items: [one]\n    infer: {prompt: hi}\n".replace('\n', eol);
        let actual = changed(&source);
        assert_eq!(actual, expected);
        assert_eq!(lot3(&actual), Lot3Outcome::Clean);
    }
}

#[test]
fn valid_trailing_commas_are_preserved_in_retired_verb_mappings() {
    for (before, after) in [
        (
            "invoke: {tool: nika:log, params: {message: hi},}",
            "invoke: {tool: nika:log, args: {message: hi},}",
        ),
        ("exec: {argv: [echo],}", "exec: {command: [echo],}"),
    ] {
        let source = format!("nika: w\ntasks:\n  a:\n    {before}\n");
        let actual = changed(&source);
        assert_eq!(actual, format!("nika: w\ntasks:\n  a:\n    {after}\n"));
        assert_eq!(lot3(&actual), Lot3Outcome::Clean);
    }
}
