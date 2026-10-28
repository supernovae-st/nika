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
//! filesystem makes `Page.txt` and `page.txt` one file). A segment past
//! ASCII may name any segment (Unicode case and normalisation forms fold
//! there too), and `/private/tmp`, `/private/var` and `/private/etc` are
//! `/tmp`, `/var` and `/etc` (the macOS firmlinks). A path the bytes do not
//! fix (a template · a subprocess · a server · a child closure) may be any
//! path. Only `/` sits at a known place: the workspace, the home directory
//! and above the workspace may sit anywhere, so a path from one of them
//! names a rooted file when the rooted path ends with its segments (holds
//! them anywhere, for a directory), and a rooted directory may hold any of
//! them; two paths from two such anchors meet when one ends with the
//! other's segments, or when either is a directory. Other symlinks stay
//! the runtime's.

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

    /// The part of this write the check can locate (its lexical spots),
    /// `None` when it can locate none.
    pub(super) fn located(&self) -> Option<Self> {
        let spots: Vec<Spot> = self
            .spots
            .iter()
            .filter(|spot| matches!(spot, Spot::At { .. }))
            .cloned()
            .collect();
        (!spots.is_empty()).then(|| Self {
            origin: self.origin.clone(),
            spots,
        })
    }
}

/// The places a task reads files: a builtin invoke's read slots (nika-cap's),
/// an `infer:`'s `vision:` files (spec 02 §Vision inputs: a literal path at its
/// spot, a templated one anywhere); empty when it reads none.
pub(super) fn read_spots(action: &RawAction) -> Vec<Spot> {
    if let RawAction::Infer(a) = action {
        return super::permits_fit::vision_files(a)
            .map(|path| {
                if path.value.contains("${{") {
                    Spot::Anywhere
                } else {
                    lexical(&path.value, false)
                }
            })
            .collect();
    }
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

/// The rooted directories macOS reaches through `/private` (firmlinks).
const PRIVATE_FIRMLINKS: [&str; 3] = ["tmp", "var", "etc"];

/// A literal path folded to its anchor and segments.
fn lexical(path: &str, descends: bool) -> Spot {
    // `$HOMEDIR/x` is a workspace directory, not the home directory.
    let home = ["${HOME}", "$HOME", "~"]
        .iter()
        .find_map(|prefix| path.strip_prefix(prefix))
        .filter(|rest| rest.is_empty() || rest.starts_with('/'));
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
    let mut parts: Vec<String> = folded
        .split('/')
        .filter(|seg| !matches!(*seg, "" | "." | ".."))
        .map(str::to_ascii_lowercase)
        .collect();
    if anchor == Anchor::Root
        && let [private, linked, ..] = parts.as_slice()
        && private == "private"
        && PRIVATE_FIRMLINKS.contains(&linked.as_str())
    {
        parts.remove(0);
    }
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
        return same(wp, rp) || (*wd && starts(rp, wp)) || (*rd && starts(wp, rp));
    }
    match (wa, ra) {
        (Anchor::Root, _) => rooted_meets_placed(wp, *wd, rp, *rd),
        (_, Anchor::Root) => rooted_meets_placed(rp, *rd, wp, *wd),
        // Two unknown places: either may sit inside the other's directory.
        _ => *wd || *rd || ends(wp, rp) || ends(rp, wp),
    }
}

/// A rooted spot against one under an anchor whose place `P` is unknown
/// (it names `P` + `placed`): `P` may begin with a rooted directory, so
/// that directory may hold anything; otherwise the rooted path must end
/// with `placed`, or hold its segments anywhere when `placed` is a
/// directory (`P` before them, the file after them).
fn rooted_meets_placed(
    rooted: &[String],
    rooted_descends: bool,
    placed: &[String],
    placed_descends: bool,
) -> bool {
    rooted_descends
        || if placed_descends {
            holds(rooted, placed)
        } else {
            ends(rooted, placed)
        }
}

/// One segment against another: equal, or past ASCII on either side (a
/// filesystem folds Unicode case and normalisation forms the check does
/// not model).
fn segment_meets(a: &str, b: &str) -> bool {
    a == b || !a.is_ascii() || !b.is_ascii()
}

/// Two segment runs that may name the same path.
fn same(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| segment_meets(x, y))
}

/// `path` begins with `head`.
fn starts(path: &[String], head: &[String]) -> bool {
    path.get(..head.len()).is_some_and(|run| same(run, head))
}

/// `path` ends with `tail`.
fn ends(path: &[String], tail: &[String]) -> bool {
    path.len()
        .checked_sub(tail.len())
        .and_then(|from| path.get(from..))
        .is_some_and(|run| same(run, tail))
}

/// `path` holds the run `inner` somewhere.
fn holds(path: &[String], inner: &[String]) -> bool {
    inner.is_empty() || path.windows(inner.len()).any(|run| same(run, inner))
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

    /// A `$HOME`-like name that is not `$HOME` is a workspace directory,
    /// as the runtime creates it (NIK-273 review).
    #[test]
    fn a_home_prefix_needs_a_separator() {
        assert!(may_meet(
            &at("$HOMEDIR/page.txt"),
            &at("./$HOMEDIR/page.txt")
        ));
        assert!(may_meet(&at("~stash/page.txt"), &at("./~stash/page.txt")));
        assert!(may_meet(&at("$HOME/page.txt"), &at("/Users/me/page.txt")));
        assert!(may_meet(&at("${HOME}/a/b"), &at("~/a/b")));
        assert!(may_meet(&at("~"), &at("$HOME")));
    }

    /// A case-insensitive, normalising filesystem folds what ASCII case
    /// does not: a segment past ASCII may name any segment.
    #[test]
    fn a_segment_past_ascii_may_be_any_segment() {
        assert!(may_meet(
            &at("./drop/PAG\u{c9}.txt"),
            &at("./drop/pag\u{e9}.txt")
        ));
        assert!(may_meet(
            &at("./drop/caf\u{e9}.txt"),
            &at("./drop/cafe\u{301}.txt")
        ));
        assert!(!may_meet(&at("./dr\u{f4}le/a.txt"), &at("./drop/b.txt")));
    }

    /// macOS reaches `/tmp`, `/var` and `/etc` through `/private`.
    #[test]
    fn the_private_firmlinks_are_one_place() {
        assert!(may_meet(
            &at("/tmp/x/page.txt"),
            &at("/private/tmp/x/page.txt")
        ));
        assert!(may_meet(&at("/private/var/a"), &at("/var/a")));
        assert!(may_meet(&under("/private/etc"), &at("/etc/hosts")));
        assert!(!may_meet(&at("/private/home/a"), &at("/home/a")));
    }

    /// A directory under an anchor whose place is unknown holds a rooted
    /// file only when its segments run inside the file's (NIK-273 review).
    #[test]
    fn a_placed_directory_holds_a_rooted_file_only_by_its_segments() {
        let rooted = at("/tmp/cache/page.txt");
        assert!(!may_meet(&rooted, &under("./src")));
        assert!(!may_meet(&rooted, &under("~/src")));
        assert!(may_meet(&rooted, &under("./cache")));
        assert!(may_meet(&rooted, &under("./tmp/cache")));
        assert!(may_meet(&rooted, &under(".")));
        assert!(may_meet(&under("./out"), &at("/srv/ws/out/a.png")));
        assert!(!may_meet(&under("./out"), &at("/srv/ws/drop/a.png")));
        assert!(
            may_meet(&under("/srv"), &under("./src")),
            "the workspace may sit in /srv"
        );
    }

    #[test]
    fn an_unfixed_path_meets_everything() {
        assert!(may_meet(&Spot::Anywhere, &at("./a")));
        assert!(may_meet(&at("./a"), &Spot::Anywhere));
    }
}
