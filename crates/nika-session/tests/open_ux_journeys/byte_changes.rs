// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The change a revision says is read from its bytes (QA gauntlet q03, q04): the author binds
//! one more value of the very same bytes and says « J'ai retiré cette ligne »; the Session says
//! the bytes did not change, and claims no change of its own.

use serde_json::{Value, json};

use super::{Act, DIGEST, HACKER_NEWS, Scenario, TECHCRUNCH, digest, resolution};
use crate::peer::{Script, call, call_saying};

/// The request: the two sites and the file, in the person's words.
const REQUEST: &str = "résume Hacker News et TechCrunch dans ./news/digest.md";
/// A line at the proposal that names the file again and asks for a line removed.
const AGAIN: &str = "garde bien ./news/digest.md, et enlève la ligne en trop";
/// What the author says it did, writing the very same bytes.
pub(crate) const CLAIMED: &str = "J'ai retiré cette ligne.";

/// The digest of the two sites, with `rows` as its selections.
fn written(rows: &[Value]) -> Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": rows,
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md",
    })
}

/// Proposed, then written again byte for byte with the file bound too, and proposed again.
pub(super) fn same_bytes_again() -> Scenario {
    let sites = [
        resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
        resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
    ];
    let mut more = sites.to_vec();
    more.push(resolution(DIGEST, "derived", "output_path", "u2", DIGEST));
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![Act::Turn(REQUEST), Act::Turn(AGAIN)],
        script: Script {
            agent: vec![
                call("candidate_write", written(&sites)),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("candidate_write", written(&more)),
                call_saying(CLAIMED, "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
