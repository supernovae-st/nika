// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One event broker (ADR-139 · law 4).
//!
//! Exactly one task polls the crossterm [`EventStream`]; the UI loop reads
//! typed [`UiEvent`]s from one channel and never touches stdin itself.
//! `SIGTERM` (and a `SIGINT` delivered by a signal rather than a key, which
//! raw mode otherwise turns into `Ctrl+C`) arrive on the same channel, so the
//! loop has one place to decide what an interruption means in its current
//! state. Stopping the broker drops the stream: the reader thread crossterm
//! keeps is released before an external process (an editor, a browser login)
//! is handed the terminal.

use crossterm::event::{Event, EventStream, KeyEvent, KeyEventKind};
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// What the UI loop reacts to. Presses only: a terminal that reports
/// releases and repeats (the kitty protocol) never doubles a key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UiEvent {
    /// A key press.
    Key(KeyEvent),
    /// A bracketed paste: data, never keys.
    Paste(String),
    /// The terminal was resized to (columns, rows).
    Resize(u16, u16),
    /// The terminal window gained focus.
    FocusGained,
    /// The terminal window lost focus.
    FocusLost,
    /// A process signal.
    Signal(Signal),
    /// The event stream ended (stdin closed) or failed; the loop leaves.
    Closed,
}

/// The two signals the shell answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Signal {
    /// `SIGINT` as a signal (not the `Ctrl+C` key raw mode reports).
    Interrupt,
    /// `SIGTERM`: leave now, restore first.
    Terminate,
}

/// The broker: one stream task, one channel.
#[derive(Debug)]
pub struct Broker {
    rx: mpsc::UnboundedReceiver<UiEvent>,
    stream: JoinHandle<()>,
    signals: JoinHandle<()>,
}

impl Broker {
    /// Start polling the terminal and the signals.
    #[must_use]
    pub fn start() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let stream = tokio::spawn(read_stream(tx.clone()));
        let signals = tokio::spawn(watch_signals(tx));
        Self {
            rx,
            stream,
            signals,
        }
    }

    /// The next event, or `None` once every sender is gone.
    pub async fn next(&mut self) -> Option<UiEvent> {
        self.rx.recv().await
    }

    /// Release stdin: the stream task is aborted AND awaited, so the
    /// `EventStream` is really dropped before this returns. crossterm keeps
    /// one reader behind a lock that the stream's thread holds while it
    /// blocks; a cursor-position query (every inline viewport computation,
    /// every resize) can only take that lock once the stream is gone.
    pub async fn stop(self) {
        self.stream.abort();
        self.signals.abort();
        let _ = self.stream.await;
        let _ = self.signals.await;
    }
}

async fn read_stream(tx: mpsc::UnboundedSender<UiEvent>) {
    let mut reader = EventStream::new();
    while let Some(next) = reader.next().await {
        let event = match next {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => UiEvent::Key(key),
            Ok(Event::Paste(text)) => UiEvent::Paste(text),
            Ok(Event::Resize(cols, rows)) => UiEvent::Resize(cols, rows),
            Ok(Event::FocusGained) => UiEvent::FocusGained,
            Ok(Event::FocusLost) => UiEvent::FocusLost,
            Ok(Event::Key(_) | Event::Mouse(_)) => continue,
            Err(_) => UiEvent::Closed,
        };
        let closing = event == UiEvent::Closed;
        if tx.send(event).is_err() || closing {
            return;
        }
    }
    let _ = tx.send(UiEvent::Closed);
}

#[cfg(unix)]
async fn watch_signals(tx: mpsc::UnboundedSender<UiEvent>) {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        return;
    };
    loop {
        let event = tokio::select! {
            _ = term.recv() => UiEvent::Signal(Signal::Terminate),
            _ = int.recv() => UiEvent::Signal(Signal::Interrupt),
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

#[cfg(not(unix))]
async fn watch_signals(tx: mpsc::UnboundedSender<UiEvent>) {
    if tokio::signal::ctrl_c().await.is_ok() {
        let _ = tx.send(UiEvent::Signal(Signal::Interrupt));
    }
}
