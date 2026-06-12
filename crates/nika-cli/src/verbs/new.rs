// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika new --from <template|intent> <dest>` — instantiate one of the
//! embedded skeletons (spec §2). Refuses to overwrite (the human keeps
//! the hand); `--force` is the explicit override. The written file is
//! the template VERBATIM — slots stay visible so the author fills them
//! deliberately.
//!
//! `--from` resolves in two rungs:
//! 1. **exact template name** — instant, unchanged;
//! 2. **intent routing** — anything else is BM25-ranked (the admitted
//!    `nika-bm25` crate · Robertson-Zaragoza Okapi) against the
//!    template names + bodies, with a small everyday-word → Nika-vocab
//!    alias bridge on the QUERY side (« scrape » → fetch · « summarize »
//!    → infer · « parallel » → fan-out). The top match instantiates and
//!    the routing is SAID (`routed intent → template …`); a zero-score
//!    query gets the honest unknown-template error. Deterministic,
//!    zero-LLM — the floor of « the binary generates the best workflow
//!    for the intent » (editor LLM loops build ON this rung).

use std::path::Path;

use nika_bm25::{BmIndex, BmParams};

use crate::verbs::{VerbOutput, exit};

/// The `nika new` verb.
#[must_use]
pub fn run(template: &str, dest: &str, force: bool) -> VerbOutput {
    let (name, body, routed) = match nika_pack::template(template) {
        Some(body) => (template.to_owned(), body, false),
        None => match route_intent(template) {
            Some(name) => {
                // route_intent only returns names from template_names() —
                // the lookup is total; an empty body would be a pack bug
                // surfaced as the honest unknown error, never a panic.
                let Some(body) = nika_pack::template(&name) else {
                    return unknown(template);
                };
                (name, body, true)
            }
            None => return unknown(template),
        },
    };
    if Path::new(dest).exists() && !force {
        return VerbOutput {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: exit::ENV,
        };
    }
    if let Err(e) = std::fs::write(dest, body) {
        return VerbOutput {
            text: format!("cannot write {dest}: {e}"),
            code: exit::ENV,
        };
    }
    let routing = if routed { "routed intent → " } else { "" };
    VerbOutput {
        text: format!(
            "{dest} ← {routing}template `{name}` · fill the `# SLOT:` lines then `nika check {dest}`"
        ),
        code: exit::OK,
    }
}

/// The unknown-template finding. The `embedded set:` line is a WIRE
/// CONTRACT — editor integrations probe `nika new --from '?'` and parse
/// the set from exactly this shape; never reword it.
fn unknown(template: &str) -> VerbOutput {
    VerbOutput {
        text: format!(
            "unknown template `{template}` — embedded set: {}",
            nika_pack::template_names().join(" · ")
        ),
        code: exit::FILE,
    }
}

/// Everyday intent words → the Nika vocabulary the template bodies
/// actually carry. Query-side only (documents are never expanded) — the
/// evidenced cheap recall upgrade at tiny corpus scale, in place of
/// embeddings (BM25 stays the ranker).
const ALIASES: &[(&str, &[&str])] = &[
    ("scrape", &["fetch", "read"]),
    ("crawl", &["fetch"]),
    ("download", &["fetch"]),
    ("http", &["fetch"]),
    ("url", &["fetch"]),
    ("website", &["fetch"]),
    ("page", &["fetch"]),
    ("api", &["fetch", "invoke"]),
    ("llm", &["infer"]),
    ("ai", &["infer"]),
    ("model", &["infer"]),
    ("prompt", &["infer"]),
    ("summarize", &["infer", "think"]),
    ("classify", &["infer"]),
    ("generate", &["infer"]),
    ("save", &["write", "persist", "state"]),
    ("shell", &["exec"]),
    ("command", &["exec"]),
    ("script", &["exec"]),
    ("build", &["exec"]),
    ("test", &["exec", "verify"]),
    ("deploy", &["ship", "act", "exec"]),
    ("release", &["ship", "act"]),
    ("parallel", &["for_each", "fan", "merge"]),
    ("concurrent", &["for_each", "fan"]),
    ("batch", &["for_each", "items"]),
    ("each", &["for_each", "items"]),
    ("every", &["for_each", "items"]),
    ("loop", &["agent", "for_each"]),
    ("iterate", &["agent", "for_each"]),
    ("agentic", &["agent"]),
    ("autonomous", &["agent", "budgeted"]),
    ("review", &["gate", "verify"]),
    ("approve", &["gate", "human"]),
    ("approval", &["gate", "human"]),
    ("confirm", &["gate", "human"]),
    ("pipeline", &["chain", "gather", "think"]),
    ("sequence", &["chain"]),
    ("transform", &["jq", "process"]),
    ("json", &["jq"]),
    ("state", &["state", "diff", "delta"]),
    ("incremental", &["state", "diff", "delta"]),
];

/// BM25-route a free-form intent to the best embedded template.
/// `None` when no template shares a single term with the (alias-
/// expanded) query — routing on zero evidence would be a guess.
fn route_intent(intent: &str) -> Option<String> {
    let names = nika_pack::template_names();
    let mut index = BmIndex::new(BmParams::default());
    for (i, name) in names.iter().enumerate() {
        let body = nika_pack::template(name).unwrap_or_default();
        // Name + body carry the template's whole vocabulary — verbs ·
        // tools · the description SLOT prose.
        index.add_document(u32::try_from(i).ok()?, &format!("{name}\n{body}"));
    }
    index.finalize();

    let mut query = intent.to_owned();
    for token in intent.split(|c: char| !c.is_ascii_alphanumeric()) {
        let lower = token.to_ascii_lowercase();
        for (word, expansions) in ALIASES {
            if lower == *word {
                for e in *expansions {
                    query.push(' ');
                    query.push_str(e);
                }
            }
        }
    }

    let ranked = index.top_k(&query, 1);
    let (doc, score) = ranked.first()?;
    if *score <= 0.0 {
        return None;
    }
    names.get(*doc as usize).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest(tag: &str) -> String {
        std::env::temp_dir()
            .join(format!("nika-new-{}-{tag}.nika.yaml", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn exact_template_name_stays_the_fast_path() {
        let d = dest("exact");
        let out = run("chain", &d, true);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(!out.text.contains("routed"), "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn parallel_intent_routes_to_fanout() {
        let d = dest("par");
        let out = run("summarize every item in parallel", &d, true);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("routed intent → template `fanout`"),
            "{}",
            out.text
        );
        // Own-corpus by construction: the instantiated file IS the
        // embedded template verbatim.
        let written = std::fs::read_to_string(&d).expect("file written");
        assert_eq!(Some(written.as_str()), nika_pack::template("fanout"));
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn agentic_intent_routes_to_agent_loop() {
        let d = dest("agent");
        let out = run(
            "an autonomous budgeted agent that researches a topic",
            &d,
            true,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("template `agent-loop`"), "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn approval_intent_routes_to_a_gated_template() {
        let d = dest("gate");
        let out = run(
            "verify then wait for human approval before deploy",
            &d,
            true,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains("`human-gated-ship`") || out.text.contains("`gate-and-act`"),
            "a gated template must win: {}",
            out.text
        );
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn zero_evidence_intent_keeps_the_wire_contract_error() {
        // '?' is the editor probe — the `embedded set:` line is parsed
        // verbatim by integrations; gibberish shares no term either.
        for q in ["?", "zzzz qqqq xxxx"] {
            let out = run(q, &dest("zero"), true);
            assert_eq!(out.code, exit::FILE, "{}", out.text);
            assert!(out.text.contains("embedded set:"), "{}", out.text);
        }
    }
}
