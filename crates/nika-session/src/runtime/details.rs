// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/details` — how the last workflow was built, on demand: the authoring
//! backend and model, the calls, tokens and time, the strategy, the
//! decision seat, the engine and spec identity. Read from the compiler's
//! own provenance, never invented; advanced, never in the ordinary
//! conversation (the third level of disclosure).

use std::fmt::Write as _;

use super::SessionRuntime;

impl SessionRuntime {
    /// The details card for the last compiler reading of this session.
    #[must_use]
    pub fn details(&self) -> String {
        let mut text = "Details · how the last workflow was built (advanced)".to_owned();
        let _ = write!(text, "\n  {}", self.intelligence_line());
        let _ = write!(text, "\n  {}", self.seat.line());
        let Some(out) = &self.last_outcome else {
            text.push_str(
                "\n  no workflow was read in this session yet · describe work to build and come back",
            );
            return text;
        };
        let prov = &out.provenance;
        let _ = write!(text, "\n  reading: {:?}", prov.cognition);
        if let Some(receipt) = &prov.authoring {
            let _ = write!(
                text,
                "\n  authoring backend: {} · {} call{} · {} ms",
                receipt.model,
                receipt.calls,
                if receipt.calls == 1 { "" } else { "s" },
                receipt.elapsed_ms
            );
            if let (Some(i), Some(o)) = (receipt.input_tokens, receipt.output_tokens) {
                let _ = write!(text, " · {i} in / {o} out tokens");
            }
            text.push_str(
                "\n  cost: the compiler meters tokens, not money · a run's cost is in its result and `/proof`",
            );
        } else {
            text.push_str("\n  authoring backend: none (no model call: the deterministic reading)");
        }
        if let Some(strategy) = &prov.strategy {
            let _ = write!(text, "\n  strategy: {strategy:?}");
        }
        if let Some(skeleton) = &prov.skeleton {
            let _ = write!(text, "\n  skeleton: {skeleton}");
        }
        if let Some(decision) = &prov.decision {
            let route = decision
                .get("route")
                .and_then(|r| r.as_str())
                .unwrap_or("none recorded");
            let seat = decision
                .get("seat")
                .and_then(|s| s.get("model"))
                .and_then(|m| m.as_str());
            let _ = write!(text, "\n  decision: route {route}");
            if let Some(seat) = seat {
                let _ = write!(text, " · seat {seat}");
            }
            let ledger = decision
                .get("ledger")
                .and_then(|l| l.as_array())
                .map_or(0, Vec::len);
            if ledger > 0 {
                let _ = write!(
                    text,
                    " · ledger {ledger} clause{}",
                    if ledger == 1 { "" } else { "s" }
                );
            }
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
                trace.display()
            );
        }
        text.push_str(
            "\n  none of this is a proof: `/proof` judges a run's trace · `/meaning` shows what was kept of your request",
        );
        text
    }
}
