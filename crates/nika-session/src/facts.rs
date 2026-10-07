// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The deterministic facts — a Nika question answered from the engine's
//! own authorities before any model is asked: the workflows the snapshot
//! saw, the builtins the catalog ships, the providers this binary drives,
//! the example or template the ONE router names, a workflow's verdict
//! from the ONE facade, a code's teaching from the ONE ladder. Zero
//! tokens, zero invention — and the answer a session without any
//! conversational intelligence still gives.

use std::path::Path;

use crate::snapshot::ProjectSnapshot;

mod question;

use question::{Ask, Phase};

pub(crate) use nika_trace::run_view::last_run;

/// The fact an input asks for, when it asks for one.
///
/// Only a closed engine question gets a fact: its form makes an engine
/// entity — the workflows here, the builtins, the providers, the gallery, a
/// workflow's verdict, a code, the last run, a word of the language — the
/// subject of the question. A line that carries work (a path, a file, a URL,
/// an inline structure, several sentences) is never a fact, whatever words
/// it contains: understanding and authoring own it whole.
#[must_use]
pub fn answer(input: &str, snapshot: &ProjectSnapshot, root: &Path) -> Option<String> {
    answer_in(input, snapshot, root, Phase::Idle)
}

/// The fact a line typed beside a waiting proposal asks for: only a strict
/// catalog question (the workflows, builtins, providers, a code, the last
/// run, a definition). Any other question is about the proposal and reads
/// its bytes.
#[must_use]
pub(crate) fn answer_beside_proposal(
    input: &str,
    snapshot: &ProjectSnapshot,
    root: &Path,
) -> Option<String> {
    answer_in(input, snapshot, root, Phase::BesideProposal)
}

fn answer_in(input: &str, snapshot: &ProjectSnapshot, root: &Path, phase: Phase) -> Option<String> {
    match question::ask(input, snapshot, phase)? {
        Ask::Explain(code) => {
            let out =
                nika_cli_host::explain::run(&code, nika_cli_host::Theme::new(false, true, false));
            Some(out.text.trim_end().to_owned())
        }
        Ask::Verdict(path) => Some(verdict(root, snapshot, &path)),
        Ask::Workflows => Some(snapshot.facts_lines().join("\n")),
        Ask::Builtins => {
            let names = crate::guard::builtin_names();
            Some(format!(
                "{} builtins this engine ships (`nika catalog --tools` for their arguments):\n  {}",
                names.len(),
                names.join(" · ")
            ))
        }
        Ask::Providers => {
            let ids: Vec<&str> = nika_providers::CANONICAL_IDS.to_vec();
            Some(format!(
                "{} providers this binary drives (`nika catalog` for the models · `nika doctor` for this machine's paths):\n  {}",
                ids.len(),
                ids.join(" · ")
            ))
        }
        Ask::Gallery => Some(route(input)),
        Ask::LastRun => Some(last_run(root)),
        Ask::Vocabulary(asked) => vocabulary(&asked),
    }
}

/// The language's own meaning for each asked word, in the order asked.
fn vocabulary(asked: &[&'static str]) -> Option<String> {
    let lines: Vec<String> = asked
        .iter()
        .filter_map(|word| nika_vocab::glossary::entry(word))
        .map(|(known, meaning)| format!("{known} → {meaning}"))
        .collect();
    (!lines.is_empty()).then(|| {
        format!(
            "what Nika calls it:\n  {}\n  (exact shapes: ask for the schema · `nika spec --canon`)",
            lines.join("\n  ")
        )
    })
}

/// The ONE facade's verdict on a workflow the snapshot holds.
fn verdict(root: &Path, snapshot: &ProjectSnapshot, rel: &str) -> String {
    let path = root.join(rel);
    let Ok(source) = std::fs::read_to_string(&path) else {
        return format!("`{rel}` could not be read");
    };
    let base = path.parent().map(Path::to_path_buf);
    let mut read = |p: &str| std::fs::read_to_string(p).map_err(|e| e.to_string());
    match nika_cli_host::oracle::audit_source(
        &source,
        &path.display().to_string(),
        Some(&mut read),
        base.as_deref(),
        nika_cli_host::oracle::AuditOptions::default(),
    ) {
        Ok(audit) => {
            let v = &audit.verdict;
            let mut lines = vec![format!(
                "`{rel}` · {} · valid {} · access ready {} · capacity fit {} · run ready {} · grade {} (authority and spend, not danger)",
                if v.clean { "clean" } else { "findings" },
                tick(Some(v.layers.valid)),
                tick(v.layers.access_ready),
                tick(Some(v.layers.capacity_fit)),
                tick(v.layers.run_ready()),
                v.grade.as_str()
            )];
            for f in audit.report.findings.iter().take(6) {
                lines.push(format!(
                    "  · {} · {}",
                    f.code.as_deref().unwrap_or("-"),
                    f.message
                ));
            }
            for b in &v.layers.blockers {
                lines.push(format!("  · {b}"));
            }
            for h in audit.report.hints.iter().take(3) {
                lines.push(format!("  · hint · {} · {}", h.kind, h.advice));
            }
            if audit.report.findings.len() > 6 {
                lines.push(format!(
                    "  · … {} more (`nika check {rel}`)",
                    audit.report.findings.len() - 6
                ));
            }
            let _ = snapshot;
            lines.join("\n")
        }
        Err(e) => format!("`{rel}` does not parse: {}", e.diagnostic()),
    }
}

fn tick(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "✔",
        Some(false) => "✖",
        None => "○",
    }
}

/// The ONE router's answer for an explicit gallery question.
fn route(input: &str) -> String {
    match nika_onboard::routing::route_query(input) {
        nika_onboard::routing::RoutedEntry::Example(slug) => {
            format!(
                "the example `{slug}` fits — read it with `nika try {slug}`, own it with `nika compile {slug} <file>`"
            )
        }
        nika_onboard::routing::RoutedEntry::Skeleton(name) => {
            format!(
                "the template `{name}` fits — `nika compile {name} <file>` lays it down with its SLOT lines"
            )
        }
        nika_onboard::routing::RoutedEntry::Clarify(options) => format!(
            "closest shapes: {} — name one, or say more about the job",
            options.join(" · ")
        ),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("alpha.nika"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("a");
        std::fs::write(
            dir.path().join("curl.nika"),
            "nika: curl\nmodel: mock/echo\npermits: { exec: [\"curl\"], net: { http: [\"example.com\"] } }\ntasks:\n  fetch:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n",
        )
        .expect("c");
        dir
    }

    /// A closed engine question is still answered for zero tokens, however
    /// it is phrased around its entity.
    #[test]
    fn closed_engine_questions_still_answer_without_intelligence() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let root = dir.path();
        for query in [
            "examples",
            "templates?",
            "scaffolds",
            "show examples",
            "show me a template for fetching a URL",
            "list the templates",
            "which example fetches a url and summarizes it?",
            "what shapes are available?",
            "is there a template for invoices?",
            "what kind of templates are there?",
            "what's a good template for a digest?",
            "montre-moi les templates",
            "quels templates as-tu ?",
            "can I see the templates?",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no gallery fact"));
            assert!(
                reply.contains("nika compile") || reply.contains("closest shapes"),
                "{query}: {reply}"
            );
        }
    }

    /// The catalog questions (inventory, builtins, providers, a verdict, the
    /// last run, a word, a code) keep their fact in their usual phrasings.
    #[test]
    fn catalog_questions_still_answer_without_intelligence() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let root = dir.path();
        for query in [
            "which workflows are here?",
            "what workflows do I have?",
            "list my workflows",
            "please show me the workflows in this project",
            "which workflows are clean?",
            "Hi. What workflows are here?",
            "OK. list my workflows",
            "liste les workflows",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no inventory fact"));
            assert!(reply.contains("alpha.nika"), "{query}: {reply}");
        }
        for query in [
            "what tools are available?",
            "list the builtins",
            "Thanks! What tools are available?",
            "liste les builtins",
            "which tools ship with nika?",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no builtins fact"));
            assert!(reply.contains("nika:read"), "{query}: {reply}");
        }
        for query in [
            "which models can I use?",
            "what providers do you support?",
            "what local models are supported?",
            "quels providers sont supportés ?",
            "Hi there! What providers do you support?",
            "which LLM providers do you support?",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no providers fact"));
            assert!(
                reply.contains("providers this binary drives"),
                "{query}: {reply}"
            );
        }
        for query in [
            "check alpha.nika",
            "is ./alpha.nika valid?",
            "Is ALPHA valid?",
            "check if alpha is valid",
            "is the alpha workflow valid?",
            "check alpha stp",
            "can you check alpha for errors?",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no verdict fact"));
            assert!(reply.contains("`alpha.nika` · clean"), "{query}: {reply}");
        }
        for query in [
            "did it run?",
            "how did the last run go?",
            "why did the last run fail?",
            "did the last run succeed?",
            "what went wrong in the last run?",
            "what's the last run?",
            "what’s the last run?",
            "which tasks failed in the last run?",
            "can you show me the last run?",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no last run fact"));
            assert!(reply.contains("no run yet"), "{query}: {reply}");
        }
        let vocab = answer("what is the nika word for a cron?", &snap, root).expect("word");
        assert!(vocab.contains("cron →"), "{vocab}");
        let plural = answer("what are triggers in nika?", &snap, root).expect("plural word");
        assert!(plural.contains("trigger →"), "{plural}");
        let pair = answer("how do I use environment variables?", &snap, root).expect("a pair");
        assert!(pair.contains("environment variable →"), "{pair}");
        for query in [
            "can you explain NIKA-AUTH-006 to me?",
            "Explain `NIKA-AUTH-006`.",
            "explain this error: NIKA-AUTH-006",
        ] {
            let reply =
                answer(query, &snap, root).unwrap_or_else(|| panic!("{query}: no code fact"));
            assert!(reply.contains("NIKA-AUTH-006"), "{query}: {reply}");
        }
    }

    /// Beside a waiting proposal a question is about the proposal: only the
    /// strict catalog questions stay facts, the rest reads the proposal bytes.
    #[test]
    fn beside_a_proposal_only_catalog_questions_are_facts() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let root = dir.path();
        for query in [
            "which workflows are here?",
            "what tools are available?",
            "which providers are supported?",
            "explain NIKA-AUTH-006",
            "what happened in the last run?",
            "what is permits?",
            "what is a loop? and a retry?",
        ] {
            assert!(
                answer_beside_proposal(query, &snap, root).is_some(),
                "{query}"
            );
        }
        let about_the_proposal: Vec<&str> = [
            "is there a timeout?",
            "is there a retry if the fetch fails?",
            "can you show me an example of the output?",
            "is alpha still valid?",
            "how do I add a retry?",
            "what does a loop do?",
            "which template is this?",
        ]
        .into_iter()
        .filter(|query| answer_beside_proposal(query, &snap, root).is_some())
        .collect();
        assert!(
            about_the_proposal.is_empty(),
            "the proposal owns {about_the_proposal:#?}"
        );
    }

    /// A word of the catalog inside a job, a path or a file name is never a
    /// catalog question: the line reaches understanding whole. Each line
    /// below was intercepted by a substring before (template, workflow +
    /// which, model + which, provider, the run, what is + output).
    #[test]
    fn engine_words_inside_work_or_paths_are_not_fact_questions() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let requests = [
            "Each training form submission in ./submissions.json should produce a personal completion certificate for the person who submitted it. Fill the template in the same file with the submitter's name and the date part of their submission time.",
            "Fill the template in ./source.json and write ./out/certificates.json",
            "Start from ./template.json and write the filled document to ./out/result.json",
            "Read ./example.json and write its total to ./out/total.json",
            "Read https://example.com and summarize it in ./out/summary.md",
            "Can you fill the template and save ./out/result.json?",
            "Show ./template.json in the output report",
            "Show example.json in the output report",
            "Use this example to create my report",
            "Fill the template with the revised title instead",
            "Create a workflow which reads orders.csv and lists what each customer spent",
            "Build a workflow that shows which invoices are overdue",
            "Use a model to classify which tickets are urgent",
            "Which model should summarize each ticket in tickets.json?",
            "Read the providers in ./vendors.csv and list the ones we still pay",
            "Compute the running total of the sales and write it to ./out/total.md",
            "For each ticket, explain what is wrong with its output",
            "What is the total of the invoices in ./invoices.csv?",
            "Summarize the last run of each pipeline listed in ./runs.json",
            "Check that every order in ./orders.csv is valid and write ./out/valid.json",
            "List the tools mentioned in ./notes.md",
            "Save {submission, to, text} rows for every submission",
            "what does this workflow do?",
            "tell me about the workflow",
            "which tools does it use?",
            "which model does it use?",
            "is there a retry? add a retry of 3 to the fetch",
            "shape the output as a table",
            "can you show me an example of what it will write?",
            "scaffold a workflow that emails me the weather every morning",
            "template d'email pour relancer les clients en retard",
            "show me some examples of overdue invoices",
            "run the latest",
            "start from a template and add a retry with 3 attempts",
            "is a human approval required before the refund?",
            "are any secrets sent to the model?",
            "what's a cron? set a schedule for every monday at 9am",
            "Do a loop over the orders and email each customer their total",
            "are all the alpha outputs correct?",
            "use a local model",
            "can you use the builtins?",
            "which model will you use?",
            "which tools will you use?",
            "can you scaffold a workflow that emails me the weather every morning?",
            "can you shape the output as a markdown table?",
            "the last run failed, run it again",
            "did the last run fail? do it again",
            "the template doesn't fit, I need one that also emails the report",
            "can you give me some example invoices to test with?",
            "use a built-in tool",
            "can you find a template and build me a weekly digest workflow from it?",
            "which template is this?",
            "clean my workflows",
            "what's a timeout? and add a timeout",
            "as-tu un modèle d’email pour relancer les clients en retard ?",
            "what is nika-0.123?",
        ];
        let intercepted: Vec<&str> = requests
            .into_iter()
            .filter(|request| answer(request, &snap, dir.path()).is_some())
            .collect();
        assert!(
            intercepted.is_empty(),
            "understanding owns {intercepted:#?}"
        );
    }

    /// The facts answer without a model: the workflows, the builtins, the
    /// providers, a verdict, a code, a shape — and stay silent otherwise.
    #[test]
    fn the_facts_answer_from_the_engine_and_stay_silent_otherwise() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let root = dir.path();
        assert!(
            answer("what workflows are here?", &snap, root)
                .expect("workflows")
                .contains("alpha.nika")
        );
        let builtins = answer("which builtins exist?", &snap, root).expect("builtins");
        assert!(
            builtins.contains("nika:read") && builtins.contains("nika:jq"),
            "{builtins}"
        );
        let providers = answer("which providers are supported?", &snap, root).expect("providers");
        assert!(
            providers.contains("mistral") && providers.contains("ollama"),
            "{providers}"
        );
        let verdict = answer("is alpha valid? check it", &snap, root).expect("verdict");
        assert!(
            verdict.contains("`alpha.nika` · clean") && verdict.contains("valid ✔"),
            "{verdict}"
        );
        assert!(
            verdict.contains("· grade ") && verdict.contains("(authority and spend, not danger)"),
            "the grade is named for what it is: {verdict}"
        );
        assert!(!verdict.contains("risk "), "never the word risk: {verdict}");
        let hinted = answer("is curl valid?", &snap, root).expect("verdict with hints");
        assert!(
            hinted.contains("· hint ·") && hinted.contains("nika:fetch"),
            "the report's hints ride the verdict fact: {hinted}"
        );
        let explain = answer("explain NIKA-AUTH-006", &snap, root).expect("explain");
        assert!(explain.contains("NIKA-AUTH-006"), "{explain}");
        let shape = answer(
            "which example fetches a url and summarizes it?",
            &snap,
            root,
        )
        .expect("shape");
        assert!(
            shape.contains("nika compile") || shape.contains("closest shapes"),
            "{shape}"
        );
        assert!(
            answer("write me a poem about the sea", &snap, root).is_none(),
            "not a fact"
        );
        let vocab = answer(
            "what do you call a trigger here? and a secret?",
            &snap,
            root,
        )
        .expect("vocabulary");
        assert!(
            vocab.contains("trigger →")
                && vocab.contains("secret →")
                && vocab.contains("`secrets:`"),
            "{vocab}"
        );
        assert!(
            answer("is there a node concept?", &snap, root)
                .is_some_and(|v| v.contains("node → a `task`"))
        );
        let none_yet = answer("what happened in the last run?", &snap, root).expect("a fact");
        assert!(none_yet.contains("no run yet"), "{none_yet}");
        let store = root.join(".nika").join("traces");
        std::fs::create_dir_all(&store).expect("store");
        std::fs::write(
            store.join("2026-09-03T00-00-00Z-abcd.ndjson"),
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"digest\"}]}\n{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"task\",\"value\":\"read\"},{\"key\":\"note\",\"value\":\"invoke · nika:read\"},{\"key\":\"duration_ms\",\"value\":2}]}\n{\"kind\":\"task_failed\",\"fields\":[{\"key\":\"task\",\"value\":\"sum\"},{\"key\":\"error\",\"value\":\"NIKA-INFER-001 · no seat\"}]}\n{\"kind\":\"workflow_failed\",\"fields\":[{\"key\":\"error\",\"value\":\"task sum failed\"}]}\n",
        )
        .expect("trace");
        let last = answer("what happened in the last run?", &snap, root).expect("a fact");
        assert!(
            last.contains("last run · `digest` · failed · task sum failed")
                && last.contains("✔ read · invoke · nika:read · 2 ms")
                && last.contains("✖ sum · NIKA-INFER-001"),
            "read from the trace, never from memory: {last}"
        );
        let permits = answer("what is permits?", &snap, root).expect("the language's own word");
        assert!(
            permits.contains("permits →") && permits.contains("boundary"),
            "{permits}"
        );
        assert!(
            answer("tell me about the sea", &snap, root).is_none(),
            "a foreign word alone is not a question"
        );
    }
}
