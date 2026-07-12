// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika examples list|show` — the embedded corpus as an EXPERIENCE.
//!
//! One source, zero drift: every fact this surface paints is derived
//! from the example FILE ITSELF at call time — the title from its own
//! header comment (`# NN · Title — pitch` · `# showcase · T1 tier ·
//! audience`), the verb chips from a line scan of the task keys, the
//! task count from the `- id:` rows. No engine-side catalog to rot.
//!
//! The listing speaks FULL filenames (`01-hello.nika.yaml`) — what you
//! see is what you type, and the pack's resolver already tolerates the
//! extension both ways. Sober registers (pipes · `--plain`) keep
//! escape-free bytes; the machine surface stays `nika_pack` itself.

use std::fmt::Write as _;

use crate::display::chrome;
use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, exit};

/// What one embedded example says about itself (derived, never stored).
struct ExampleMeta {
    /// The full teaching filename (`01-hello.nika.yaml`).
    file: String,
    /// The header title/pitch line, cleaned (`Hello world — the
    /// smallest useful Nika workflow`).
    title: String,
    /// The verbs the tasks actually use, in the locked order.
    verbs: Vec<&'static str>,
    /// How many tasks the workflow carries.
    tasks: usize,
}

/// Derive the display metadata from one example body.
fn meta(slug: &str, body: &str) -> ExampleMeta {
    ExampleMeta {
        file: format!("{slug}.nika.yaml"),
        title: title_of(slug, body),
        verbs: verbs_of(body),
        tasks: body
            .lines()
            .filter(|l| l.trim_start().starts_with("- id:"))
            .count(),
    }
}

/// The title out of the file's OWN header comments — foundation files
/// carry `# NN · Title — pitch`, showcase files carry the pitch on the
/// first prose comment after the `# showcase · …` tier line. A file
/// without either degrades to an empty title (never invented prose).
fn title_of(slug: &str, body: &str) -> String {
    let number = slug
        .rsplit('/')
        .next()
        .and_then(|s| s.split('-').next())
        .unwrap_or_default();
    let mut saw_tier_line = false;
    for line in body.lines().take(12) {
        let Some(comment) = line.strip_prefix('#') else {
            break; // the header block ended — YAML begins
        };
        let text = comment.trim();
        if text.is_empty() || text.starts_with("SPDX") || text.starts_with("yaml-language-server") {
            continue;
        }
        // Foundation: `01 · Hello world — …` (starts with its number).
        if let Some(rest) = text.strip_prefix(number) {
            return rest.trim_start_matches([' ', '·']).trim().to_owned();
        }
        // Showcase: the tier line first (`showcase · T1 starter · …`),
        // then the pitch line right under it.
        if text.starts_with("showcase") {
            saw_tier_line = true;
            continue;
        }
        if saw_tier_line {
            return text.trim_end_matches('.').to_owned();
        }
        break;
    }
    String::new()
}

/// The verbs a workflow's tasks use — a line scan for the 4 locked verb
/// keys at task-field indentation (`    infer:` …), deduped in the
/// locked presentation order.
fn verbs_of(body: &str) -> Vec<&'static str> {
    let mut found = [false; 4];
    for line in body.lines() {
        let t = line.trim_start();
        for (i, verb) in ["infer:", "exec:", "invoke:", "agent:"].iter().enumerate() {
            if t.starts_with(verb) {
                found[i] = true;
            }
        }
    }
    ["infer", "exec", "invoke", "agent"]
        .into_iter()
        .zip(found)
        .filter_map(|(v, hit)| hit.then_some(v))
        .collect()
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
                    theme.paint(Role::Dim, &m.title),
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
        for slug in members {
            let Some(body) = nika_pack::example(slug) else {
                continue;
            };
            let m = meta(slug, body);
            let _ = writeln!(
                text,
                "{}",
                chrome::rail_line(
                    theme,
                    &format!(
                        " {}  {}{}",
                        theme.paint(Role::Strong, &m.file),
                        chips(&m.verbs, theme),
                        theme.paint(Role::Dim, &m.title),
                    ),
                )
            );
        }
    }

    let _ = write!(
        text,
        "\nnext ·\n  nika examples show 01-hello.nika.yaml            # read one (the extension is optional)\n  nika examples run 01-hello --model mock/echo     # offline proof · zero keys"
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
        "{} {} {}",
        theme.logo(),
        theme.paint(Role::Strong, &m.file),
        theme.paint(
            Role::Dim,
            &format!("— {} · {} task(s){verbs_said}", m.title, m.tasks)
        ),
    );
    let run_hint = crate::display::vocab::hint(
        theme,
        "run",
        &format!("nika examples run {clean} --model mock/echo"),
    );
    VerbOutput::ok(format!("{header}\n\n{body}\n{run_hint}"))
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
