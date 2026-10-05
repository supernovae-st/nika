// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tui` — the terminal renderer of the Nika session (ADR-139).
//!
//! The chat fades, the workflow remains: the session (`nika-session`) is the
//! truth, the presentation law (`nika-tui-core`) derives what a screen may
//! claim, and this crate only PAINTS and LISTENS. Nothing here parses stdout
//! or invents a state; every event is typed on the way in and every human
//! act leaves as a line the session runtime already knows how to read.
//!
//! Three laws hold the shell together:
//!
//! - **One owner of the terminal** ([`terminal`]). Raw mode, bracketed
//!   paste, focus events, the keyboard protocol and the alternate screen are
//!   enabled in one place and restored in reverse in one place, on the
//!   normal exit, on `Ctrl+C`, on a panic and on `SIGTERM`. A broken
//!   terminal after a crash is P0.
//! - **The workspace, inline on request** ([`app`]). The door chooses the
//!   presentation: bare `nika` opens the workspace ([`workspace`]) on the
//!   alternate screen, and `NIKA_TUI=inline` the inline viewport, whose
//!   finished blocks (a proposal accepted, a run's result) are pushed into
//!   the terminal's own scrollback with `Terminal::insert_before`, so history
//!   stays copyable and tmux, SSH and the shell's own scrolling stay
//!   ordinary. `Ctrl+T` moves between them with the draft intact; the focus
//!   presentation stands in where the workspace does not fit.
//! - **A paste is data** ([`composer`]). A pasted `yes`, `run it` or `/quit`
//!   never acts; `Enter` sends, `Alt+Enter` breaks a line, `Up`/`Down` recall
//!   history only at the buffer's edges.
//!
//! Plain and scripted modes are not this crate's: a non-TTY, `TERM=dumb`, a
//! redirected stdout or a machine flag keep the existing line loop of the
//! CLI, untouched.
//!
//! The UX-1 wave ships the shell with a scripted fixture ([`model::Script`])
//! driven by the `nika-tui-proto` binary in both presentations; the real
//! session runtime is wired in the next wave through the same
//! [`model::Beat`] vocabulary.
//!
//! Every colour and glyph comes through [`visual`]: the engine's theme roles
//! resolved at paint time, the workspace icons with their ASCII twins, and the
//! butterfly sampled from the repository's own logomark. The full-terminal
//! workspace ([`workspace`]) composes its geometry, header, project aside,
//! object in view, conversation panel and pinned row in one frame
//! ([`workspace::screen::draw`]), the keyboard moves between them
//! ([`workspace::focus`], routed by the crate-private desk), and the project
//! it shows is the one the conversation lends, read-only
//! ([`model::Conversation::project`]). A workflow opened in the aside shows
//! the faces of the one look the Live host adapter takes of it
//! ([`model::Conversation::inspect`], [`workspace::inspect`]), painted by
//! `nika-tui-view`; the renderer reads no file and grants nothing.

pub mod app;
pub mod composer;
pub mod events;
pub mod model;
pub mod render;
mod scroll;
pub mod session;
pub mod terminal;
pub mod visual;
pub mod workspace;
