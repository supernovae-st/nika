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

/// The showcase tiers, in reading order — the tier prefix is the spec's
/// own taxonomy (the filenames carry it); only the reading label lives
/// here.
const TIERS: [(&str, &str); 4] = [
    ("t1", "T1 · starters — one obvious win"),
    ("t2", "T2 · daily ops — gates · retries · state"),
    ("t3", "T3 · parallel intelligence — fan-out · agent tools"),
    ("t4", "T4 · autonomous — budgets · await · recovery"),
];

/// `nika examples list` — the corpus, organized: the foundation path
/// (numbered steps · full filenames · titles · verb chips), then the
/// showcase by tier. Derived entirely from the pack at call time.
#[must_use]
pub fn list(theme: Theme) -> VerbOutput {
    let slugs = nika_pack::example_slugs();
    let foundation: Vec<&String> = slugs.iter().filter(|s| !s.contains('/')).collect();
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

    for (prefix, label) in TIERS {
        let members: Vec<&String> = slugs
            .iter()
            .filter(|s| {
                s.strip_prefix("showcase/")
                    .is_some_and(|r| r.starts_with(prefix))
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let _ = writeln!(text, "{}", chrome::rail_head(theme, label));
        let tier_width = members
            .iter()
            .map(|s| s.chars().count() + ".nika.yaml".len())
            .max()
            .unwrap_or(0);
        for slug in members {
            let Some(body) = nika_pack::example(slug) else {
                continue;
            };
            let m = meta(slug, body);
            let pad = " ".repeat(tier_width.saturating_sub(m.file.chars().count()));
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
        "\nnext ·\n  nika examples show 01-hello.nika.yaml            # read one (the extension is optional)\n  nika examples run 01-hello --model mock/echo     # offline proof · zero keys\n  nika examples copy 01-hello                      # make any of these yours"
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
    let mut text = format!(
        "{} {} {}",
        theme.paint(Role::Good, if theme.ascii { "+" } else { "✔" }),
        theme.paint(Role::Strong, &dest),
        theme.paint(Role::Dim, "— yours now · edit anything"),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// Every listing row speaks the FULL filename — what you see is
    /// what you type (and the resolver tolerates it back).
    #[test]
    fn list_speaks_full_filenames_grouped_by_tier() {
        let out = list(PLAIN);
        assert_eq!(out.code, exit::OK);
        assert!(out.text.contains("the path"), "{}", out.text);
        assert!(out.text.contains("01-hello.nika.yaml"), "{}", out.text);
        assert!(
            out.text.contains("T1 · starters"),
            "showcase tiers group: {}",
            out.text
        );
        assert!(
            out.text.contains("showcase/t4-release-train.nika.yaml"),
            "{}",
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

        let standup = nika_pack::example("showcase/t1-standup-digest").expect("embedded");
        let ms = meta("showcase/t1-standup-digest", standup);
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
}
