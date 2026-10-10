// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a replay trial names before any run: each step it cannot exercise, in the words of what
//! that step reaches for, the filters it judges, and what each task reads.

use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

use super::super::{Capture, Captures};
use super::{screen, sources};

/// The feed the news trial reads.
const FEED: &str = "https://feed.example/items";

/// A news digest: a captured feed, a jq filter, a model step, a write and a post.
const NEWS: &str = r#"nika: news-trial
model: mock/echo
const:
  feed_url: "https://feed.example/items"
permits:
  tools: ["nika:fetch", "nika:jq", "nika:write"]
  net:
    http: ["feed.example", "hooks.example"]
  fs:
    write: ["./news/digest.md"]
tasks:
  feed:
    invoke:
      tool: "nika:fetch"
      args: { url: "${{ const.feed_url }}", mode: json }
  filter:
    with: { feed: "${{ tasks.feed.output }}" }
    invoke:
      tool: "nika:jq"
      args: { input: "${{ with.feed }}", expression: "[.items[] | select(.date | fromdateiso8601 > 0)]" }
  summarize:
    with: { items: "${{ tasks.filter.output }}" }
    infer:
      prompt: "Résume : ${{ with.items }}"
  write_digest:
    with: { digest: "${{ tasks.summarize.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./news/digest.md", content: "${{ with.digest }}" }
  post:
    with: { digest: "${{ tasks.summarize.output }}" }
    invoke:
      tool: "nika:fetch"
      args: { url: "https://hooks.example/team", method: POST, body: "${{ with.digest }}" }
"#;

fn parsed(source: &str) -> RawWorkflow {
    nika_schema::parse(source, FileId::new(0), ParseMode::Strict).expect("the fixture parses")
}

fn captured(urls: &[&str]) -> Captures {
    let mut captures = Captures::new();
    for url in urls {
        let page = Capture::new(*url, 200, None, b"{\"items\": []}".to_vec(), 0);
        captures.insert(page).expect("a small capture is lent");
    }
    captures
}

fn named(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    (pairs.iter())
        .map(|(task, need)| ((*task).to_owned(), (*need).to_owned()))
        .collect()
}

#[test]
fn a_trial_names_what_it_cannot_exercise_before_any_run() {
    let screened = screen(&parsed(NEWS), &captured(&[FEED]));
    let expected = named(&[
        ("summarize", "the model step"),
        ("post", "a request other than GET"),
    ]);
    assert_eq!(screened.unexercised, expected);
    assert_eq!(screened.filters, ["filter"]);
    assert_eq!(screened.upstream["filter"], ["feed"]);
    assert_eq!(screened.upstream["summarize"], ["filter"]);
    assert_eq!(screened.upstream["write_digest"], ["summarize"]);
    assert!(screened.upstream["feed"].is_empty());
}

#[test]
fn a_source_no_capture_holds_is_not_exercised() {
    let screened = screen(&parsed(NEWS), &Captures::new());
    assert_eq!(
        screened.unexercised[0],
        ("feed".to_owned(), "the network".to_owned())
    );
    // A capture answers only its exact address: another spelling is no capture of it.
    let other = screen(&parsed(NEWS), &captured(&["https://feed.example/items/"]));
    assert!(other.is_unexercised("feed"));
}

/// A workflow of one task, `step`, whose body is `body` (indented as a task's fields).
fn one(body: &str) -> String {
    format!(
        "nika: one\npermits: {{ tools: [\"nika:fetch\", \"nika:notify\", \"nika:prompt\"] }}\ntasks:\n  step:\n{body}"
    )
}

#[test]
fn each_step_beyond_the_room_is_named_in_its_own_words() {
    let fetch =
        |args: &str| format!("    invoke: {{ tool: \"nika:fetch\", args: {{ {args} }} }}\n");
    let cases = [
        (
            fetch("url: \"https://feed.example/items\", traverse: { max_pages: 2 }"),
            "a crawl",
        ),
        (
            fetch("url: \"https://feed.example/items\", mode: jq, jq: \".items\""),
            "a jq extraction",
        ),
        (
            fetch("url: \"https://feed.example/items\", method: DELETE"),
            "a request other than GET",
        ),
        (
            "    exec: { command: [\"touch\", \"x\"] }\n".to_owned(),
            "a process",
        ),
        (
            "    invoke: { tool: \"nika:notify\", args: { message: hi } }\n".to_owned(),
            "a notification",
        ),
        (
            "    invoke: { tool: \"nika:prompt\", args: { message: \"Proceed?\" } }\n".to_owned(),
            "a person",
        ),
        (
            "    agent: { prompt: hi, max_turns: 1 }\n".to_owned(),
            "the model step",
        ),
    ];
    for (body, need) in cases {
        let screened = screen(&parsed(&one(&body)), &captured(&[FEED]));
        assert_eq!(screened.unexercised, named(&[("step", need)]), "{body}");
    }
}

#[test]
fn an_address_known_only_at_run_time_is_exercised_and_screened_by_the_room() {
    let source = "nika: late\npermits: { tools: [\"nika:fetch\", \"nika:log\"] }\ntasks:\n  pick:\n    invoke: { tool: \"nika:log\", args: { message: hi } }\n  step:\n    with: { url: \"${{ tasks.pick.output }}\" }\n    invoke: { tool: \"nika:fetch\", args: { url: \"${{ with.url }}\" } }\n";
    let workflow = parsed(source);
    let screened = screen(&workflow, &Captures::new());
    assert!(screened.unexercised.is_empty(), "{screened:?}");
    assert_eq!(screened.upstream["step"], ["pick"]);
    let task = |id: &str| (workflow.tasks.iter()).find(|task| task.value.id.value == id);
    let (pick, step) = (task("pick").expect("pick"), task("step").expect("step"));
    assert!(
        screened.screens(&step.value),
        "a fetch the room replays or refuses"
    );
    assert!(!screened.screens(&pick.value), "a log the room runs");
}

#[test]
fn a_task_behind_a_step_the_trial_does_not_exercise_is_left_to_the_room() {
    let workflow = parsed(NEWS);
    let screened = screen(&workflow, &captured(&[FEED]));
    assert!(screened.behind("write_digest") && screened.behind("post"));
    assert!(!screened.behind("filter") && !screened.behind("summarize"));
    let task = |id: &str| {
        let found = (workflow.tasks.iter()).find(|task| task.value.id.value == id);
        &found.expect("the task is declared").value
    };
    assert!(
        screened.screens(task("write_digest")),
        "behind the model step"
    );
    assert!(screened.screens(task("feed")), "a fetch the room replays");
    assert!(
        !screened.screens(task("filter")),
        "a filter the host screens and the room runs"
    );
}

#[test]
fn a_trial_captures_the_addresses_known_before_the_run_once_each() {
    let workflow = parsed(NEWS);
    assert_eq!(sources(&workflow), [FEED], "the post is left out");
    let twice = NEWS.replace("method: POST, body: \"${{ with.digest }}\"", "mode: text");
    let twice = twice.replace("https://hooks.example/team", FEED);
    assert_eq!(
        sources(&parsed(&twice)),
        [FEED],
        "one address, captured once"
    );
    let crawl = one(
        "    invoke: { tool: \"nika:fetch\", args: { url: \"https://feed.example/items\", traverse: { max_pages: 2 } } }\n",
    );
    assert!(sources(&parsed(&crawl)).is_empty());
}
