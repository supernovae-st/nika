// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika examples list|show` — the embedded corpus as an EXPERIENCE.
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
use crate::verbs::{VerbOutput, exit};
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
const CONSTRUCTS: [(&str, &str); 16] = [
    ("infer:", "ask a model"),
    ("exec:", "run a program"),
    ("invoke:", "call a tool"),
    ("agent:", "a bounded loop"),
    ("for_each:", "fan out over a collection"),
    ("when:", "a skip gate"),
    ("after:", "an explicit edge"),
    ("retry:", "absorb a transient failure"),
    ("on_error:", "recover, or route the failure"),
    ("on_finally:", "cleanup that always runs"),
    ("schema:", "structured output"),
    ("returns:", "a typed task output"),
    // The four the corpus does not cover, deliberately listed so the gap
    // is visible rather than absent. Measured 2026-07-29 against the spec
    // prose, which discusses each at length: composition 36 mentions,
    // returns: 47, config: 16, declassify: 3 — and the spec calls that
    // last one « the ONLY door » through the permit taint. An author who
    // meets that wall with no example widens a boundary instead.
    ("workflow:", "call another workflow"),
    ("config:", "a value authority"),
    ("declassify:", "the door through a taint"),
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
/// - `workflow:` at column 0 is the ENVELOPE every file carries. Only an
///   INDENTED one is a call to another workflow. Counted flat, composition
///   read as « 33 files » when the true answer is zero, and the corpus
///   would have reported full coverage of the construct it covers least.
/// - `inputs:` is the same shape: the top-level authority block, not a
///   nested key.
///
/// The rule that generalises: an envelope field and a task field can share
/// a name, and indentation is the only thing that tells them apart.
fn teaches(body: &str) -> Vec<&'static str> {
    CONSTRUCTS
        .iter()
        .filter(|(key, _)| {
            body.lines().any(|l| {
                let t = l.trim_start();
                if !t.starts_with(key) || t.starts_with('#') {
                    return false;
                }
                let indented = l.len() > t.len();
                match *key {
                    // nested only — the envelope form is a different thing
                    "workflow:" => indented,
                    // top level only — the authority block
                    "inputs:" | "config:" => !indented,
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

/// `nika examples list` — the corpus, organized: the foundation path
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
        "\nnext ·\n  nika examples show 01-hello.nika.yaml            # read one (the extension is optional)\n  nika examples teaches for_each:                  # which file shows a construct\n  nika examples run 01-hello --model mock/echo     # offline proof · zero keys\n  nika examples copy 01-hello                      # make any of these yours"
    );
    VerbOutput::ok(text)
}

/// `nika examples teaches [construct]` — the corpus indexed by what its
/// files USE, and with no argument, the coverage of that index.
///
/// The measured need this answers: an author who read TWO examples — « the
/// one matching the intent, then the one covering what the first did not »
/// — went from 8 check-fix rounds to 0. A flat list does not answer *which
/// two*; a size-tier does not either. What a reader asks is which file
/// shows a given construct, and that is a question with an exact answer.
///
/// With no argument it prints every construct, gaps included, because a
/// verdict that shows only what it covers claims more than it holds. The
/// empty rows are the corpus telling on itself.
#[must_use]
pub fn teaches_verb(construct: Option<&str>, theme: Theme) -> VerbOutput {
    let idx = index();
    let mut text = String::new();

    if let Some(want) = construct {
        // A bare `for_each` means `for_each:` — the colon is how the spec
        // writes a key, and typing it is not a hazing ritual.
        let key = want.trim_end_matches(':');
        let Some((c, label, files)) = idx
            .iter()
            .find(|(k, _, _)| k.trim_end_matches(':').eq_ignore_ascii_case(key))
        else {
            let known = idx
                .iter()
                .map(|(k, _, _)| *k)
                .collect::<Vec<_>>()
                .join(" · ");
            return VerbOutput {
                text: format!("examples: `{want}` is not an indexed construct\n  known: {known}\n"),
                code: crate::verbs::exit::ENV,
            };
        };
        if files.is_empty() {
            let _ = write!(
                text,
                "{}\n  {}\n",
                chrome::rail_head(theme, &format!("{c} — {label}")),
                theme.paint(
                    Role::Bad,
                    "no example teaches this · the corpus does not cover it yet"
                )
            );
            return VerbOutput::ok(text);
        }
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_head(theme, &format!("{c} — {label} · {} files", files.len()))
        );
        for f in files {
            let _ = writeln!(
                text,
                "{}",
                chrome::rail_line(theme, &format!(" {}", theme.paint(Role::Strong, f)))
            );
        }
        return VerbOutput::ok(text);
    }

    let covered = idx.iter().filter(|(_, _, f)| !f.is_empty()).count();
    let _ = writeln!(
        text,
        "{}",
        chrome::rail_head(
            theme,
            &format!("constructs — {covered} of {} covered", idx.len())
        )
    );
    let width = idx
        .iter()
        .map(|(k, _, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    for (key, label, files) in &idx {
        let pad = " ".repeat(width.saturating_sub(key.chars().count()));
        let right = if files.is_empty() {
            theme.paint(Role::Bad, "no example — not covered")
        } else {
            theme.paint(Role::Dim, &format!("{} files · {label}", files.len()))
        };
        let _ = writeln!(
            text,
            "{}",
            chrome::rail_line(
                theme,
                &format!(" {}{pad}  {right}", theme.paint(Role::Strong, key))
            )
        );
    }
    let _ = write!(
        text,
        "\nnext ·\n  nika examples teaches for_each:   # the files that show one construct\n"
    );
    VerbOutput::ok(text)
}

/// `nika examples show <slug>` — the anatomy header, then the file
/// VERBATIM (the comments are the teaching), then the next move.
#[must_use]
pub fn show(slug: &str, theme: Theme) -> VerbOutput {
    let Some(body) = nika_pack::example(slug) else {
        return VerbOutput {
            text: format!("unknown example `{slug}` — `nika examples list` names the embedded set"),
            code: exit::FILE,
        };
    };
    let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    let m = meta(clean, body);
    let verbs_said = if m.verbs.is_empty() {
        String::new()
    } else {
        format!(" · {}", m.verbs.join(" · "))
    };
    let header = format!(
        "{} {}\n  {}",
        theme.logo(),
        theme.paint(Role::Strong, &m.file),
        theme.paint(
            Role::Dim,
            &format!(
                "{} · {} task(s){verbs_said}",
                clip_title(&m.title, 72),
                m.tasks
            )
        ),
    );
    let run_hint = crate::display::vocab::hint(
        theme,
        "run",
        &format!("nika examples run {clean} --model mock/echo"),
    );
    VerbOutput::ok(format!("{header}\n\n{body}\n{run_hint}"))
}

/// `nika examples copy <slug> [dest]` — take the lesson home: the
/// embedded example lands as YOUR file, ready to edit and run. The
/// showroom stays side-effect-free (`run` stages to a temp file); this
/// is the one deliberate "make it yours" gesture, and it says the next
/// two steps. Refuses to overwrite without `--force`.
pub fn copy(slug: &str, dest: Option<&str>, force: bool, theme: Theme) -> VerbOutput {
    let Some(body) = nika_pack::example(slug) else {
        return VerbOutput {
            text: format!("unknown example `{slug}` — `nika examples list` names the embedded set"),
            code: exit::FILE,
        };
    };
    let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    // `showcase/t2-support-triage` lands as `t2-support-triage.nika.yaml`
    // — the file joins YOUR flat workspace, the corpus tiering stays in
    // the pack.
    let base = clean.rsplit('/').next().unwrap_or(clean);
    let dest = dest.map_or_else(|| format!("{base}.nika.yaml"), str::to_owned);
    let path = std::path::Path::new(&dest);
    if path.exists() && !force {
        return VerbOutput {
            text: format!("{dest} already exists — `--force` overwrites, or pick another name"),
            code: exit::FILE,
        };
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return VerbOutput {
            text: format!(
                "nika examples copy: cannot create `{}`: {e}",
                parent.display()
            ),
            code: exit::ENV,
        };
    }
    if let Err(e) = std::fs::write(path, body) {
        return VerbOutput {
            text: format!("nika examples copy: cannot write `{dest}`: {e}"),
            code: exit::ENV,
        };
    }
    // The ingredients ride with the recipe (gauntlet 2026-07-31 · the one
    // rage-quit): every `examples/fixtures/…` path the body reads is
    // materialized beside the copy, at the exact relative path the yaml
    // names. Files already yours are never clobbered.
    let fixtures = materialize_fixtures(body, path);
    let mut text = format!(
        "{} {} {}",
        theme.paint(Role::Good, if theme.ascii { "+" } else { "✔" }),
        theme.paint(Role::Strong, &dest),
        theme.paint(Role::Dim, "— yours now · edit anything"),
    );
    match fixtures {
        Ok((0, 0)) => {}
        Ok((written, kept)) => {
            let mut note = format!(
                "{} {}",
                theme.paint(Role::Good, if theme.ascii { "+" } else { "✔" }),
                theme.paint(
                    Role::Dim,
                    &format!(
                        "examples/fixtures · {} (the recipe's ingredients)",
                        crate::text::count(written, "file")
                    )
                ),
            );
            if kept > 0 {
                let _ = write!(
                    note,
                    " {}",
                    theme.paint(Role::Dim, &format!("· {kept} already yours, kept"))
                );
            }
            let _ = write!(text, "\n{note}");
        }
        Err(e) => {
            return VerbOutput {
                text: format!("nika examples copy: cannot write a fixture: {e}"),
                code: exit::ENV,
            };
        }
    }
    let _ = write!(
        text,
        "\n{}",
        crate::display::vocab::hint(
            theme,
            "next",
            &format!("nika check {dest} · then: nika run {dest}")
        )
    );
    // No agent briefs beside the new file → the founding door, once.
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let briefed = ["CLAUDE.md", "AGENTS.md"]
        .iter()
        .any(|b| dir.map_or_else(|| std::path::Path::new(b).exists(), |d| d.join(b).exists()));
    if !briefed {
        let _ = write!(
            text,
            "\n{}",
            crate::display::vocab::hint(theme, "found a home for it", "nika init")
        );
    }
    VerbOutput::ok(text)
}

/// The `examples/fixtures/<tail>` references a body reads, each tail cut
/// at its first glob star (a `photos/**` read pulls the whole `photos/`
/// dir) and stripped of a trailing `/`.
fn fixture_prefixes(body: &str) -> Vec<String> {
    const MARK: &str = "examples/fixtures/";
    let mut out: Vec<String> = Vec::new();
    for (i, _) in body.match_indices(MARK) {
        let tail: String = body[i + MARK.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '*'))
            .collect();
        let cut = tail.split('*').next().unwrap_or("");
        let cut = cut.trim_end_matches('/');
        if !cut.is_empty() && !out.iter().any(|p| p == cut) {
            out.push(cut.to_owned());
        }
    }
    out
}

/// Write the pack fixtures a body reads beside the copied recipe —
/// under `<dest dir>/examples/fixtures/…`, the exact relative path the
/// yaml names. Returns (written, kept-because-existing).
fn materialize_fixtures(body: &str, dest: &std::path::Path) -> std::io::Result<(usize, usize)> {
    let prefixes = fixture_prefixes(body);
    if prefixes.is_empty() {
        return Ok((0, 0));
    }
    let base = dest.parent().unwrap_or_else(|| std::path::Path::new(""));
    let (mut written, mut kept) = (0, 0);
    for (tail, bytes) in nika_pack::example_fixture_files() {
        let wanted = prefixes
            .iter()
            .any(|p| tail == p || tail.starts_with(&format!("{p}/")));
        if !wanted {
            continue;
        }
        let target = base.join("examples").join("fixtures").join(tail);
        if target.exists() {
            kept += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
        written += 1;
    }
    Ok((written, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// Gauntlet 2026-07-31 (the one rage-quit): a copied recipe brings
    /// its ingredients — the fixtures its body reads land beside it at
    /// the exact relative paths the yaml names, and a file already
    /// yours is kept, never clobbered.
    #[test]
    fn copy_brings_the_fixtures_the_body_reads() {
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("review.nika.yaml");
        let out = copy(
            "06-code-review",
            Some(dest.to_str().expect("utf8")),
            false,
            PLAIN,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        let fixture = dir.path().join("examples/fixtures/review-me.rs");
        assert!(fixture.exists(), "the ingredient landed: {}", out.text);
        assert!(
            out.text.contains("examples/fixtures"),
            "the note names the ingredients: {}",
            out.text
        );

        // Second copy over the same dir: the fixture is YOURS now — kept.
        std::fs::write(&fixture, "mine").expect("user edit");
        let again = copy(
            "06-code-review",
            Some(dir.path().join("review2.nika.yaml").to_str().expect("utf8")),
            false,
            PLAIN,
        );
        assert_eq!(again.code, exit::OK, "{}", again.text);
        assert_eq!(
            std::fs::read_to_string(&fixture).expect("read"),
            "mine",
            "an existing fixture is never clobbered"
        );
        assert!(again.text.contains("already yours"), "{}", again.text);
    }

    /// A dir-glob read (`photos/**`) pulls the whole dir; plain files
    /// come one by one; a starless body brings nothing.
    #[test]
    fn fixture_prefixes_cut_globs_and_dedup() {
        let body = "a: examples/fixtures/sales.csv\nb: \"./examples/fixtures/photos/**\"\nc: examples/fixtures/sales.csv\n";
        assert_eq!(fixture_prefixes(body), vec!["sales.csv", "photos"]);
        assert!(fixture_prefixes("no refs here").is_empty());
    }

    /// Every listing row speaks the FULL filename — what you see is
    /// what you type (and the resolver tolerates it back).
    #[test]
    fn list_speaks_full_filenames_in_two_groups() {
        let out = list(PLAIN);
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("the path"), "{}", out.text);
        assert!(out.text.contains("01-hello.nika.yaml"), "{}", out.text);
        // Two groups, not five. The tier headings are gone with the
        // prefix that fed them; what a reader wanted from them lives in
        // `nika examples teaches` and is derived rather than declared.
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

    /// `copy` writes the embedded body verbatim, names the next two
    /// steps, refuses a silent overwrite, and flattens showcase paths.
    #[test]
    fn copy_takes_the_lesson_home() {
        let dir = std::env::temp_dir().join(format!("nika-copy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dest = dir.join("mine.nika.yaml");
        let dest_s = dest.to_string_lossy().into_owned();

        let out = copy("01-hello", Some(&dest_s), false, PLAIN);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        let body = std::fs::read_to_string(&dest).expect("written");
        assert_eq!(body, nika_pack::example("01-hello").expect("embedded"));
        assert!(out.text.contains("yours now"), "{}", out.text);
        assert!(
            out.text.contains(&format!("nika check {dest_s}")),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("nika init"),
            "no briefs beside it → the founding door"
        );

        // Refuse the silent overwrite; --force allows it.
        let refused = copy("01-hello", Some(&dest_s), false, PLAIN);
        assert_eq!(refused.code, exit::FILE);
        assert!(refused.text.contains("--force"), "{}", refused.text);
        assert_eq!(copy("01-hello", Some(&dest_s), true, PLAIN).code, exit::OK);

        // A showcase slug flattens to its basename (default dest shape).
        let unknown = copy("nope", None, false, PLAIN);
        assert_eq!(unknown.code, exit::FILE);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `show` accepts slug AND filename, frames the anatomy, and keeps
    /// the body VERBATIM (the teaching comments survive).
    #[test]
    fn show_frames_and_keeps_the_body_verbatim() {
        let by_slug = show("01-hello", PLAIN);
        let by_file = show("01-hello.nika.yaml", PLAIN);
        assert_eq!(by_slug.code, exit::OK);
        assert_eq!(by_slug.text, by_file.text, "extension-tolerant");
        let body = nika_pack::example("01-hello").expect("embedded");
        assert!(by_slug.text.contains(body), "verbatim body");
        assert!(by_slug.text.contains("1 task(s)"), "{}", by_slug.text);
        assert!(
            by_slug.text.contains("run: nika examples run 01-hello"),
            "{}",
            by_slug.text
        );
        assert_eq!(show("nope", PLAIN).code, exit::FILE);
    }

    /// Colour off = zero escapes (the sober register law).
    #[test]
    fn sober_register_stays_escape_free() {
        assert!(!list(PLAIN).text.contains('\x1b'));
        assert!(!show("01-hello", PLAIN).text.contains('\x1b'));
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
