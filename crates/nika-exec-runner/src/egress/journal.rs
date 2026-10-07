// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The egress journal: the one greppable line per [`EgressEvent`], and the
//! stderr observer the runner wires when none is injected.

use std::sync::Arc;

use super::{EgressEvent, EgressObserver};

/// The one journal line for an event (pure — the [`stderr_journal`]
/// wrapper's `eprintln` is the only impurity, so tests pin the EXACT
/// shapes: the REFUSED row is the greppable security event, `allowed`
/// the debug line, `closed` the metering row).
fn journal_line(event: &EgressEvent) -> String {
    match event {
        EgressEvent::Decision(d) if d.allowed => {
            format!("nika:egress allowed {}:{}", d.host, d.port)
        }
        EgressEvent::Decision(d) => format!(
            "nika:egress REFUSED {}:{} (not in permits.net.http)",
            d.host, d.port
        ),
        EgressEvent::Closed {
            host,
            port,
            bytes_up,
            bytes_down,
        } => format!("nika:egress closed {host}:{port} up={bytes_up} down={bytes_down}"),
    }
}

/// The default journal when no observer is injected — a namespaced stderr
/// line per event (see the module doc for the FCI-009 seam rationale).
/// REFUSED is the security event (greppable), `allowed` the debug line,
/// `closed` the metering row (F-P5 · octets, never content).
/// stderr, NOT `tracing::warn!`: no workspace tracing subscriber exists
/// (the `StderrEmitter` precedent in `nika-cli`), so a tracing call would
/// journal into the void — and a security journal that can silently vanish
/// is worse than an unformatted one.
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
pub(crate) fn stderr_journal() -> EgressObserver {
    Arc::new(|e: &EgressEvent| eprintln!("{}", journal_line(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::EgressDecision;

    #[test]
    fn the_journal_lines_are_the_greppable_contract() {
        // F-P5 (b) · REFUSED is the security event, verbatim — the
        // composer greps this line; `allowed` and `closed` are the debug
        // and metering rows.
        let refused = journal_line(&EgressEvent::Decision(EgressDecision {
            host: "evil.com".to_owned(),
            port: 443,
            allowed: false,
        }));
        assert_eq!(
            refused,
            "nika:egress REFUSED evil.com:443 (not in permits.net.http)"
        );
        let allowed = journal_line(&EgressEvent::Decision(EgressDecision {
            host: "api.github.com".to_owned(),
            port: 443,
            allowed: true,
        }));
        assert_eq!(allowed, "nika:egress allowed api.github.com:443");
        let closed = journal_line(&EgressEvent::Closed {
            host: "api.github.com".to_owned(),
            port: 443,
            bytes_up: 128,
            bytes_down: 4096,
        });
        assert_eq!(
            closed,
            "nika:egress closed api.github.com:443 up=128 down=4096"
        );
    }
}
