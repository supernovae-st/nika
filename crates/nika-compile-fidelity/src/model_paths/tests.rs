// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{ModelPaths, ModelSink, model_steps_on_paths};

const READ: &str = "  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes.md\" } }\n";
const WRITE_SUMMARY: &str = "  write:\n    with: { summary: \"${{ tasks.summarize.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/summary.md\", content: \"${{ with.summary }}\" } }\n";
const WRITE_COPY: &str = "  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/summary.md\", content: \"${{ with.text }}\" } }\n";
const SUMMARIZE: &str = "  summarize:\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Summarize ${{ with.text }}\" }\n";

/// The walk over `read` + `tasks`, with `outputs` appended.
fn paths(tasks: &str, outputs: &str) -> ModelPaths {
    let source = format!("nika: w\ntasks:\n{READ}{tasks}{outputs}");
    model_steps_on_paths(&source).expect("the fixture parses")
}

fn task(id: &str) -> ModelSink {
    ModelSink::Task(id.to_owned())
}

fn step(id: &str, sink: ModelSink) -> (String, ModelSink) {
    (id.to_owned(), sink)
}

/// The request asks for a summary and the candidate copies the file: the write's path holds no
/// model step, and none exists elsewhere.
#[test]
fn a_copy_carries_no_model_step() {
    let p = paths(WRITE_COPY, "");
    assert_eq!(p.sinks, [task("write")]);
    assert!(p.steps.is_empty() && p.unreached.is_empty(), "{p:?}");
}

/// A model step whose output no sink reads is unreached: the decorative call.
#[test]
fn a_model_step_no_sink_reads_is_unreached() {
    let p = paths(&format!("{SUMMARIZE}{WRITE_COPY}"), "");
    assert!(p.steps.is_empty(), "{p:?}");
    assert_eq!(p.unreached, ["summarize"]);
}

/// A model step whose output the write binds is on the write's path.
#[test]
fn a_model_step_the_write_reads_is_on_its_path() {
    let p = paths(&format!("{SUMMARIZE}{WRITE_SUMMARY}"), "");
    assert_eq!(p.steps, [step("summarize", task("write"))]);
    assert!(p.unreached.is_empty(), "{p:?}");
}

/// An agent granted an effect tool writes the result itself: it is its own sink and its own step.
#[test]
fn a_writing_agent_is_its_own_sink_and_step() {
    let p = paths(
        "  draft:\n    with: { text: \"${{ tasks.read.output }}\" }\n    agent: { prompt: \"Draft a reply to ${{ with.text }} in ./out/reply.md\", tools: [\"nika:write\"] }\n",
        "",
    );
    assert_eq!(p.sinks, [task("draft")]);
    assert_eq!(p.steps, [step("draft", task("draft"))]);
}

/// A child-workflow call on the path hides its own steps: it is named, never counted as a step.
#[test]
fn a_child_call_on_the_path_is_named() {
    let with_write = paths(
        &format!(
            "  summarize:\n    with: {{ text: \"${{{{ tasks.read.output }}}}\" }}\n    invoke: {{ workflow: \"./summarize.nika\", args: {{ text: \"${{{{ with.text }}}}\" }} }}\n{WRITE_SUMMARY}"
        ),
        "",
    );
    assert_eq!(
        with_write.children,
        [(
            "summarize".to_owned(),
            "./summarize.nika".to_owned(),
            task("write")
        )]
    );
    assert!(with_write.steps.is_empty(), "{with_write:?}");
}

/// An `outputs:` key is a sink, and a model step it reads is on its path.
#[test]
fn an_output_key_is_a_sink() {
    let p = paths(
        SUMMARIZE,
        "outputs:\n  summary: \"${{ tasks.summarize.output }}\"\n",
    );
    let summary = ModelSink::Output("summary".to_owned());
    assert_eq!(p.sinks, std::slice::from_ref(&summary));
    assert_eq!(p.steps, [step("summarize", summary)]);
}

/// A group fold reaches every member.
#[test]
fn a_group_fold_reaches_every_member() {
    let p = paths(
        "  tone:\n    group: views\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Tone of ${{ with.text }}\" }\n  topic:\n    group: views\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Topic of ${{ with.text }}\" }\n  write:\n    with: { views: \"${{ group.views }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/views.json\", content: \"${{ with.views }}\" } }\n",
        "",
    );
    let mut steps = p.steps.clone();
    steps.sort();
    assert_eq!(
        steps,
        [step("tone", task("write")), step("topic", task("write"))]
    );
}

/// `after:` orders without carrying a value: a model step that only precedes the write never feeds
/// it.
#[test]
fn an_after_edge_carries_no_value() {
    let p = paths(
        &format!(
            "{SUMMARIZE}  write:\n    after: {{ summarize: success }}\n    with: {{ text: \"${{{{ tasks.read.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./out/summary.md\", content: \"${{{{ with.text }}}}\" }} }}\n"
        ),
        "",
    );
    assert!(p.steps.is_empty(), "{p:?}");
    assert_eq!(p.unreached, ["summarize"]);
}

/// A GET fetch reads the world; a POST sends a result to it.
#[test]
fn only_a_sending_fetch_is_a_sink() {
    let p = paths(
        "  look:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.test/a\" } }\n  send:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.test/in\", method: \"POST\", body: \"${{ with.text }}\" } }\n",
        "",
    );
    assert_eq!(p.sinks, [task("send")]);
    let sent = (task("send"), "https://example.test/in".to_owned());
    assert_eq!(p.destinations, [sent]);
}

/// A sink names the literal path it writes; a templated path is the run's, never named.
#[test]
fn a_sink_names_only_the_literal_it_writes() {
    let p = paths(WRITE_COPY, "");
    let written = (task("write"), "./out/summary.md".to_owned());
    assert_eq!(p.destinations, [written]);
    let templated = "  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/${{ with.text }}.md\", content: \"x\" } }\n";
    assert!(paths(templated, "").destinations.is_empty());
}

/// Unparsable bytes give no walk, never an empty one, which would read as « nothing carries it ».
#[test]
fn unparsable_bytes_give_no_walk() {
    assert!(model_steps_on_paths("nika: [").is_none());
}
