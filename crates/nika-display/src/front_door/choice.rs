// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Existing serialized choice snapshots and their welcome projections.

use std::fmt::Write as _;

use serde::Serialize;

use crate::theme::{Role, Theme};

/// One barreau of the scale (D-cand-1).
///
/// No `next` here. Every rung used to carry its own copy of the same
/// string, and the JSON mirror served those copies while the TTY
/// derived a fresh one from the directory — so `rungs[].next` kept
/// saying `nika compile hello hello.nika` in a folder that already held the file
/// (#1187). The next step belongs to the SCREEN, not to a rung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rung {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub ready: bool,
    pub reason: String,
}

/// The cascade — persisté, source unique.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InferenceChoice {
    pub rungs: Vec<Rung>,
    /// Featured rung id after sort (the arrow).
    pub arrow: String,
    /// Suggested execution model; never an authoring mutation.
    pub chosen_model: String,
    pub slogan: String,
    pub ram_gb: Option<u32>,
    pub local_tier: String,
    pub local_pull: String,
    pub local_download_gb: String,
    /// ACP seats observed (ids only). Doctor --json projects this.
    pub acp_runtimes: Vec<AcpRuntime>,
    /// Env NAMES present, never values.
    pub keys_present: Vec<String>,
    /// Harness seat id when the arrow is ACCESS · what `--access` pins.
    pub chosen_access: Option<String>,
}

/// One harness seat as doctor --json names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcpRuntime {
    pub id: String,
    pub detected: bool,
    pub authenticated: bool,
}

impl InferenceChoice {
    /// Human projection — TTY and pipe render this same product, and
    /// so does `--json`: `next` is computed ONCE by the caller and
    /// handed to every projection (#1187).
    #[must_use]
    pub fn render_human_next(&self, theme: Theme, next: &str) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{}", theme.paint(Role::Strong, &self.slogan));
        let _ = writeln!(s);
        let _ = writeln!(s, "Nika runs a plan from a file, with a model you pick.");
        if let Some(gb) = self.ram_gb {
            let _ = writeln!(
                s,
                "{}",
                // « this hardware », not « this machine » — the body's
                // `this machine` section is the ENVIRONMENT one
                // (editors · providers · workspace). One name, one
                // referent: the same two words stood for three things
                // on this screen (#1196 · the A-06 class in prose).
                theme.paint(
                    Role::Dim,
                    &format!(
                        "this hardware · {gb} GB · Gear One {} ({})",
                        self.local_tier, self.local_download_gb
                    )
                )
            );
        }
        let _ = writeln!(s);
        for rung in self.rungs.iter().filter(|r| r.id != "cloud") {
            if !rung.available && !rung.ready && rung.id == "harness" {
                continue;
            }
            if !rung.available && !rung.ready && rung.id == "key" {
                continue;
            }
            let arrow = if rung.id == self.arrow { "▸ " } else { "  " };
            let name = if rung.id == self.arrow {
                theme.paint(Role::Strong, &rung.name)
            } else {
                rung.name.clone()
            };
            let _ = writeln!(s, "{arrow}{name:<22} {}", rung.reason);
        }
        let _ = writeln!(s);
        let _ = writeln!(s, "Next:");
        let _ = writeln!(s, "  {}", theme.paint(Role::Strong, next));
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "Coming: Nika Cloud · our models, our tools, your workflows online."
        );
        s
    }

    /// Versioned welcome envelope fragment.
    ///
    /// `next` is the screen's ONE next step, passed in rather than
    /// derived: an agent reading `rungs[].next` and a human reading
    /// the `Next:` block must be told the same thing (#1187). Cloud is
    /// the one rung with no door — it does not ship yet.
    #[must_use]
    pub fn welcome_json(&self, next: &str) -> serde_json::Value {
        let rungs: Vec<serde_json::Value> = self
            .rungs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "name": r.name,
                    "available": r.available,
                    "ready": r.ready,
                    "reason": r.reason,
                    "next": if r.id == "cloud" { "" } else { next },
                })
            })
            .collect();
        serde_json::json!({
            "arrow": self.arrow,
            "next": next,
            "chosen_model": self.chosen_model,
            "chosen_access": self.chosen_access,
            "slogan": self.slogan,
            "rungs": rungs,
        })
    }
}
