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
    if crate::verbs::welcome::is_first_wow(from, dest) {
        let path = crate::verbs::welcome::first_wow_dest(dest);
        let out = crate::verbs::welcome::write_first_wow(std::path::Path::new(path), force);
        if out.code == 0 {
            crate::metrics::record_if_enabled(
                crate::metrics::EventKind::DraftCreated,
                crate::metrics::Facts {
                    draft: Some(crate::metrics::DraftSource::New),
                    ..crate::metrics::Facts::none()
                },
            );
        }
        return out;
    }
    let audit = move |path: &str| {
        let v = crate::verbs::check::run(path, false, false, None, theme);
        nika_onboard::Outcome {
            text: v.text,
            code: v.code,
        }
    };
    let out = nika_onboard::guided::dispatch(from, dest, force, theme, &audit);
    stamp_written(&out, dest);
    // W8 metrics: success here means a draft landed on disk — `--from
    // <template|intent>` instantiates directly (the `?` discovery query
    // is the one OK that writes nothing); a bare `nika new` reaches the
    // wizard only interactively (a pipe exits before any write).
    let drafted = match from {
        Some(f) if f != "?" => Some(crate::metrics::DraftSource::New),
        None => Some(crate::metrics::DraftSource::Guided),
        _ => None,
    };
    if out.code == 0
        && let Some(source) = drafted
    {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::DraftCreated,
            crate::metrics::Facts {
                draft: Some(source),
                ..crate::metrics::Facts::none()
            },
        );
    }
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
    stamp_written(&out, dest);
    // W8 metrics: success here means a file landed on disk (the `?`
    // discovery query is the one OK that writes nothing).
    if out.code == 0 && template != "?" {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::DraftCreated,
            crate::metrics::Facts {
                draft: Some(crate::metrics::DraftSource::New),
                ..crate::metrics::Facts::none()
            },
        );
    }
    VerbOutput {
        text: out.text,
        code: out.code,
    }
}

fn stamp_written(out: &nika_onboard::Outcome, dest: Option<&str>) {
    if out.code != 0 {
        return;
    }
    let Some(path) = written_dest(&out.text, dest) else {
        return;
    };
    let _ = crate::verbs::welcome::stamp_cascade_model(&path);
}

fn written_dest(text: &str, dest: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(d) = dest.filter(|d| d.ends_with(".nika.yaml") || d.ends_with(".nika.yml")) {
        return Some(std::path::PathBuf::from(d));
    }
    text.split_whitespace()
        .next()
        .filter(|t| t.ends_with(".nika.yaml") || t.ends_with(".nika.yml"))
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_is_the_first_wow_slug() {
        assert!(crate::verbs::welcome::is_first_wow(Some("hello"), None));
        assert!(crate::verbs::welcome::is_first_wow(
            Some("hello.nika.yaml"),
            None
        ));
        assert!(crate::verbs::welcome::is_first_wow(
            None,
            Some("hello.nika.yaml")
        ));
        assert!(!crate::verbs::welcome::is_first_wow(
            Some("chain"),
            Some("out.nika.yaml")
        ));
        assert!(!crate::verbs::welcome::is_first_wow(Some("01-hello"), None));
        assert_eq!(
            crate::verbs::welcome::first_wow_dest(None),
            "hello.nika.yaml"
        );
        assert_eq!(
            crate::verbs::welcome::first_wow_dest(Some("hello")),
            "hello.nika.yaml"
        );
        assert_eq!(
            crate::verbs::welcome::first_wow_dest(Some("out.nika.yaml")),
            "out.nika.yaml"
        );
    }

    #[test]
    fn written_dest_reads_the_receipt_head() {
        let path = written_dest("hello.nika.yaml ← template `chain`", None);
        assert_eq!(
            path.as_deref(),
            Some(std::path::Path::new("hello.nika.yaml"))
        );
    }
}
