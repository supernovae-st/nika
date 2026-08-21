// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pipe hygiene (A-4 · F-08): main-thread print `BrokenPipe` becomes the
//! standard Unix 141; all other panics re-raise.

use std::io::Write as _;

/// Run the real entry under the pipe guard: install the silencer, turn
/// a broken-pipe print death into the honest unix exit, re-raise
/// everything else. `panic = "unwind"` is a workspace commitment
/// (Cargo.toml pins it for test panic-catching), so the catch always
/// sees the payload.
pub(crate) fn guard(real_main: fn() -> std::process::ExitCode) -> std::process::ExitCode {
    guard_with_stdout(real_main, stdout_is_open())
}

fn guard_with_stdout(
    real_main: fn() -> std::process::ExitCode,
    stdout_open: bool,
) -> std::process::ExitCode {
    if !stdout_open {
        let _ = writeln!(std::io::stderr().lock(), "nika: stdout is unavailable");
        return std::process::ExitCode::from(crate::verbs::exit::ENV);
    }
    install_pipe_panic_silencer();
    match std::panic::catch_unwind(real_main) {
        Ok(code) => code,
        Err(payload) => {
            if is_broken_pipe_payload(payload.as_ref()) {
                std::process::ExitCode::from(141)
            } else {
                std::panic::resume_unwind(payload)
            }
        }
    }
}

#[cfg(unix)]
fn stdout_is_open() -> bool {
    let stdout = std::io::stdout();
    nix::fcntl::fcntl(&stdout, nix::fcntl::FcntlArg::F_GETFL).is_ok()
}

#[cfg(not(unix))]
fn stdout_is_open() -> bool {
    true
}

/// The std print-macro panic for a closed pipe, and nothing else — the
/// message shape is std's own (`failed printing to <stream>: <error>`),
/// so a real defect that merely mentions a pipe never matches.
fn is_broken_pipe_payload(payload: &dyn std::any::Any) -> bool {
    let text = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    text.is_some_and(|s| s.starts_with("failed printing to") && s.contains("Broken pipe"))
}

/// Silence ONLY the broken-pipe print panic (the catch in [`guard`]
/// turns it into the honest exit); every other panic keeps the default
/// hook's full report.
fn install_pipe_panic_silencer() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !is_broken_pipe_payload(info.payload()) {
            default_hook(info);
        }
    }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn unavailable_stdout_refuses_before_main() {
        let code = super::guard_with_stdout(|| std::process::ExitCode::SUCCESS, false);
        assert_eq!(code, std::process::ExitCode::from(crate::verbs::exit::ENV));
    }
}
