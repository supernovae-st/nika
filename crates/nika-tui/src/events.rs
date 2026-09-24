// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One event broker (ADR-139 · law 4).
//!
//! Exactly one thread reads the terminal; the UI loop reads typed
//! [`UiEvent`]s from one channel and never touches stdin itself. `SIGTERM`
//! (and a `SIGINT` delivered by a signal rather than a key, which raw mode
//! otherwise turns into `Ctrl+C`) arrive on the same channel, so the loop
//! has one place to decide what an interruption means in its current state.
//!
//! The loop is synchronous: it blocks on one std channel, so the plain run
//! path the session hands the terminal to may build its own executor on
//! the same thread (a runtime cannot start inside a runtime). The signal
//! watcher runs on its own thread with its own small executor.
//!
//! Why a thread with a short `poll` rather than crossterm's `EventStream`:
//! crossterm keeps ONE input reader behind a lock, and a cursor-position
//! query (every inline viewport computation, every resize, the switch back
//! from the focus view) needs that lock within two seconds. A stream's
//! reader thread holds the lock while it blocks and releases it only some
//! time after the stream is dropped, a window that differs by platform. The
//! thread here holds the lock for at most one `poll` slice ([`POLL_SLICE`])
//! and, once [`Broker::pause`] is asked, parks without touching the reader
//! until [`Broker::resume`]; `pause` returns only after the thread has
//! acknowledged, so the query that follows always finds the lock free.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use std::sync::mpsc;

use crossterm::event::{Event, KeyEvent, KeyEventKind};

/// How long one `poll` may hold the input reader.
pub const POLL_SLICE: Duration = Duration::from_millis(50);
/// How long `pause` waits for the reader thread to park before giving up.
const PAUSE_ACK: Duration = Duration::from_millis(1000);

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
    /// The reader failed (stdin gone); the loop leaves.
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

/// The broker: one reader thread, one signal task, one channel.
#[derive(Debug)]
pub struct Broker {
    rx: mpsc::Receiver<UiEvent>,
    paused: Arc<AtomicBool>,
    parked: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Broker {
    /// Start reading the terminal and watching the signals.
    #[must_use]
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let paused = Arc::new(AtomicBool::new(false));
        let parked = Arc::new(AtomicBool::new(false));
        let stopping = Arc::new(AtomicBool::new(false));
        let reader = std::thread::Builder::new()
            .name("nika-tui-input".to_owned())
            .spawn({
                let tx = tx.clone();
                let paused = Arc::clone(&paused);
                let parked = Arc::clone(&parked);
                let stopping = Arc::clone(&stopping);
                move || read_loop(&tx, &paused, &parked, &stopping)
            })
            .ok();
        // The signal watcher lives on its own thread with its own small
        // executor and ends with the process: a signal after the shell left
        // finds no receiver and is dropped.
        let _signals = std::thread::Builder::new()
            .name("nika-tui-signals".to_owned())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build()
                else {
                    return;
                };
                runtime.block_on(watch_signals(tx));
            });
        Self {
            rx,
            paused,
            parked,
            stopping,
            reader,
        }
    }

    /// The next event, blocking; `None` once every sender is gone.
    pub fn recv(&mut self) -> Option<UiEvent> {
        self.rx.recv().ok()
    }

    /// The next event if one is already waiting; never blocks. The shell
    /// asks while a turn runs, so an interruption is heard before the
    /// turn ends.
    pub fn try_recv(&mut self) -> Option<UiEvent> {
        self.rx.try_recv().ok()
    }

    /// Drain pre-question input after painting a fresh consent question. The
    /// reader is parked, so the broker remains the single input owner. Signals
    /// and terminal changes are returned to the shell instead of discarded.
    ///
    /// # Errors
    /// Returns an error if the reader cannot be parked, terminal input fails,
    /// or queued input exceeds the bounded drain. The reader is resumed before
    /// returning in each case.
    pub fn discard_typeahead(&mut self) -> std::io::Result<Vec<UiEvent>> {
        self.pause();
        let result = (|| {
            if !self.parked.load(Ordering::SeqCst) {
                return Err(std::io::Error::other(
                    "cannot establish fresh input boundary",
                ));
            }
            let mut events: Vec<_> = self.rx.try_iter().collect();
            for _ in 0..4096 {
                if !crossterm::event::poll(Duration::ZERO)? {
                    return Ok(events);
                }
                if let Some(event) = decode(crossterm::event::read()?) {
                    events.push(event);
                }
            }
            Err(std::io::Error::other(
                "terminal typeahead exceeds fresh review bound",
            ))
        })();
        self.resume();
        result
    }

    /// Park the reader: it stops touching the terminal's input until
    /// [`Broker::resume`]. Returns once the thread has acknowledged (or after
    /// one second if it never does), so the caller may query the cursor.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + PAUSE_ACK;
        while !self.parked.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Let the reader read again.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        // A later pause must not mistake the previous parked acknowledgement
        // for its own while the reader is resuming a terminal read.
        let deadline = std::time::Instant::now() + PAUSE_ACK;
        while self.parked.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Release stdin for good: the reader thread ends within one poll
    /// slice.
    pub fn stop(mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_loop(
    tx: &mpsc::Sender<UiEvent>,
    paused: &AtomicBool,
    parked: &AtomicBool,
    stopping: &AtomicBool,
) {
    while !stopping.load(Ordering::SeqCst) {
        if paused.load(Ordering::SeqCst) {
            parked.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        parked.store(false, Ordering::SeqCst);
        match crossterm::event::poll(POLL_SLICE) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(_) => {
                let _ = tx.send(UiEvent::Closed);
                return;
            }
        }
        let event = match crossterm::event::read() {
            Ok(event) => match decode(event) {
                Some(event) => event,
                None => continue,
            },
            Err(_) => UiEvent::Closed,
        };
        let closing = event == UiEvent::Closed;
        if tx.send(event).is_err() || closing {
            return;
        }
    }
}

fn decode(event: Event) -> Option<UiEvent> {
    Some(match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => UiEvent::Key(key),
        Event::Key(_) | Event::Mouse(_) => return None,
        Event::Paste(text) => UiEvent::Paste(text),
        Event::Resize(cols, rows) => UiEvent::Resize(cols, rows),
        Event::FocusGained => UiEvent::FocusGained,
        Event::FocusLost => UiEvent::FocusLost,
    })
}

#[cfg(unix)]
async fn watch_signals(tx: mpsc::Sender<UiEvent>) {
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
async fn watch_signals(tx: mpsc::Sender<UiEvent>) {
    if tokio::signal::ctrl_c().await.is_ok() {
        let _ = tx.send(UiEvent::Signal(Signal::Interrupt));
    }
}
