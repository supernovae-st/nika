// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native machine door, `nika session --json`: one JSON command per stdin line, one frame
//! per stdout line. The Session's log is written in order by the calling thread (every frame
//! there carries `event`, `opened` first and `closed` last); direct replies (refusals, replayed
//! results, reads) are written by the reader as they are answered and never carry `event`.
//! stdin is read on its own thread, so a Stop, a close or a read is taken while a turn runs.
//! stdin's end closes the Session: a preparation under way is stopped and the door returns once
//! the log is closed.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, PoisonError};

use crate::host::{Dispatch, SessionHost};
use crate::wire::{Command, Frame, Refused};

/// Write one frame as one line.
fn write_line<W: Write>(output: &Mutex<W>, frame: &Frame) -> std::io::Result<()> {
    let mut output = output.lock().unwrap_or_else(PoisonError::into_inner);
    output.write_all(frame.to_line().as_bytes())?;
    output.write_all(b"\n")?;
    output.flush()
}

/// Drive `host` with commands from `input`, writing frames to `output`, until its log closes.
///
/// # Errors
/// The reader thread could not start, or `output` refused a write.
pub fn drive<R, W>(host: &Arc<SessionHost>, input: R, output: &Arc<Mutex<W>>) -> std::io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let reader_host = Arc::clone(host);
    let reader_output = Arc::clone(output);
    // Detached on purpose: a reader blocked on an open stdin must not keep a closed Session's
    // door from returning.
    std::thread::Builder::new()
        .name("nika-session-stdin".to_owned())
        .spawn(move || read_commands(&reader_host, input, &reader_output))?;
    let mut cursor = 0;
    loop {
        let (events, complete) = host.wait_events_after(cursor);
        for frame in &events {
            write_line(output, frame)?;
            cursor = frame.event().unwrap_or(cursor);
        }
        if complete {
            return Ok(());
        }
    }
}

/// Read commands until stdin ends, then close the Session.
fn read_commands<R: BufRead, W: Write>(host: &SessionHost, mut input: R, output: &Mutex<W>) {
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if text.trim().is_empty() {
            continue;
        }
        let reply = match Command::parse(text.as_bytes()) {
            Err(why) => Some(Frame::refused(
                host.session(),
                Refused::Malformed,
                why,
                None,
                None,
                None,
            )),
            Ok(command) => match host.dispatch(command) {
                Dispatch::Reply(frame) => Some(frame),
                Dispatch::Closing => break,
                _ => None,
            },
        };
        if let Some(frame) = reply
            && write_line(output, &frame).is_err()
        {
            break;
        }
    }
    let _closing = host.dispatch(Command::Close);
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
pub(crate) mod tests;
