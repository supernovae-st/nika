// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/details` — how the last workflow was built, on demand: the authoring
//! backend and model, the calls, tokens and time, the strategy, the
//! decision seat, the knowledge the seat read (the pinned snapshot, the
//! pack's digest, every reference, the instruction digest of every call
//! that carried it), the engine and spec identity. Read from the compiler's
//! own provenance and the session's record beside it, never invented;
//! advanced, never in the ordinary conversation (the third level of
//! disclosure).

use std::fmt::Write as _;

use nika_onboard::compile::reading::{decision_words, receipt_words};

use super::SessionRuntime;

/// The authoring receipt's lines in the compile unit's own words ([`receipt_words`]: the model,
/// the calls, tokens and time, where the calls really went, each explicit reasoning effort, the
/// cost basis), with where this host reads a run's cost.
pub(super) fn receipt_lines(receipt: &nika_onboard::compile::AuthoringReceipt, text: &mut String) {
    text.push_str(&receipt_words(
        receipt,
        "a run's cost is in its result and `/proof`",
    ));
}

impl SessionRuntime {
    /// The details card for the last compiler reading of this session.
    #[must_use]
    pub fn details(&self) -> String {
        let mut text = "Details · how the last workflow was built (advanced)".to_owned();
        let _ = write!(text, "\n  {}", self.intelligence_line());
        let _ = write!(text, "\n  {}", self.seat.line());
        let _ = write!(text, "\n  {}", self.authoring_context.line());
        if !self.routes.is_empty() {
            let _ = write!(
                text,
                "\n  routes: {} open line(s) routed this session (phase · act · how · the line's hash) · last:",
                self.routes.len()
            );
            for record in self.routes.iter().rev().take(5) {
                let _ = write!(text, "\n    {}", record.line());
            }
        }
        let Some(out) = &self.last_outcome else {
            text.push_str(
                "\n  no workflow was read in this session yet · describe work to build and come back",
            );
            return text;
        };
        let prov = &out.provenance;
        let _ = write!(text, "\n  reading: {:?}", prov.cognition);
        match &prov.authoring {
            Some(receipt) => receipt_lines(receipt, &mut text),
            None => text
                .push_str("\n  authoring backend: none (no model call: the deterministic reading)"),
        }
        if let Some(strategy) = &prov.strategy {
            let _ = write!(text, "\n  strategy: {strategy:?}");
        }
        if let Some(skeleton) = &prov.skeleton {
            let _ = write!(text, "\n  skeleton: {skeleton}");
        }
        if let Some(decision) = &prov.decision {
            decision_words(decision, &mut text);
        }
        let _ = write!(
            text,
            "\n  engine: compiler {} · spec {}",
            prov.compiler_version,
            prov.spec_pin.chars().take(12).collect::<String>()
        );
        if let Some(trace) = &self.last_trace {
            let _ = write!(
                text,
                "\n  last run: trace `{}` (`/proof` judges it)",
                super::shown_trace(&self.snapshot.root, trace)
            );
        }
        text.push_str(
            "\n  none of this is a proof: `/proof` judges a run's trace · `/meaning` shows what was kept of your request",
        );
        text
    }
}
