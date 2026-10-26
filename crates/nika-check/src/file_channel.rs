// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The file channel of the content-taint pass ([`super::content_flow`]):
//! a tainted writer leaves untrusted content in a file, and a task that
//! reads the file later carries it on. An `exec:` reads what it likes, so
//! any armed writer reaches it; a builtin read reaches a writer only when
//! the path it reads may be one the writer wrote (NIK-273 follow-up).
//!
//! Paths are compared lexically, as the check has no filesystem: folded
//! with [`nika_cap::lexically_normalize`] and ASCII case (a case-insensitive
//! filesystem makes `Page.txt` and `page.txt` one file). A path the bytes
//! do not fix (a template · a subprocess · a server · a child closure) may
//! be any path. Two paths written from different anchors (the workspace,
//! `/`, the home directory, above the workspace) name the same file only
//! when one ends with the other's segments, since where the workspace sits
//! is unknown statically; a directory reached from another anchor may hold
//! anything. Symlinks stay the runtime's.

use nika_cap::SlotPath;
use nika_schema::raw::RawAction;

/// Where one file effect may land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Spot {
    /// Any path.
    Anywhere,
    /// A lexical path: its anchor and folded segments. `descends`: the
    /// effect reaches below it.
    At {
        anchor: Anchor,
        parts: Vec<String>,
        descends: bool,
    },
}

/// What a lexical path is relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Anchor {
    /// The workflow's own tree (`./…`).
    Workspace,
    /// The filesystem root (`/…`).
    Root,
    /// The home directory (`~/…` · `$HOME/…`).
    Home,
    /// Above the workspace (`../…`): how far is not kept.
    Above,
}

/// One tainted write the channel carries: the origin of the content and
/// where it may have landed.
#[derive(Debug, Clone)]
pub(super) struct Written {
    pub(super) origin: String,
    pub(super) spots: Vec<Spot>,
}

impl Written {
    /// A write that may have landed anywhere.
    pub(super) fn anywhere(origin: String) -> Self {
        Self {
            origin,
            spots: vec![Spot::Anywhere],
        }
    }
}

/// The places a builtin invoke reads files (nika-cap's read slots),
/// empty when it reads none or is not a builtin invoke.
pub(super) fn read_spots(action: &RawAction) -> Vec<Spot> {
    builtin_slots(action, nika_cap::read_slot_paths)
}

/// Where a writer's content may land. A builtin writes its write slots; a
/// subprocess, a server, a child closure or a templated path, anywhere.
pub(super) fn written_spots(action: &RawAction) -> Vec<Spot> {
    let spots = builtin_slots(action, nika_cap::write_slot_paths);
    if spots.is_empty() {
        vec![Spot::Anywhere]
    } else {
        spots
    }
}

/// The first armed write a read of `reads` may meet: its origin.
pub(super) fn first_met<'a>(channel: &'a [Written], reads: &[Spot]) -> Option<&'a String> {
    channel
        .iter()
        .find(|w| w.spots.iter().any(|s| reads.iter().any(|r| may_meet(s, r))))
        .map(|w| &w.origin)
}

/// One builtin invoke's slots through `slots`, as spots.
fn builtin_slots(
    action: &RawAction,
    slots: fn(&str, Option<&serde_json::Value>) -> Vec<SlotPath>,
) -> Vec<Spot> {
    let RawAction::Invoke(inv) = action else {
        return Vec::new();
    };
    let Some(tool) = inv.tool() else {
        return Vec::new();
    };
    if !tool.value.starts_with("nika:") {
        return Vec::new();
    }
    slots(&tool.value, inv.args.as_ref().map(|a| &a.value))
        .iter()
        .map(spot)
        .collect()
}

/// A slot path as a spot.
fn spot(slot: &SlotPath) -> Spot {
    match slot {
        SlotPath::Literal { path, descends } => lexical(path, *descends),
        _ => Spot::Anywhere,
    }
}

/// A literal path folded to its anchor and segments.
fn lexical(path: &str, descends: bool) -> Spot {
    let home = ["${HOME}", "$HOME", "~"]
        .iter()
        .find_map(|prefix| path.strip_prefix(prefix));
    let (anchor, folded) = if let Some(rest) = home {
        let folded = nika_cap::lexically_normalize(&format!("/{rest}"));
        (Anchor::Home, folded)
    } else {
        let folded = nika_cap::lexically_normalize(path);
        let anchor = if folded.starts_with('/') {
            Anchor::Root
        } else if folded.starts_with("..") {
            Anchor::Above
        } else {
            Anchor::Workspace
        };
        (anchor, folded)
    };
    let parts = folded
        .split('/')
        .filter(|seg| !matches!(*seg, "" | "." | ".."))
        .map(str::to_ascii_lowercase)
        .collect();
    Spot::At {
        anchor,
        parts,
        descends,
    }
}

/// May a write at `written` and a read at `read` touch the same file?
fn may_meet(written: &Spot, read: &Spot) -> bool {
    let (
        Spot::At {
            anchor: wa,
            parts: wp,
            descends: wd,
        },
        Spot::At {
            anchor: ra,
            parts: rp,
            descends: rd,
        },
    ) = (written, read)
    else {
        return true;
    };
    if wa == ra && *wa != Anchor::Above {
        return wp == rp || (*wd && rp.starts_with(wp)) || (*rd && wp.starts_with(rp));
    }
    *wd || *rd || wp.ends_with(rp) || rp.ends_with(wp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &str) -> Spot {
        lexical(path, false)
    }

    fn under(path: &str) -> Spot {
        lexical(path, true)
    }

    #[test]
    fn one_anchor_compares_folded_paths() {
        assert!(may_meet(&at("./drop/page.txt"), &at("drop/page.txt")));
        assert!(may_meet(
            &at("./drop/page.txt"),
            &at("./drop/x/../page.txt")
        ));
        assert!(may_meet(&at("./drop/Page.TXT"), &at("./drop/page.txt")));
        assert!(!may_meet(&at("./drop/page.txt"), &at("./drop/other.txt")));
        assert!(may_meet(&at("./drop/page.txt"), &under("./drop")));
        assert!(may_meet(&under("./out"), &at("./out/deep/a.png")));
        assert!(!may_meet(&at("./drop/page.txt"), &under("./drop/archive")));
        assert!(may_meet(&at("/tmp/a"), &at("/tmp/x/../a")));
        assert!(!may_meet(&at("/tmp/a"), &at("/tmp/b")));
    }

    #[test]
    fn two_anchors_meet_when_one_ends_the_other() {
        assert!(may_meet(
            &at("./drop/page.txt"),
            &at("/srv/ws/drop/page.txt")
        ));
        assert!(!may_meet(&at("./drop/page.txt"), &at("/etc/passwd")));
        assert!(may_meet(&at("~/notes/a.md"), &at("/home/me/notes/a.md")));
        assert!(may_meet(&at("../ws/a.txt"), &at("./a.txt")));
        assert!(
            may_meet(&under("/srv"), &at("./a.txt")),
            "a directory may hold it"
        );
    }

    #[test]
    fn an_unfixed_path_meets_everything() {
        assert!(may_meet(&Spot::Anywhere, &at("./a")));
        assert!(may_meet(&at("./a"), &Spot::Anywhere));
    }
}
