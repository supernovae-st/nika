// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Local terminal evidence and a bounded, explicitly negotiated child conversation.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]
use nika_providers::admission::{CostChallenge, CostResponse};
use std::io::{IsTerminal as _, Read as _, Write as _};

/// A transport choice, never evidence of a decision or a monetary waiver.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub enum ReviewChannel {
    #[default]
    Unavailable,
    Terminal,
    Stdio,
}
impl From<bool> for ReviewChannel {
    fn from(local: bool) -> Self {
        if local {
            Self::Terminal
        } else {
            Self::Unavailable
        }
    }
}
impl ReviewChannel {
    pub(super) fn available(self) -> bool {
        match self {
            Self::Unavailable => false,
            Self::Terminal => std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
            // The opt-in flag is insufficient: actually observe the controlling terminal.
            Self::Stdio => controlling_terminal(),
        }
    }
    pub(super) fn ask(self, challenge: &CostChallenge) -> Result<CostResponse, String> {
        match self {
            Self::Stdio => {
                let frame = serde_json::to_string(challenge).map_err(|e| e.to_string())?;
                let mut out = std::io::stdout().lock();
                writeln!(out, "{frame}")
                    .and_then(|()| out.flush())
                    .map_err(|e| e.to_string())?;
                read_response()
            }
            Self::Terminal => {
                eprintln!("{}", challenge.display());
                let mut answer = fresh_answer()?;
                if answer.trim().eq_ignore_ascii_case("details") {
                    eprintln!("{}", challenge.details());
                    answer = fresh_answer()?;
                }
                Ok(challenge.response(answer.trim().eq_ignore_ascii_case("yes")))
            }
            Self::Unavailable => Err("no interactive Run cost decision channel".into()),
        }
    }
}
/// Discard earlier typeahead before showing the prompt. Typing and display are not atomic.
fn fresh_answer() -> Result<String, String> {
    let (mut answer, mut prompt) = (String::new(), std::io::stderr());
    crate::lines::fresh_terminal(&mut prompt)
        .and_then(|()| write!(prompt, "continue once? › "))
        .and_then(|()| std::io::stdin().read_line(&mut answer))
        .map_err(|e| format!("no fresh terminal answer ({e}); nothing sent"))?;
    Ok(answer)
}
fn controlling_terminal() -> bool {
    #[cfg(unix)]
    {
        std::fs::File::open("/dev/tty").is_ok_and(|tty| tty.is_terminal())
    }
    #[cfg(not(unix))]
    {
        false
    }
}
/// Require one whole document followed by EOF before dispatch. A second document,
/// duplicate keys, oversized input, abandonment or a stalled sender all refuse.
#[cfg(unix)]
fn read_response() -> Result<CostResponse, String> {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd as _;
    let mut input = std::io::stdin();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let mut bytes = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err("Run cost decision expired; nothing sent".into());
        }
        let millis = u16::try_from(left.as_millis().min(1000)).map_err(|e| e.to_string())?;
        let mut fds = [PollFd::new(input.as_fd(), PollFlags::POLLIN)];
        if poll(&mut fds, millis).map_err(|e| e.to_string())? == 0 {
            continue;
        }
        let mut chunk = [0; 1024];
        let size = input.read(&mut chunk).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..size]);
        if bytes.len() > 16_384 {
            return Err("Run cost response exceeds 16 KiB".into());
        }
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid Run cost response: {e}"))
}
#[cfg(not(unix))]
fn read_response() -> Result<CostResponse, String> {
    Err("Run review stdio requires a supported controlling-terminal host".into())
}

#[cfg(all(test, unix))]
mod tests;
