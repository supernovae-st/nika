// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The egress journal: the one greppable line per [`EgressEvent`], and the
//! stderr observer the runner wires when none is injected.
//!
//! The proxy calls its observer on the connection thread, before it dials.
//! `nika run` holds the stderr lock until the run ends, so a line written
//! there kept every CONNECT of a confined `exec` waiting for the run, and
//! the child's client timed out. The observer only queues the line; one
//! journal thread writes it.

use std::sync::mpsc::{self, SendError, Sender};
use std::sync::{Arc, OnceLock};

use super::{EgressEvent, EgressObserver};

/// Where a journal line goes (stderr in production, a probe in tests).
type LineWriter = Arc<dyn Fn(&str) + Send + Sync>;

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
/// is worse than an unformatted one. The lines ride [`queued_journal`].
pub(crate) fn stderr_journal() -> EgressObserver {
    queued_journal(write_to_stderr)
}

#[allow(clippy::disallowed_macros, clippy::print_stderr)]
fn write_to_stderr(line: &str) {
    eprintln!("{line}");
}

/// An observer that queues each event's line for `write`, which runs on one
/// journal thread: started by the first event, ended when the observer is
/// dropped, the lines in their order. A `write` that waits delays the later
/// lines, never the connection. Should the thread fail to start, `write`
/// runs inline and the connection waits for it, as it used to.
pub(super) fn queued_journal(write: impl Fn(&str) + Send + Sync + 'static) -> EgressObserver {
    let write: LineWriter = Arc::new(write);
    let queue: OnceLock<Option<Sender<String>>> = OnceLock::new();
    Arc::new(move |event: &EgressEvent| {
        let line = journal_line(event);
        let line = match queue.get_or_init(|| start_journal(Arc::clone(&write))) {
            Some(sender) => match sender.send(line) {
                Ok(()) => return,
                Err(SendError(line)) => line,
            },
            None => line,
        };
        write(&line);
    })
}

/// Start the journal thread; `None` when it could not be spawned.
fn start_journal(write: LineWriter) -> Option<Sender<String>> {
    let (sender, lines) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("nika-egress-journal".to_owned())
        .spawn(move || {
            for line in lines {
                write(&line);
            }
        })
        .ok()
        .map(|_| sender)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::EgressDecision;
    use std::time::Duration;

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

    #[test]
    fn one_journal_thread_writes_the_lines_in_their_order() {
        let (written, lines) = mpsc::channel::<(String, Option<String>)>();
        let observer = queued_journal(move |line| {
            let thread = std::thread::current().name().map(str::to_owned);
            let _ = written.send((line.to_owned(), thread));
        });
        for port in [1, 2, 3] {
            observer(&EgressEvent::Decision(EgressDecision {
                host: "h".to_owned(),
                port,
                allowed: true,
            }));
        }
        let got: Vec<(String, Option<String>)> = (0..3)
            .map(|_| lines.recv_timeout(Duration::from_secs(2)).expect("a line"))
            .collect();
        let expected: Vec<(String, Option<String>)> = (1..=3)
            .map(|port| {
                (
                    format!("nika:egress allowed h:{port}"),
                    Some("nika-egress-journal".to_owned()),
                )
            })
            .collect();
        assert_eq!(got, expected, "written by the journal thread, in order");
    }
}
