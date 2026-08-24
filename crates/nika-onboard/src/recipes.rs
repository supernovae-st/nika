// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Project recipes — curated SETS over the embedded single-file
//! templates. A recipe never carries YAML of its own: every workflow it
//! scaffolds is an embedded template through the same `stamp` the `new`
//! wizard uses (id from the file name · model only where the skeleton
//! takes one), so the own-corpus law (#261 — a fresh scaffold audits
//! clean) is inherited, never re-proven here.

use std::path::Path;

use crate::guided;

/// One project recipe: a name the wizard lists, a tagline, and the
/// `(template, destination, workflow-id)` rows it scaffolds under the
/// project dir. The id is EXPLICIT because the numbered teaching
/// filenames (`01-…`) would derive an id starting with a digit — and a
/// kebab workflow id must open on a letter (the schema's own law).
pub(crate) struct Recipe {
    pub name: &'static str,
    pub tagline: &'static str,
    pub workflows: &'static [(&'static str, &'static str, &'static str)],
}

/// The recipe register (wizard order — the agentic curriculum leads
/// because founding an agentic project is the verb's whole point;
/// `starter` keeps the one-guided-workflow flow; `minimal` is the
/// briefs-only floor, today's `--yes` shape).
pub(crate) const RECIPES: [Recipe; 5] = [
    Recipe {
        name: "agentic",
        tagline: "learn the 4 patterns — chain · fan-out · gate · agent loop",
        workflows: &[
            ("chain", "workflows/01-hello-chain.nika.yaml", "hello-chain"),
            (
                "fanout",
                "workflows/02-parallel-fanout.nika.yaml",
                "parallel-fanout",
            ),
            (
                "gate-and-act",
                "workflows/03-gated-ship.nika.yaml",
                "gated-ship",
            ),
            (
                "agent-loop",
                "workflows/04-agent-loop.nika.yaml",
                "agent-loop",
            ),
        ],
    },
    Recipe {
        name: "starter",
        tagline: "one guided workflow — the three-question flow",
        workflows: &[],
    },
    Recipe {
        name: "ship",
        tagline: "ship safely — human-gated release · docker report",
        workflows: &[
            (
                "human-gated-ship",
                "workflows/01-gated-release.nika.yaml",
                "gated-release",
            ),
            (
                "docker-report",
                "workflows/02-docker-report.nika.yaml",
                "docker-report",
            ),
        ],
    },
    Recipe {
        name: "content",
        tagline: "make content — website brief · media asset pack",
        workflows: &[
            (
                "website-brief",
                "workflows/01-website-brief.nika.yaml",
                "website-brief",
            ),
            (
                "media-asset-pack",
                "workflows/02-media-asset-pack.nika.yaml",
                "media-asset-pack",
            ),
        ],
    },
    Recipe {
        name: "minimal",
        tagline: "briefs only — no workflows",
        workflows: &[],
    },
];

/// Look a recipe up by name (the `--recipe` flag arrives as a clap
/// `ValueEnum` string; the wizard arrives with a menu index — both end
/// here).
pub(crate) fn recipe(name: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.name == name)
}

/// Does any workflow in this recipe take the wizard's model answer
/// (a top-level `model:` line)? Decides whether the model question
/// fires at all — asking and not stamping would promise what the files
/// don't carry (the `new` wizard's own law).
pub(crate) fn takes_model(r: &Recipe) -> bool {
    r.workflows
        .iter()
        .any(|(tpl, _, _)| nika_pack::template(tpl).is_some_and(guided::template_takes_model))
}

/// Materialize a recipe's workflows under `dir` — template VERBATIM
/// through `stamp` (id from the destination stem · model only where the
/// skeleton takes one) — plus the generated `workflows/README.md` index
/// (the curriculum's clear path: what each file teaches, in order, with
/// its check/run commands). An existing file is SKIPPED unless `force`;
/// a write failure is carried, never panicked.
pub(crate) fn scaffold(
    dir: &str,
    r: &Recipe,
    model: Option<&str>,
    force: bool,
) -> Vec<(String, ScaffoldStatus)> {
    let mut out: Vec<(String, ScaffoldStatus)> = r
        .workflows
        .iter()
        .map(|(tpl, rel, id)| {
            let dest = Path::new(dir).join(rel).to_string_lossy().into_owned();
            (dest.clone(), scaffold_one(tpl, &dest, id, model, force))
        })
        .collect();
    if !r.workflows.is_empty() {
        let dest = Path::new(dir)
            .join("workflows/README.md")
            .to_string_lossy()
            .into_owned();
        let status = if Path::new(&dest).exists() && !force {
            ScaffoldStatus::Skipped
        } else {
            match std::fs::write(&dest, readme(r)) {
                Ok(()) => ScaffoldStatus::Created,
                Err(e) => ScaffoldStatus::Failed(e.to_string()),
            }
        };
        out.push((dest, status));
    }
    out
}

/// The curriculum index — GENERATED from the recipe table + each
/// template's own `# TEMPLATE ·` header (one source, marked generated).
fn readme(r: &Recipe) -> String {
    let mut s = format!(
        "# workflows — the `{}` set\n\n> Scaffolded by `nika init` (generated — regenerate by re-running\n> `nika init --recipe {}` with `--force`). {}\n\nRead them in order — each file teaches one pattern; the `# SLOT:`\nlines are yours to fill.\n\n",
        r.name, r.name, r.tagline,
    );
    for (i, (tpl, rel, _)) in r.workflows.iter().enumerate() {
        let tag = nika_pack::template(tpl)
            .map(|b| crate::guided::tagline(tpl, b))
            .unwrap_or_default();
        let file = rel.strip_prefix("workflows/").unwrap_or(rel);
        let _ = std::fmt::Write::write_fmt(
            &mut s,
            format_args!(
                "## {} · {file}\n\n{tag}\n\n```sh\nnika check {rel}\nnika run {rel} --model mock/echo   # offline first — swap the model when ready\n```\n\n",
                i + 1,
            ),
        );
    }
    s.push_str("Every finding teaches: `nika explain NIKA-XXXX`. The full contract\nlives in `AGENTS.md` at the repo root.\n");
    s
}

/// Materialize ONE embedded example under `dir` — VERBATIM (a lesson,
/// complete: no SLOTs, no stamp) — plus the single-entry generated
/// index. The same skip/force law as the recipe sets.
pub(crate) fn scaffold_example(
    dir: &str,
    slug: &str,
    force: bool,
) -> Vec<(String, ScaffoldStatus)> {
    let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    let base = clean.rsplit('/').next().unwrap_or(clean);
    let rel = format!("workflows/{base}.nika.yaml");
    let dest = Path::new(dir).join(&rel).to_string_lossy().into_owned();
    let Some(body) = nika_pack::example(slug) else {
        return vec![(
            dest,
            ScaffoldStatus::Failed(format!(
                "unknown example `{slug}` — bare `nika try` names the embedded set"
            )),
        )];
    };
    let status = if Path::new(&dest).exists() && !force {
        ScaffoldStatus::Skipped
    } else if let Some(parent) = Path::new(&dest).parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        ScaffoldStatus::Failed(e.to_string())
    } else {
        match std::fs::write(&dest, body) {
            Ok(()) => ScaffoldStatus::Created,
            Err(e) => ScaffoldStatus::Failed(e.to_string()),
        }
    };
    let mut out = vec![(dest, status)];
    let readme_dest = Path::new(dir)
        .join("workflows/README.md")
        .to_string_lossy()
        .into_owned();
    let readme_status = if Path::new(&readme_dest).exists() && !force {
        ScaffoldStatus::Skipped
    } else {
        let tag = crate::guided::tagline(clean, body);
        let text = format!(
            "# workflows — founded from an example\n\n> Scaffolded by `nika init` (generated — regenerate by re-running\n> `nika init --example {clean}` with `--force`).\n\n## {base}.nika.yaml\n\n{tag}\n\n```sh\nnika check {rel}\nnika run {rel} --model mock/echo   # offline first — swap the model when ready\n```\n\nEvery finding teaches: `nika explain NIKA-XXXX`. The full contract\nlives in `AGENTS.md` at the repo root.\n"
        );
        match std::fs::write(&readme_dest, text) {
            Ok(()) => ScaffoldStatus::Created,
            Err(e) => ScaffoldStatus::Failed(e.to_string()),
        }
    };
    out.push((readme_dest, readme_status));
    out
}

/// The per-file outcome the report speaks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScaffoldStatus {
    Created,
    Skipped,
    Failed(String),
}

fn scaffold_one(
    tpl: &str,
    dest: &str,
    id: &str,
    model: Option<&str>,
    force: bool,
) -> ScaffoldStatus {
    let Some(body) = nika_pack::template(tpl) else {
        // A recipe naming a template the pack doesn't carry is a table
        // bug — surfaced honestly, pinned by the own-corpus test below.
        return ScaffoldStatus::Failed(format!("unknown template `{tpl}`"));
    };
    if Path::new(dest).exists() && !force {
        return ScaffoldStatus::Skipped;
    }
    let stamp_model = model.filter(|_| guided::template_takes_model(body));
    let mut stamped = guided::stamp(body, id, stamp_model);
    // A recipe file lives under `workflows/` and runs from the REPO root
    // — the skeleton's repo-root README default would miss in a freshly
    // founded (empty) directory, so the scaffold points it at the index
    // THIS scaffold writes: the curriculum's first run summarizes its
    // own curriculum, green in any directory `init` just founded.
    stamped = stamped.replace("\"./README.md\"", "\"./workflows/README.md\"");
    if let Some(parent) = Path::new(dest).parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ScaffoldStatus::Failed(e.to_string());
    }
    match std::fs::write(dest, stamped) {
        Ok(()) => ScaffoldStatus::Created,
        Err(e) => ScaffoldStatus::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every recipe workflow names a template the pack actually embeds —
    /// the table cannot rot silently when the pack set moves.
    #[test]
    fn every_recipe_template_exists_in_the_pack() {
        for r in &RECIPES {
            for (tpl, dest, id) in r.workflows {
                assert!(
                    nika_pack::template(tpl).is_some(),
                    "recipe `{}` names unknown template `{tpl}`",
                    r.name
                );
                assert!(
                    dest.starts_with("workflows/") && dest.ends_with(".nika.yaml"),
                    "recipe `{}` dest `{dest}` breaks the layout law",
                    r.name
                );
                assert!(
                    id.chars().next().is_some_and(|c| c.is_ascii_lowercase()),
                    "recipe `{}` id `{id}` must open on a letter (kebab law)",
                    r.name
                );
            }
        }
    }

    /// The own-corpus law, inherited: every workflow every recipe can
    /// scaffold parses AND checks clean (stamped id · no model or the
    /// offline mock — exactly what the non-interactive path writes).
    #[test]
    fn every_recipe_scaffold_audits_clean() {
        for r in &RECIPES {
            let dir =
                std::env::temp_dir().join(format!("nika-recipe-{}-{}", r.name, std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).expect("mkdir");
            let out = scaffold(dir.to_str().expect("utf8"), r, None, false);
            let readme_rows = usize::from(!r.workflows.is_empty());
            assert_eq!(out.len(), r.workflows.len() + readme_rows);
            for (path, status) in &out {
                assert_eq!(*status, ScaffoldStatus::Created, "{path}");
                if path.ends_with("README.md") {
                    let body = std::fs::read_to_string(path).expect("written");
                    assert!(body.contains("generated"), "the index says what it is");
                    assert!(body.contains("nika check workflows/"), "{body}");
                    continue;
                }
                let body = std::fs::read_to_string(path).expect("written");
                let wf = nika_schema::parse(
                    &body,
                    nika_schema::FileId::new(0),
                    nika_schema::ParseMode::Strict,
                )
                .expect("recipe scaffold parses");
                // The own-corpus law as #1066 amended it: a fresh
                // scaffold may refuse for its OWN unfilled slots (the
                // invitation to fill them) and for nothing else.
                let report = nika_check::check(&wf);
                assert!(
                    report.is_clean()
                        || report.findings.iter().all(|f| f.kind == "slot")
                            && !report.slot_findings.is_empty(),
                    "{path}: a fresh recipe scaffold FAILS its own audit for something \
                     other than its unfilled slots (own-corpus law)"
                );
            }
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// Re-running a recipe skips what exists (the human keeps the hand).
    #[test]
    fn rerun_skips_existing_workflows() {
        let r = recipe("ship").expect("registered");
        let dir = std::env::temp_dir().join(format!("nika-recipe-rerun-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        let d = dir.to_str().expect("utf8");
        scaffold(d, r, None, false);
        let again = scaffold(d, r, None, false);
        assert!(again.iter().all(|(_, s)| *s == ScaffoldStatus::Skipped));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The model question only fires for recipes whose skeletons take a
    /// top-level model — and both kinds exist in the register.
    #[test]
    fn takes_model_splits_the_register() {
        let kinds: Vec<bool> = RECIPES.iter().map(takes_model).collect();
        assert!(kinds.contains(&true), "some recipe takes a model");
        assert!(
            !takes_model(recipe("minimal").expect("registered")),
            "minimal takes none"
        );
    }

    #[test]
    fn agentic_leads_and_covers_the_four_patterns() {
        assert_eq!(RECIPES[0].name, "agentic", "the curriculum leads");
        let templates: Vec<&str> = RECIPES[0].workflows.iter().map(|(t, _, _)| *t).collect();
        assert_eq!(
            templates,
            ["chain", "fanout", "gate-and-act", "agent-loop"],
            "chain → fan-out → gate → agent, the teaching order"
        );
    }
}
