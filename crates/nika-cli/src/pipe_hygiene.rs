// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pipe hygiene (A-4 · F-08): `nika run … | head` used to spill a raw
//! Rust panic (`failed printing to stdout: Broken pipe`) and exit 101
//! once the reader closed. The workspace forbids unsafe code, so the
//! unix reset (`SIGPIPE` → `SIG_DFL`) is out of reach; the equivalent safe
//! seam is the panic plane: the hook keeps the screen clean, the catch
//! in [`guard`] turns the death into 141 — the exact code a
//! SIGPIPE-killed process reports, which is what every unix tool in a
//! pipeline says. Scope is honest: the std print macros on the MAIN
//! thread (the renderer's thread — measured, the F-08 panic lives
//! there). Any other panic re-raises untouched.

/// Run the real entry under the pipe guard: install the silencer, turn
/// a broken-pipe print death into the honest unix exit, re-raise
/// everything else. `panic = "unwind"` is a workspace commitment
/// (Cargo.toml pins it for test panic-catching), so the catch always
/// sees the payload.
pub(crate) fn guard(real_main: fn() -> std::process::ExitCode) -> std::process::ExitCode {
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
