// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A request to see the results (NIK-234, live gauntlet wave 9 r03): « Pouvez-vous me montrer le
//! contenu des trois traductions » reran the saved workflow, a metered run that rewrote the files
//! the person wanted to read. Here the host's router hears a request to run in every line while
//! nothing waits ([`hears_runs`]), as the live one heard one in that line: what the Session does
//! with that reading is the journey's to prove. `shown_result_journeys` judges it.

use serde_json::json;

use super::{Act, DIGEST, HACKER_NEWS, Ran, Scenario, TECHCRUNCH, digest, resolution};
use crate::peer::{Script, call, call_saying, say};

/// The request: the two sites and the file, in the person's words.
const REQUEST: &str = "résume Hacker News et TechCrunch dans ./news/digest.md";
/// What the run wrote.
pub(crate) const WRITTEN: &str = "# Tech\n- Rust 1.97 est sorti\n";
/// The person asks to see what was written, as the live gauntlet's person did.
pub(crate) const SHOW: &str =
    "Merci. Pouvez-vous me montrer le contenu du résumé, pour que je le relise ?";
/// The same request in English.
pub(crate) const SHOW_EN: &str = "Thanks. Can you show me what the digest says now?";
/// What the author answers once it read the file.
pub(crate) const SHOWN: &str = "Voici le contenu de ./news/digest.md : « Rust 1.97 est sorti ».";

/// Whether the host's router of the scenario `name` hears a request to run in every line while
/// nothing waits.
pub(crate) fn hears_runs(name: &str) -> bool {
    name.starts_with("shown_results")
}

/// The digest of the two sites the request names.
fn written() -> serde_json::Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
        ],
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md",
    })
}

/// Proposed, saved with « oui », run with « relance-le » to its end, which wrote the digest;
/// then `line` asks to see it, and the author reads the file and answers.
fn shown(line: &'static str) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit("oui"),
            Act::Turn("relance-le"),
            Act::RunEnds(Ran::Wrote(DIGEST, WRITTEN)),
            Act::Turn(line),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("read", json!({"path": DIGEST})),
                say(SHOWN),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

pub(crate) fn scenario(name: &str) -> Scenario {
    match name {
        "shown_results_in_french" => shown(SHOW),
        "shown_results_in_english" => shown(SHOW_EN),
        other => panic!("unknown shown-results scenario {other}"),
    }
}
