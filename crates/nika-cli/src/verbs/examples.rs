// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The showroom listing behind bare `nika try` — the embedded corpus as an EXPERIENCE.
//!
//! One source, zero drift: every fact this surface paints is derived
//! from the example FILE ITSELF at call time — the title from its own
//! header comment (`# NN · Title — pitch` · `# showcase · T1 tier ·
//! audience`), the verb chips from a line scan of the task keys, the
//! task count from the task-map keys. No engine-side catalog to rot.
//!
//! The listing speaks FULL filenames (`01-hello.nika.yaml`) — what you
//! see is what you type, and the pack's resolver already tolerates the
//! extension both ways. Sober registers (pipes · `--plain`) keep
//! escape-free bytes; the machine surface stays `nika_pack` itself.

use std::fmt::Write as _;

use crate::display::chrome;
use crate::display::theme::{Role, Theme};
use crate::verbs::VerbOutput;
use nika_pack::meta;

/// Clip a title to `max` chars on a WORD boundary with the honest
/// ellipsis — a pitch cut mid-clause reads like a bug, not a teaser.
fn clip_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        return title.to_owned();
    }
    let cut: String = title.chars().take(max).collect();
    let head = cut.rsplit_once(' ').map_or(cut.as_str(), |(h, _)| h);
    format!("{}…", head.trim_end_matches(['·', ',', ' ']))
}

/// The 2-cell verb chips for one example (`◇ ▷ ` …), painted.
fn chips(verbs: &[&str], theme: Theme) -> String {
    verbs.iter().map(|v| theme.verb_glyph(v)).collect()
}

/// The language CONSTRUCTS a corpus file can teach, in the order a reader
/// meets them — the axis the corpus is indexed on.
///
/// This replaced a four-tier taxonomy (`t1-`…`t4-`) the filenames used to
/// carry. The tiers were nominally sizes, but read their own labels back:
/// « gates · retries · state », « fan-out · agent tools », « budgets ·
/// await · recovery ». They were never sizes. They were an approximation
/// of the question a reader actually asks — *which file shows me
/// `for_each`?* — and the approximation cost a prefix on every filename
/// plus a fourth word in the vocabulary.
///
/// So the approximation is not renamed, it is REPLACED by what it
/// approximated: the index is derived from the files, at call time, from
/// the embedded pack. Nothing is stored, so nothing can drift.
///
/// One computation serves three needs — grouping the list, routing
/// `--teaches`, and reporting the gaps. A grouping that only decorated a
/// list would not have earned its place.
#[cfg(test)]
const CONSTRUCTS: [(&str, &str); 15] = [
    ("infer:", "ask a model"),
    ("exec:", "run a program"),
    ("invoke:", "call a tool"),
    ("agent:", "a bounded loop"),
    ("for_each:", "fan out over a collection"),
    ("when:", "a skip gate"),
    ("after:", "an explicit edge"),
    ("retry:", "absorb a transient failure"),
    ("on_error:", "recover, or route the failure"),
    ("unwind", "cleanup that always runs"),
    ("schema:", "structured output"),
    ("returns:", "a typed task output"),
    // The ones the corpus does not cover, deliberately listed so the gap
    // is visible rather than absent. Measured 2026-07-29 against the spec
    // prose, which discusses each at length: composition 36 mentions,
    // returns: 47 — and the spec calls `lift:` « the ONLY door » through
    // the permit taint. An author who meets that wall with no example
    // widens a boundary instead.
    //
    // `config:` LEFT this census (2026-08-13): the spec folded it into
    // `inputs:` with `required: false`, so the corpus stopped teaching
    // it. The engine still parses it until that tranche lands — this
    // table follows the CORPUS, which is what it measures, and listing
    // a key the corpus deliberately dropped would report a gap nobody
    // can close.
    ("workflow:", "call another workflow"),
    ("lift:", "the door through a taint"),
    ("inputs:", "a runtime parameter"),
];

/// Which constructs a file body teaches.
///
/// A file teaches a construct by USING it, so the match is on the YAML
/// key at a line start and a comment never counts.
///
/// TWO keys need their nesting checked, and getting this wrong is not a
/// cosmetic miss — it inverts the answer:
///
/// - `workflow:` at column 0 is the retired envelope identity (the
///   name lives on `nika:` since 0.109), not composition. Only an
///   INDENTED one is a call to another workflow. Counted flat, leftover
///   fourteen-key files would read as composition coverage they do not
///   have.
/// - `inputs:` is the same shape: the top-level authority block, not a
///   nested key.
///
/// The rule that generalises: an envelope field and a task field can share
/// a name, and indentation is the only thing that tells them apart.
#[cfg(test)]
fn teaches(body: &str) -> Vec<&'static str> {
    CONSTRUCTS
        .iter()
        .filter(|(key, _)| {
            body.lines().any(|l| {
                let t = l.trim_start();
                // `unwind` is a predicate VALUE, not a key — it is the
                // one construct spelled inside `after:`, so the key-at-
                // line-start rule cannot see it.
                if *key == "unwind" {
                    return !t.starts_with('#') && t.contains("unwind");
                }
                if !t.starts_with(key) || t.starts_with('#') {
                    return false;
                }
                let indented = l.len() > t.len();
                match *key {
                    // nested only — the envelope form is a different thing
                    "workflow:" => indented,
                    // top level only — the authority block
                    "inputs:" => !indented,
                    _ => true,
                }
            })
        })
        .map(|(key, _)| *key)
        .collect()
}

/// The whole corpus indexed by construct — `(construct, label, files)`,
/// including the constructs NO file teaches.
///
/// The empty rows are the point. A construct with no example is, for any
/// reader who learns from examples, a construct the language does not
/// have: measured 2026-07-29, six authors writing from the reference
/// prose alone took 45 check-fix rounds between them and none went green
/// first try, while one who read TWO EXAMPLES wrote their next workflow
/// green in zero. Keeping the gaps in the same computation that serves
/// the list means they cannot be quietly forgotten.
#[cfg(test)]
fn index() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let slugs = nika_pack::example_slugs();
    CONSTRUCTS
        .iter()
        .map(|(key, label)| {
            let files: Vec<String> = slugs
                .iter()
                .filter(|s| nika_pack::example(s).is_some_and(|b| teaches(b).contains(key)))
                .map(|s| meta(s, nika_pack::example(s).unwrap_or_default()).file)
                .collect();
            (*key, *label, files)
        })
        .collect()
}

/// The three storefront jobs — contrasted trades (support · meetings ·
/// dev), each a complete offline rehearsal. The FIRST screen of bare
/// `nika try`: 39 rows at once was a choice tax the gauntlet measured
/// (UX107-13 · D-018 one primary door), and the operator picked these
/// three. `try --all` keeps the whole corpus one flag away, and the
/// concierge still teaches `01-hello` directly — the path never hides.
const STOREFRONT: [&str; 3] = ["support-triage", "meeting-actions", "release-notes"];

/// The slugless-`try` choice: the storefront is a TTY rendering — a
/// pipe gets the full parsable corpus unchanged (the vscode extension
/// runs bare `try` and anchors on `.nika.yaml` rows, a wire contract;
/// the same TTY law every interactive surface here follows), and
/// `--all` forces the shelf on a terminal.
#[must_use]
pub fn shelf_or_front(all: bool, theme: Theme) -> VerbOutput {
    if all || !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        list(theme)
    } else {
        storefront(theme)
    }
}

/// Bare `nika try` — the storefront: three familiar jobs, whole rows
/// (file · what goes in → what comes out · verbs), then the doors to
/// the rest. Derived from the pack at call time; `--all` renders the
/// full corpus (the path + every job).
#[must_use]
pub fn storefront(theme: Theme) -> VerbOutput {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "{}",
        chrome::rail_head(theme, "three jobs to see — offline · zero keys")
    );
    for slug in STOREFRONT {
        let Some(body) = nika_pack::example(slug) else {
            // A storefront slug missing from the pack is a build defect
            // — fall back to the full corpus rather than a bare window.
            return list(theme);
        };
        let m = meta(slug, body);
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_line(
                theme,
                &format!(
                    " {}  {}",
                    theme.paint(Role::Strong, &format!("nika try {slug}")),
                    chips(&m.verbs, theme),
                ),
            )
        );
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_line(
                theme,
                &format!("   {}", theme.paint(Role::Dim, &clip_title(&m.title, 66))),
            )
        );
    }
    let _ = write!(
        text,
        "\nnext ·\n  nika try support-triage              # watch one work · nothing written\n  nika new support-triage              # make it yours (ingredients included)\n  nika new \"describe your job\"         # route your own words to the closest one\n  nika try --all                       # the whole shelf · the 13-step path + every job\n\n{}",
        theme.paint(
            Role::Dim,
            "verbs · \u{25c7} infer (ask a model) · \u{25b7} exec (run a command) · \u{25c6} invoke (use a tool) · \u{2726} agent (bounded loop)"
        )
    );
    VerbOutput::ok(text)
}

/// `nika try --all` — the corpus, organized: the foundation path
/// (numbered steps · full filenames · titles · verb chips), then the
/// showcase by tier. Derived entirely from the pack at call time.
#[must_use]
pub fn list(theme: Theme) -> VerbOutput {
    let slugs = nika_pack::example_slugs();
    // A LESSON is a numbered file, read in order, each introducing one
    // construct. This used to be « has no slash », which worked only while
    // the jobs lived under `showcase/`; once that directory was nuked the
    // filter admitted everything and the path claimed 33 steps. The number
    // is the real signal, and it is the same rule the spec's projector
    // uses to exclude lessons from its use-case targets.
    let is_lesson = |s: &str| {
        s.len() >= 3 && s.as_bytes()[..2].iter().all(u8::is_ascii_digit) && s.as_bytes()[2] == b'-'
    };
    let foundation: Vec<&String> = slugs.iter().filter(|s| is_lesson(s)).collect();
    let mut text = String::new();

    let _ = writeln!(
        text,
        "{}",
        chrome::rail_head(theme, &format!("the path — {} steps", foundation.len()))
    );
    let width = foundation
        .iter()
        .map(|s| s.chars().count() + ".nika.yaml".len())
        .max()
        .unwrap_or(0);
    for slug in &foundation {
        let Some(body) = nika_pack::example(slug) else {
            continue;
        };
        let m = meta(slug, body);
        let pad = " ".repeat(width.saturating_sub(m.file.chars().count()));
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_line(
                theme,
                &format!(
                    " {}{pad}  {}{}",
                    theme.paint(Role::Strong, &m.file),
                    chips(&m.verbs, theme),
                    theme.paint(Role::Dim, &clip_title(&m.title, 58)),
                ),
            )
        );
    }

    // The jobs — every example that is not a numbered lesson — listed once,
    // by name. The tier grouping that used to sit here is gone: it sorted 26
    // files into four buckets a reader could not act on, and the useful
    // question it approximated now has its own verb below.
    let jobs: Vec<&String> = slugs.iter().filter(|s| !foundation.contains(s)).collect();
    if !jobs.is_empty() {
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_head(theme, &format!("the jobs — {} of them", jobs.len()))
        );
        let job_width = jobs
            .iter()
            .map(|s| s.chars().count() + ".nika.yaml".len())
            .max()
            .unwrap_or(0);
        for slug in jobs {
            let Some(body) = nika_pack::example(slug) else {
                continue;
            };
            let m = meta(slug, body);
            let pad = " ".repeat(job_width.saturating_sub(m.file.chars().count()));
            let _ = writeln!(
                text,
                "{}",
                chrome::rail_line(
                    theme,
                    &format!(
                        " {}{pad}  {}{}",
                        theme.paint(Role::Strong, &m.file),
                        chips(&m.verbs, theme),
                        theme.paint(Role::Dim, &clip_title(&m.title, 46)),
                    ),
                )
            );
        }
    }

    let _ = write!(
        text,
        "\nnext ·\n  nika try 01-hello                    # see it work · offline · zero keys\n  nika new 01-hello                    # make it yours (ingredients included)\n  nika new \"describe your job\"         # route your own words to the closest one\n\n{}",
        theme.paint(
            Role::Dim,
            "verbs · \u{25c7} infer (ask a model) · \u{25b7} exec (run a command) · \u{25c6} invoke (use a tool) · \u{2726} agent (bounded loop)"
        )
    );
    VerbOutput::ok(text)
}

/// The `try` seat law (V5 · RAMS-4): offline by default — the mock
/// rehearsal unless a real seat is asked for. `--model self` keeps the
/// example's own `model:` (the file IS the lesson on that seat).
#[must_use]
pub fn rehearsal_seat(model: Option<&str>) -> Option<&str> {
    match model {
        None => Some("mock/echo"),
        Some("self") => None,
        Some(m) => Some(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    /// The storefront (UX107-13 · operator-picked trio): three
    /// contrasted jobs, every taught line names a living door, the
    /// verb legend teaches the glyphs, and `--all` stays one row
    /// away. Every slug in the trio must EXIST in the pack — a
    /// missing one falls back to the full corpus, never a bare
    /// window.
    #[test]
    fn the_storefront_teaches_three_jobs_and_the_shelf_door() {
        for slug in STOREFRONT {
            let body = nika_pack::example(slug).unwrap_or_default();
            assert!(
                !body.is_empty(),
                "storefront slug `{slug}` must exist in the pack"
            );
            // Every ingredient the job reads must ship IN the binary:
            // the rehearsal room stages them beside the workflow, and a
            // storefront row that cannot find its own fixture is the
            // « the tool teaches a command that breaks » class — three
            // of these shipped for one afternoon (gauntlet 08-01,
            // Sofia: two of three jobs exited 1 on a clean floor).
            let embedded: Vec<&str> = nika_pack::example_fixture_files()
                .iter()
                .map(|(tail, _)| *tail)
                .collect();
            for line in body.lines() {
                let Some(rest) = line.split("examples/fixtures/").nth(1) else {
                    continue;
                };
                let wanted: String = rest
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'')
                    .collect();
                let wanted = wanted.trim_end_matches(['*', '/']);
                if wanted.is_empty() {
                    continue;
                }
                assert!(
                    embedded
                        .iter()
                        .any(|t| *t == wanted || t.starts_with(&format!("{wanted}/"))),
                    "storefront `{slug}` reads `{wanted}` — the pack must embed it"
                );
            }
        }
        let out = storefront(Theme::new(false, false, false));
        assert_eq!(out.code, exit::OK);
        for slug in STOREFRONT {
            assert!(
                out.text.contains(&format!("nika try {slug}")),
                "{}",
                out.text
            );
        }
        assert!(out.text.contains("nika try --all"), "{}", out.text);
        assert!(
            out.text.contains("infer (ask a model)"),
            "the verb legend teaches, never assumes: {}",
            out.text
        );
        assert!(
            !out.text.to_lowercase().contains("free"),
            "no free-shaped claim: {}",
            out.text
        );
    }

    /// The full shelf keeps its parsable rows (the vscode extension
    /// anchors on `.nika.yaml` tokens — a wire contract) AND gains the
    /// same verb legend the storefront carries.
    #[test]
    fn the_full_shelf_stays_parsable_and_carries_the_legend() {
        let out = list(Theme::new(false, false, false));
        assert_eq!(out.code, exit::OK);
        assert!(
            out.text.matches(".nika.yaml").count() >= 30,
            "the shelf lists the corpus: {}",
            out.text
        );
        assert!(out.text.contains("infer (ask a model)"), "{}", out.text);
    }

    /// V5 · RAMS-4: bare = mock rehearsal · `self` = the example's own
    /// seat · anything else passes through.
    #[test]
    fn the_rehearsal_seat_is_offline_by_default() {
        assert_eq!(rehearsal_seat(None), Some("mock/echo"));
        assert_eq!(rehearsal_seat(Some("self")), None);
        assert_eq!(rehearsal_seat(Some("ollama/qwen3")), Some("ollama/qwen3"));
    }

    const PLAIN: Theme = Theme::new(false, false, false);

    /// Every listing row speaks the FULL filename — what you see is
    /// what you type (and the resolver tolerates it back).
    #[test]
    fn list_speaks_full_filenames_in_two_groups() {
        let out = list(PLAIN);
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("the path"), "{}", out.text);
        assert!(out.text.contains("01-hello.nika.yaml"), "{}", out.text);
        // Two groups, not five. The tier headings are gone with the
        // prefix that fed them; what a reader wanted from them is
        // derived from the pack at call time rather than declared.
        assert!(
            out.text.contains("the jobs"),
            "the second group: {}",
            out.text
        );
        assert!(
            !out.text.contains("T1 ·") && !out.text.contains("T4 ·"),
            "no tier heading survives: {}",
            out.text
        );
        assert!(
            out.text.contains("release-train.nika.yaml") && !out.text.contains("t4-release-train"),
            "jobs are named, not ranked: {}",
            out.text
        );
        assert!(out.text.contains("next ·"), "the clear path: {}", out.text);
        // Every listed name resolves back through the pack (round-trip).
        for line in out.text.lines() {
            if let Some(idx) = line.find(".nika.yaml") {
                let start = line[..idx].rfind(' ').map_or(0, |i| i + 1);
                let name = &line[start..idx + ".nika.yaml".len()];
                assert!(
                    nika_pack::example(name).is_some(),
                    "listed `{name}` must resolve"
                );
            }
        }
    }

    /// Titles come from the files' OWN headers — foundation numbered
    /// form and showcase pitch form both parse; verbs match the body.
    #[test]
    fn meta_derives_from_the_file_itself() {
        let hello = nika_pack::example("01-hello").expect("embedded");
        let m = meta("01-hello", hello);
        assert!(
            m.title.starts_with("Hello world"),
            "foundation title: {}",
            m.title
        );
        assert_eq!(m.verbs, vec!["infer"], "hello is one infer");
        assert_eq!(m.tasks, 1);

        let standup = nika_pack::example("standup-digest").expect("embedded");
        let ms = meta("standup-digest", standup);
        assert!(
            ms.title.to_lowercase().contains("standup"),
            "showcase pitch line: {}",
            ms.title
        );
        assert!(!ms.verbs.is_empty());
    }

    /// Colour off = zero escapes (the sober register law).
    #[test]
    fn sober_register_stays_escape_free() {
        assert!(!list(PLAIN).text.contains('\x1b'));
    }

    /// The coverage ratchet, constructs leg. Every construct the index
    /// knows must have a corpus file showing it — a construct with no
    /// example is, for an author who learns from examples, a construct
    /// the language does not have (measured: examples beat the prose
    /// reference 8 check-fix rounds to 0). At 16/16 since the four
    /// zero-coverage lessons landed; the 17th key cannot ship uncovered
    /// because this is what refuses.
    #[test]
    fn every_construct_has_a_showcase() {
        let gaps: Vec<&str> = index()
            .into_iter()
            .filter(|(_, _, files)| files.is_empty())
            .map(|(key, _, _)| key)
            .collect();
        assert!(
            gaps.is_empty(),
            "constructs with no example — write the lesson in the same arc as the key: {gaps:?}"
        );
    }

    /// The coverage ratchet, builtins leg — the same gate the kit has
    /// (`the_kit_never_teaches_a_form_the_engine_refuses`), pointed the
    /// other way: the corpus must SHOW what the engine ships. Four ride a
    /// named debt; a 29th builtin cannot join silently, and a debt paid
    /// by a new lesson must be struck from the list in the same arc.
    #[test]
    fn every_builtin_is_shown_or_carries_a_named_debt() {
        // Each entry: why the gap is tolerated TODAY + the showcase owed.
        const OWED: &[(&str, &str)] = &[
            (
                "compose",
                "the agent loop's self-verification intrinsic (ADR-096) — owes \
                 the lesson where a loop checks the workflow it just wrote",
            ),
            (
                "decide",
                "the deterministic decision kernel (spec 11 · W-DEC) — the \
                 costliest gap: an agent that never sees it pays a model call \
                 for an `if`",
            ),
            (
                "inspect",
                "cost · records · dag_info · threads behind one door (ADR-088) \
                 — owes the lesson where a run reads itself",
            ),
            (
                "tts_generate",
                "the audio graduate — a showcase gap, not a logic gap",
            ),
        ];
        let mut bodies = String::new();
        for slug in nika_pack::example_slugs() {
            bodies.push_str(nika_pack::example(&slug).unwrap_or_default());
            bodies.push('\n');
        }
        for name in nika_pack::template_names() {
            bodies.push_str(nika_pack::template(&name).unwrap_or_default());
            bodies.push('\n');
        }
        // Shown = the token `nika:<name>` anywhere in a corpus body, closed
        // on the right so `nika:json_diff` never credits a longer name. A
        // comment counts: naming the tool in a teaching comment still puts
        // it in the reader's reach (recomputed both ways 2026-07-29 — with
        // and without comments the split is the same 24 shown / 4 owed, so
        // the choice is not load-bearing today; it will matter the day a
        // tool is ONLY comment-named, and reach is the honest criterion).
        let shown = |name: &str| {
            let tok = format!("nika:{name}");
            bodies.match_indices(&tok).any(|(i, _)| {
                !bodies[i + tok.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        };
        assert!(
            !nika_catalog::all_builtins().is_empty(),
            "no builtins — the ratchet would pass by vacuity"
        );
        let mut orphans = Vec::new();
        let mut paid = Vec::new();
        for b in nika_catalog::all_builtins() {
            let owed = OWED.iter().any(|(n, _)| *n == b.name);
            match (shown(b.name), owed) {
                (false, false) => orphans.push(b.name),
                (true, true) => paid.push(b.name),
                _ => {}
            }
        }
        assert!(
            orphans.is_empty(),
            "builtins with no corpus showcase and no named debt — add the \
             example or write the debt into OWED: {orphans:?}"
        );
        assert!(
            paid.is_empty(),
            "owed builtins that now HAVE a showcase — tighten the ratchet, \
             strike them from OWED: {paid:?}"
        );
    }

    /// The two sovereign builtin roots — `canon/builtins.yaml` (the
    /// language, projected into the embedded canon.yaml) and
    /// `ALL_BUILTINS` (the engine) — were "kept in step by hand" (SSOT
    /// §5). This is the consumer-side gate that retires the hand: set
    /// equality both ways, so a builtin added to either root without the
    /// other refuses here. The scan reads the generated canon shape
    /// (byte-gated upstream by ssot-compiler), not a YAML parser.
    #[test]
    fn the_two_builtin_roots_agree_at_the_seam() {
        let canon = nika_pack::canon();
        let mut in_block = false;
        let mut in_items = false;
        let mut names: Vec<String> = Vec::new();
        for l in canon.lines() {
            if l == "builtins:" {
                in_block = true;
                continue;
            }
            if !in_block {
                continue;
            }
            if !l.starts_with(' ') && !l.trim().is_empty() {
                break; // next top-level key · the block is over
            }
            if l.trim_start().starts_with("items:") {
                in_items = true;
                continue;
            }
            if in_items {
                if l.trim_start().starts_with('#') {
                    continue; // a comment inside items must not end the scan
                }
                if let Some(n) = l.trim_start().strip_prefix("- ") {
                    names.push(n.trim().to_owned());
                } else if !l.trim().is_empty() {
                    break; // a sibling key after items · the list is over
                }
            }
        }
        assert!(
            !names.is_empty(),
            "canon builtins block not found — the seam scan no longer \
             matches the generated canon shape"
        );
        let canon_set: std::collections::BTreeSet<&str> =
            names.iter().map(String::as_str).collect();
        let engine_set: std::collections::BTreeSet<&str> = nika_catalog::all_builtins()
            .iter()
            .map(|b| b.name)
            .collect();
        let only_canon: Vec<&&str> = canon_set.difference(&engine_set).collect();
        let only_engine: Vec<&&str> = engine_set.difference(&canon_set).collect();
        assert!(
            only_canon.is_empty() && only_engine.is_empty(),
            "the builtin roots drifted — canon-only: {only_canon:?} · \
             engine-only: {only_engine:?}"
        );
    }
}
