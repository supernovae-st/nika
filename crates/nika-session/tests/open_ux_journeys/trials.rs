// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trial journeys: a candidate tried where nothing leaves the room, on the pages its
//! sources answered and the project files it reads, before any proposal.

use serde_json::{Value, json};

use super::{Act, COUNT, COUNT_REQUEST, SEAT, Scenario};
use crate::peer::{Script, call, call_saying};

/// When every journey's pages were observed: 2026-10-10 08:00 UTC.
pub(crate) const OBSERVED_AT: u64 = 1_791_619_200_000;
/// The feed the trial journeys read.
pub(crate) const STORIES: &str = "https://feed.example/stories";
/// Its stories dated without an offset: the E1 shape `fromdateiso8601` refuses.
pub(super) const NO_OFFSET: &str = r#"{"hits": [{"title": "A", "created_at": "2026-10-10T08:00:00"}, {"title": "B", "created_at": "2026-10-10T09:30:00"}]}"#;
/// Its stories as an RSS feed, whose `feed` mode gives each item an RFC 3339 `published`.
pub(super) const STORIES_RSS: &str = "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Stories</title><item><title>A</title><link>https://feed.example/a</link><pubDate>Sat, 10 Oct 2026 08:00:00 GMT</pubDate></item><item><title>B</title><link>https://feed.example/b</link><pubDate>Sat, 10 Oct 2026 09:30:00 GMT</pubDate></item></channel></rss>";
/// The request of the trial journeys: the feed and the file, both named.
pub(crate) const STORIES_REQUEST: &str =
    "recupere https://feed.example/stories et garde les stories du jour dans ./news/today.json";
/// The request that sends a summary of the feed to a hook.
pub(crate) const SEND_REQUEST: &str = "recupere https://feed.example/stories, resume les et envoie le resume a https://hooks.example/team";
/// A filter that refuses a date without an offset, and its repair.
pub(super) const OFFSET_BROKEN: &str =
    "fromjson | [.hits[] | select((.created_at | fromdateiso8601) >= 0)]";
/// The repair keeps the civil date the request means, never inventing an offset.
pub(super) const OFFSET_FIXED: &str =
    "fromjson | [.hits[] | select(.created_at | startswith(\"2026-10-10\"))]";
/// A filter that reads RFC 822 dates out of an RFC 3339 feed (it keeps nothing), and its repair.
pub(super) const RFC_822_BROKEN: &str =
    "[.items[] | select(.published | test(\"^[A-Z][a-z]{2}, \"))]";
pub(super) const RFC_822_FIXED: &str =
    "[.items[] | select(.published | startswith(\"2026-10-10\"))]";
/// What the author says once its repaired candidate is shown.
pub(crate) const TRIED: &str = "Voici le workflow corrigé : il garde les stories du jour.";
/// The words a proposal's trial opens with.
pub(crate) const TRIED_ON: &str =
    "tried on the pages observed at 08:00 UTC: https://feed.example/stories";

/// The pages a scenario's trials observe, by address: every other page is none of the journey's.
pub(crate) fn feeds(name: &str) -> Vec<(&'static str, &'static str)> {
    match name {
        "trial_offset" => vec![(STORIES, NO_OFFSET)],
        "trial_kept_nothing"
        | "trial_names_what_it_skips"
        | "own_filter"
        | "two_edits"
        | "trial_assert" => {
            vec![(STORIES, STORIES_RSS)]
        }
        _ => Vec::new(),
    }
}

/// The stories of the day the jq `filter` keeps of the page read in `mode`, written to
/// `./news/today.json`.
pub(super) fn stories(mode: &str, filter: &str) -> Value {
    let source = format!(
        "nika: today-stories\npermits:\n  tools: [\"nika:fetch\", \"nika:jq\", \"nika:write\"]\n  net:\n    http: [\"feed.example\"]\n  fs:\n    write: [\"./news/today.json\"]\ntasks:\n  stories:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{STORIES}\", mode: {mode} }}\n  today:\n    with: {{ stories: \"${{{{ tasks.stories.output }}}}\" }}\n    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: \"${{{{ with.stories }}}}\", expression: '{filter}' }}\n  keep:\n    with: {{ kept: \"${{{{ tasks.today.output }}}}\" }}\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./news/today.json\", content: \"${{{{ with.kept }}}}\" }}\n"
    );
    json!({"source": source, "resolutions": [],
        "summary": "les stories du jour dans ./news/today.json"})
}

/// A candidate whose `broken` filter fails its trial, then its `fixed` revision.
pub(super) fn tried(mode: &str, broken: &str, fixed: &str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(STORIES_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", stories(mode, broken)),
                call("propose", json!({})),
                call("candidate_write", stories(mode, fixed)),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// I4c item 1: the broken filter and the workflow's name changed by one `candidate_edit`.
pub(super) fn two_edits() -> Scenario {
    let edits = json!({"edits": [
        {"old": RFC_822_BROKEN, "new": RFC_822_FIXED},
        {"old": "nika: today-stories", "new": "nika: stories-of-the-day"}]});
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(STORIES_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", stories("feed", RFC_822_BROKEN)),
                call("candidate_edit", edits),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The author's own jq filter over the named feed, written and proposed with no selection for it.
pub(super) fn own_filter() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(STORIES_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", stories("feed", RFC_822_FIXED)),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The feed summarized by the session's model and sent to a hook: a trial runs neither.
pub(super) fn trial_names_what_it_skips() -> Scenario {
    let source = format!(
        "nika: stories-digest\nmodel: {SEAT}\npermits:\n  tools: [\"nika:fetch\"]\n  net:\n    http: [\"feed.example\", \"hooks.example\"]\ntasks:\n  stories:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{STORIES}\", mode: feed }} }}\n  summarize:\n    with: {{ stories: \"${{{{ tasks.stories.output }}}}\" }}\n    infer: {{ prompt: \"Résume ces stories : ${{{{ with.stories }}}}\", max_tokens: 400 }}\n  send:\n    with: {{ digest: \"${{{{ tasks.summarize.output }}}}\" }}\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://hooks.example/team\", method: POST, body: \"${{{{ with.digest }}}}\" }} }}\n"
    );
    let write = json!({"source": source, "resolutions": [],
        "summary": "le résumé des stories envoyé à https://hooks.example/team"});
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(SEND_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", write),
                call_saying(
                    "Voici le workflow : il résume les stories et envoie le résumé.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
/// The sales the counting journeys read.
pub(super) const SALES: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n";

/// The counting workflow, written for the counting request.
pub(super) fn count_write() -> Value {
    json!({"source": COUNT, "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"})
}
/// The same sales, changed after the proposal.
pub(crate) const SALES_CHANGED: &str = "date,montant\n2026-10-01,12\n2026-10-02,30\n2026-10-03,7\n";
/// What the author says once the counting workflow is shown.
pub(super) const COUNTED: &str = "Voici le workflow : il compte les lignes de ./data/ventes.csv.";

/// The counting workflow proposed, then the person's yes; `changed` edits the sales between.
pub(super) fn reads_sales(changed: bool) -> Scenario {
    let mut acts = vec![Act::Turn(COUNT_REQUEST)];
    if changed {
        acts.push(Act::Edit("data/ventes.csv", SALES_CHANGED));
    }
    acts.push(Act::Submit("oui"));
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        preparing: false,
        acts,
        script: Script {
            agent: vec![
                call("candidate_write", count_write()),
                call_saying(COUNTED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The counting workflow written once over an output path no word of the person names, then
/// over the one they named, then proposed: no `check` or `verify` round trip.
pub(super) fn write_carries_its_findings() -> Scenario {
    let invented = json!({"source": COUNT.replace("./out/total.txt", "./out/other.txt"),
        "resolutions": [], "summary": "compte les lignes de ./data/ventes.csv"});
    Scenario {
        files: vec![("data/ventes.csv", SALES)],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(COUNT_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", invented),
                call("candidate_write", count_write()),
                call_saying(
                    "Voici le workflow : il compte les lignes de ./data/ventes.csv.",
                    "propose",
                    json!({}),
                ),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The trial journeys' request, newest first.
pub(crate) const NEWEST_REQUEST: &str = "recupere https://feed.example/stories et garde les stories du jour dans ./news/today.json, de la plus recente a la plus ancienne";
/// The order the request states, as a boolean over the stories kept.
pub(super) const NEWEST_FIRST: &str = "map(.published) == (map(.published) | sort | reverse)";
/// The filter that keeps the stories of the day, newest first.
pub(super) const NEWEST_FIXED: &str =
    "[.items[] | select(.published | startswith(\"2026-10-10\"))] | sort_by(.published) | reverse";

/// The stories the jq `filter` keeps, written once a `nika:assert` checked they are newest first.
pub(super) fn asserted(filter: &str) -> Value {
    let source = format!(
        "nika: newest-stories\npermits:\n  tools: [\"nika:fetch\", \"nika:jq\", \"nika:assert\", \"nika:write\"]\n  net:\n    http: [\"feed.example\"]\n  fs:\n    write: [\"./news/today.json\"]\ntasks:\n  stories:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{STORIES}\", mode: feed }}\n  today:\n    with: {{ stories: \"${{{{ tasks.stories.output }}}}\" }}\n    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: \"${{{{ with.stories }}}}\", expression: '{filter}' }}\n  newest_first:\n    with: {{ kept: \"${{{{ tasks.today.output }}}}\" }}\n    invoke:\n      tool: \"nika:jq\"\n      args: {{ input: \"${{{{ with.kept }}}}\", expression: '{NEWEST_FIRST}' }}\n  ordered:\n    with:\n      ok: ${{{{ tasks.newest_first.output }}}}\n    invoke:\n      tool: nika:assert\n      args:\n        condition: ${{{{ with.ok }}}}\n        message: the stories are not de la plus recente a la plus ancienne\n  keep:\n    after:\n      ordered: success\n    with: {{ kept: \"${{{{ tasks.today.output }}}}\" }}\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./news/today.json\", content: \"${{{{ with.kept }}}}\" }}\n"
    );
    json!({"source": source, "resolutions": [],
        "summary": "les stories du jour, de la plus récente à la plus ancienne"})
}

/// The newest-first stories: the trial stops at the assertion the filter breaks, then the
/// repaired filter is proposed.
pub(super) fn trial_assert() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(NEWEST_REQUEST)],
        script: Script {
            agent: vec![
                call("candidate_write", asserted(RFC_822_FIXED)),
                call("propose", json!({})),
                call("candidate_write", asserted(NEWEST_FIXED)),
                call_saying(TRIED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
