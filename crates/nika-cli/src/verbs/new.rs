// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika new` — the composition-root adapter over
//! [`nika_onboard::guided`] (descended with the founding surface at the
//! 15k prod-LOC wall · 2026-07-12). The root injects the ONE effect the
//! guided flow needs — the real audit ladder the wizard's materialize
//! step shows — and re-wraps the outcome.

use crate::display::theme::Theme;
use crate::verbs::VerbOutput;

/// `nika new` — resolve the missing `--from` per clig.dev (a terminal
/// gets the guided flow · a pipe fails fast naming the flag).
#[must_use]
pub fn dispatch(from: Option<&str>, dest: Option<&str>, force: bool, theme: Theme) -> VerbOutput {
    let audit = move |path: &str| {
        let v = crate::verbs::check::run(path, false, false, None, theme);
        nika_onboard::Outcome {
            text: v.text,
            code: v.code,
        }
    };
    let out = nika_onboard::guided::dispatch(from, dest, force, theme, &audit);
    VerbOutput {
        text: out.text,
        code: out.code,
    }
}

/// The flag form (`--from <template|intent> <dest>`) — pure
/// instantiation, no audit involved (the member's own surface).
#[must_use]
pub fn run(template: &str, dest: Option<&str>, force: bool) -> VerbOutput {
    let out = nika_onboard::guided::run(template, dest, force);
    VerbOutput {
        text: out.text,
        code: out.code,
    }
}
