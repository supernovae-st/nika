// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The plain session's line source: one line per call from the terminal,
//! the lock taken inside each read and released before returning, and a
//! pasted burst folded into ONE line (paste is data, never several
//! submissions). A size-cap member of the nika-cli unit hosts it
//! (D-2026-07-09-N1 · ADR-110): the door reads through it.

use std::io::BufRead;

/// One line from the terminal — and a pasted burst folded into ONE line.
///
/// Paste is data, never several submissions (UX-2 · the renderer's
/// bracketed-paste law). The plain loop has no paste bracket, so it reads
/// the line discipline's tell instead: when the next complete line is
/// already waiting the instant this one was read, nobody typed it — the
/// lines were pasted together. They join as one datum (their line ends
/// become spaces), so « yes ⏎ run it ⏎ /quit » pasted at a consent prompt
/// is one line that is not a consent, not three gestures. Best effort:
/// a paste the terminal delivers in slow pieces can still split.
///
/// # Errors
///
/// The terminal's own read error.
pub fn read_burst(buf: &mut Vec<u8>) -> std::io::Result<usize> {
    let mut stdin = std::io::stdin().lock();
    let mut total = stdin.read_until(b'\n', buf)?;
    while total > 0 && buf.last() == Some(&b'\n') && stdin_pending() {
        buf.pop();
        buf.push(b' ');
        let more = stdin.read_until(b'\n', buf)?;
        if more == 0 {
            buf.push(b'\n');
            break;
        }
        total += more;
    }
    Ok(total)
}

/// Fresh-input boundary of a spending question: flush `out`, discard all typeahead without
/// blocking (Rust stdin buffer, terminal queue, unterminated line), restore. Not atomic.
/// # Errors
/// Not a terminal, a failed mode change, or typeahead over 1 MiB: fail closed.
pub fn fresh_terminal<W: std::io::Write + ?Sized>(out: &mut W) -> std::io::Result<()> {
    use nix::sys::termios::{FlushArg, LocalFlags, SetArg, SpecialCharacterIndices as Cc};
    out.flush()?;
    let stdin = std::io::stdin();
    let saved = nix::sys::termios::tcgetattr(&stdin)?;
    let mut drain = saved.clone();
    drain.local_flags.remove(LocalFlags::ICANON);
    drain.control_chars[Cc::VMIN as usize] = 0;
    drain.control_chars[Cc::VTIME as usize] = 0;
    nix::sys::termios::tcsetattr(&stdin, SetArg::TCSANOW, &drain)?;
    let drained = discard(&mut stdin.lock(), 1 << 20);
    nix::sys::termios::tcsetattr(&stdin, SetArg::TCSANOW, &saved)?;
    nix::sys::termios::tcflush(&stdin, FlushArg::TCIFLUSH)?;
    drained
}

/// Consume what `source` yields until it has nothing now, refusing past `bound` bytes.
fn discard(source: &mut impl BufRead, bound: usize) -> std::io::Result<()> {
    let mut seen = 0;
    while seen <= bound {
        let n = source.fill_buf()?.len();
        if n == 0 {
            return Ok(());
        }
        source.consume(n);
        seen += n;
    }
    Err(std::io::Error::other("typeahead exceeds its bound"))
}

/// Is another line already waiting on stdin, right now (a zero wait)?
#[must_use]
pub fn stdin_pending() -> bool {
    use std::os::fd::AsFd as _;

    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    let stdin = std::io::stdin();
    let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
    matches!(poll(&mut fds, PollTimeout::ZERO), Ok(n) if n > 0)
}

/// A line source that takes its lock INSIDE each read and releases it
/// before returning: the door never holds stdin across a turn, so a run it
/// starts can ask its own gate on the same terminal (`ask_on_tty` locks
/// stdin too — held across the loop, that lock never came back).
pub struct PerCallLines<F> {
    fill: F,
    buf: Vec<u8>,
    pos: usize,
}

impl<F> PerCallLines<F>
where
    F: FnMut(&mut Vec<u8>) -> std::io::Result<usize>,
{
    /// A source over `fill`, which appends one line to the buffer per call.
    pub fn new(fill: F) -> Self {
        Self {
            fill,
            buf: Vec::new(),
            pos: 0,
        }
    }
}

impl<F> std::io::Read for PerCallLines<F>
where
    F: FnMut(&mut Vec<u8>) -> std::io::Result<usize>,
{
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<F> BufRead for PerCallLines<F>
where
    F: FnMut(&mut Vec<u8>) -> std::io::Result<usize>,
{
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos >= self.buf.len() {
            self.buf.clear();
            self.pos = 0;
            (self.fill)(&mut self.buf)?;
        }
        Ok(&self.buf[self.pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.buf.len());
    }
}

#[cfg(all(test, unix))]
mod tests;
