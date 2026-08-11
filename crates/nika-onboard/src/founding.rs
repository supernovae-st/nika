// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init [dir]` — found a repo for Nika workflows.
//!
//! Two doors, one law. **Bare on a terminal** the founding wizard runs
//! (`wizard.rs` — recipe · model · canvas · agents · scaffold · proof ·
//! the ready panel). **Any flag, `--yes`, a pipe, or CI** is the
//! scriptable twin: `--recipe` scaffolds a workflow set, `--theme`
//! stamps the VS Code DAG skin, `--wire` connects agent clients — and
//! plain `--yes` keeps the historical report byte-for-byte.
//!
//! The human keeps the hand everywhere: an existing file is SKIPPED,
//! never clobbered — `--force` is the explicit override (same law as
//! `nika new`). A write failure is the one environment error (`exit 3`).
//! The one append-maybe surface is `.gitignore` (`crate::gitignore` —
//! adds-only: the trace-cover section joins an existing file, never a
//! rewrite, and a second run adds nothing).

use std::fmt::Write as _;
use std::path::Path;

use crate::recipes::{self, ScaffoldStatus};
use crate::{Audit, Outcome, Wire, briefs, codes, gitignore};

pub use briefs::agents_md;

/// The `--recipe` vocabulary for clap (`value_parser`) — pinned against
/// the register by test so the two can never drift.
pub const RECIPE_NAMES: [&str; 5] = ["agentic", "starter", "ship", "content", "minimal"];

/// The `--theme` vocabulary — `nika.dag.theme`'s own enum (the VS Code
/// extension's canvas skin), stamped into `.vscode/settings.json`. The
/// composition root mirrors this as its clap `ValueEnum`; here it stays
/// plain (no CLI-framework dependency below the root).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTheme {
    /// The brand skin — engineered black · verb hues.
    Nika,
    /// Adaptive — follows the editor's colors.
    Editor,
    /// Terminal green.
    Phosphor,
    /// Let the extension decide.
    Auto,
}

impl CanvasTheme {
    /// The wire word `nika.dag.theme` speaks.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nika => "nika",
            Self::Editor => "editor",
            Self::Phosphor => "phosphor",
            Self::Auto => "auto",
        }
    }
}

/// What `init` does (or declines to do) for one target file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Write `body` to `path` (it is absent, or `--force`).
    Create { path: String, body: &'static str },
    /// Leave `path` untouched — it already exists (`--force` to overwrite).
    Skip { path: String },
}

/// PURE plan over an injected existence oracle — `Create` an absent file (or
/// any when `force`), `Skip` an existing one. Testable without the filesystem.
pub(crate) fn plan(dir: &str, exists: &dyn Fn(&str) -> bool, force: bool) -> Vec<Action> {
    briefs::targets()
        .into_iter()
        .map(|(rel, body)| {
            let path = join(dir, rel);
            if !force && exists(&path) {
                Action::Skip { path }
            } else {
                Action::Create { path, body }
            }
        })
        .collect()
}

/// Join a base dir and a relative path the same way the apply step will.
fn join(dir: &str, rel: &str) -> String {
    Path::new(dir).join(rel).to_string_lossy().into_owned()
}

/// Render the report (✔ created · · skipped · ✖ write error) — the
/// HISTORICAL byte shape scripts have parsed since #158; the wizard has
/// its own themed register.
pub(crate) fn render(lines: &[(char, String)]) -> String {
    let mut s = String::new();
    for (glyph, msg) in lines {
        let _ = writeln!(s, "{glyph} {msg}");
    }
    s
}

/// The beginner's next move · init used to end SILENTLY (the 2026-07-05
/// beginner walk: « you init and… sit there ») — an onboarding surface
/// must hand over to the next command. Golden path: offline proof in
/// 10s → scaffold → audit-before-tokens. Byte-stable: this is the exact
/// non-interactive shape scripts have seen since #158.
pub(crate) const NEXT_BLOCK: &str = "next ·\n  nika try 01-hello   # offline proof · zero keys\n  nika new                                       # your first workflow — guided on a terminal\n  nika new chain my-first.nika.yaml       # the same, scriptable\n  nika check my-first.nika.yaml                  # audit before a single token";

/// The scriptable path — briefs report (historical bytes), then each
/// flagged extra as its own receipt block, then the hand-off. The door
/// logic (bare-TTY → the wizard) lives at the composition root; this
/// crate exposes the two paths and the root routes.
#[must_use]
pub fn scripted_run(
    dir: &str,
    force: bool,
    recipe: Option<&str>,
    example: Option<&str>,
    canvas: Option<CanvasTheme>,
    wires: &[&str],
    audit: &Audit<'_>,
    wire: &Wire<'_>,
) -> Outcome {
    // One founding source at a time — the clap surface already refuses
    // the pair; a direct lib call gets the same honesty.
    if recipe.is_some() && example.is_some() {
        return Outcome {
            text: "pass --recipe OR --example, not both — one founding source".to_owned(),
            code: codes::ENV,
        };
    }
    let rows = apply_briefs(dir, force, canvas);
    // The trace cover rides the same report — adds-only
    // (`gitignore.rs`), so a re-run or the human's own entry is a calm
    // skip row, and its write failure is the same exit-3 class as a
    // brief's.
    let git = gitignore::ensure(dir);
    let failed = rows
        .iter()
        .any(|(_, o)| matches!(o, BriefOutcome::Failed(_)))
        || matches!(git.1, gitignore::Outcome::Failed(_));
    // The HISTORICAL report bytes (joined paths · ✔/·/✖ rows) — scripts
    // have parsed this shape since #158. The gitignore row is ADDITIVE:
    // one more row after the brief rows, no historical byte changes.
    let mut lines: Vec<(char, String)> = rows
        .iter()
        .map(|(path, outcome)| match outcome {
            BriefOutcome::Created => ('✔', format!("created {path}")),
            BriefOutcome::Skipped => (
                '·',
                format!("skipped {path} (exists · --force to overwrite)"),
            ),
            BriefOutcome::Failed(e) => ('✖', format!("{path}: {e}")),
        })
        .collect();
    lines.push(gitignore::report(&git.0, &git.1));
    let mut text = render(&lines);
    if failed {
        return Outcome::env(text);
    }

    let (first_workflow, worst) =
        match found_from_source(dir, force, recipe, example, audit, &mut text) {
            Ok(pair) => pair,
            Err(out) => return out,
        };

    // `starter`'s workflow step IS the three-question conversation, so its
    // template set is empty by design (`recipes::RECIPES`) and a scripted
    // run scaffolds none. Measured 2026-08-03: `--recipe starter -y` then
    // lands byte-identical to `--recipe minimal`, output included — a flag
    // that silently becomes another flag. The wiring did land; say which
    // half a pipe cannot deliver, and where the twin lives.
    if recipe == Some("starter") && first_workflow.is_none() {
        let _ = writeln!(
            text,
            "· starter's workflow step is a conversation — a script cannot answer it. \
             The wiring landed; run bare `nika init` on a terminal for the questions, \
             or `nika new \"<your job in plain words>\" <file>.nika.yaml` for the twin."
        );
    }

    for line in wire_receipts(dir, wires, wire) {
        let _ = writeln!(text, "{line}");
    }

    let next = first_workflow.map_or_else(
        || NEXT_BLOCK.to_owned(),
        |first| {
            format!(
                "next ·\n  nika run {first} --model mock/echo   # offline proof · zero keys\n  nika check {first}                    # audit before a single token\n  nika explain <NIKA-XXXX>              # every finding teaches"
            )
        },
    );
    Outcome {
        text: format!("{text}\n{next}"),
        code: worst,
    }
}

/// Resolve the ONE founding source into a scaffold set, speak its
/// report, and hand back the first workflow + the worst audit code.
///
/// The example lane (a verbatim lesson) and a recipe (a template set)
/// are two doors to the same ladder — the report and proof below are
/// byte-identical between them. Neither door taken (plain `--yes`) is
/// the historical shape: briefs only, nothing scaffolded.
///
/// `Err` carries the honest refusal an unknown recipe earns on a direct
/// lib call — clap's `value_parser` guards the CLI door, not this one.
fn found_from_source(
    dir: &str,
    force: bool,
    recipe: Option<&str>,
    example: Option<&str>,
    audit: &Audit<'_>,
    text: &mut String,
) -> Result<(Option<String>, u8), Outcome> {
    let scaffolded = match (example, recipe) {
        (Some(slug), _) => recipes::scaffold_example(dir, slug, force),
        (None, Some(name)) => {
            let Some(r) = recipes::recipe(name) else {
                return Err(Outcome {
                    text: format!(
                        "unknown recipe `{name}` — the register: {}",
                        RECIPE_NAMES.join(" · ")
                    ),
                    code: codes::FILE,
                });
            };
            recipes::scaffold(dir, r, None, force)
        }
        (None, None) => return Ok((None, codes::OK)),
    };
    scaffold_report(dir, &scaffolded, audit, text)
}

/// Speak one scaffold set's report bytes into `text` (✔/·/✖ rows —
/// the shape scripts parse) and run the proof ladder over what was
/// created. Shared verbatim by the recipe and example lanes.
fn scaffold_report(
    dir: &str,
    scaffolded: &[(String, ScaffoldStatus)],
    audit: &Audit<'_>,
    text: &mut String,
) -> Result<(Option<String>, u8), Outcome> {
    let mut created: Vec<String> = Vec::new();
    for (path, status) in scaffolded {
        let rel = rel_to(dir, path);
        match status {
            ScaffoldStatus::Created => {
                let _ = writeln!(text, "✔ created {rel}");
                // The proof ladder audits WORKFLOWS — the generated
                // index rides the report but never the check.
                if path.ends_with(".nika.yaml") {
                    created.push(path.clone());
                }
            }
            ScaffoldStatus::Skipped => {
                let _ = writeln!(text, "· skipped {rel} (exists · --force to overwrite)");
            }
            ScaffoldStatus::Failed(e) => {
                return Err(Outcome::env(format!("{text}✖ {rel}: {e}\n")));
            }
        }
    }
    let first = created.first().map(|p| rel_to(dir, p));
    let mut worst = codes::OK;
    for (line, code) in proof_receipts(dir, &created, audit) {
        worst = worst.max(code);
        let _ = writeln!(text, "{line}");
    }
    Ok((first, worst))
}

/// What one brief write came to — the registers compose their own
/// message shapes over it (scripted keeps the historical joined-path
/// bytes · the wizard rail speaks project-relative).
pub(crate) enum BriefOutcome {
    Created,
    Skipped,
    Failed(String),
}

/// Write the briefs per `plan`, honoring the canvas stamp on a CREATED
/// settings file.
pub(crate) fn apply_briefs(
    dir: &str,
    force: bool,
    canvas: Option<CanvasTheme>,
) -> Vec<(String, BriefOutcome)> {
    let plan = plan(dir, &|p| Path::new(p).exists(), force);
    let mut rows: Vec<(String, BriefOutcome)> = Vec::new();
    for action in plan {
        match action {
            Action::Skip { path } => rows.push((path, BriefOutcome::Skipped)),
            Action::Create { path, body } => {
                let themed;
                let body = match canvas {
                    Some(c) if path.ends_with(".vscode/settings.json") => {
                        themed = themed_settings(c);
                        themed.as_str()
                    }
                    _ => body,
                };
                let outcome = match write_file(&path, body) {
                    Ok(()) => BriefOutcome::Created,
                    Err(e) => BriefOutcome::Failed(e.to_string()),
                };
                rows.push((path, outcome));
            }
        }
    }
    rows
}

/// The schema-wiring settings body with `nika.dag.theme` stamped in —
/// parsed and re-emitted (never string-spliced), so the wiring survives
/// any future shape of the const.
fn themed_settings(canvas: CanvasTheme) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(briefs::vscode_settings()).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "nika.dag.theme".to_owned(),
            serde_json::Value::String(canvas.as_str().to_owned()),
        );
    }
    let mut body = serde_json::to_string_pretty(&value).unwrap_or_default();
    body.push('\n');
    body
}

/// Audit every scaffolded workflow NOW — the ladder inside the first
/// minute is the product's argument. Clean collapses to one receipt
/// line; findings expand to the full report (the vitest law: collapse
/// success, expand failure).
pub(crate) fn proof_receipts(
    dir: &str,
    created: &[impl AsRef<str>],
    audit: &Audit<'_>,
) -> Vec<(String, u8)> {
    created
        .iter()
        .map(|path| {
            let path = path.as_ref();
            let audit = audit(path);
            let rel = rel_to(dir, path);
            if audit.code == codes::OK {
                let tail = audit
                    .text
                    .lines()
                    .rev()
                    .find(|l| l.contains("audited"))
                    .map_or_else(|| "audited clean".to_owned(), |l| l.trim().to_owned());
                (format!("  {tail} ← {rel}"), codes::OK)
            } else {
                (
                    format!("{}\n✖ {rel} — findings above", audit.text.trim_end()),
                    audit.code,
                )
            }
        })
        .collect()
}

/// Connect the picked agent clients through the REAL `wire` verb —
/// each client's own receipt, indented under one header.
pub(crate) fn wire_receipts(dir: &str, wires: &[&str], wire: &Wire<'_>) -> Vec<String> {
    if wires.is_empty() {
        return Vec::new();
    }
    let mut lines = vec!["wired ·".to_owned()];
    for client in wires {
        let out = wire(client, dir);
        for l in out.text.lines() {
            lines.push(format!("  {l}"));
        }
    }
    lines
}

/// A path relative to the project dir when it nests there.
fn rel_to(dir: &str, path: &str) -> String {
    Path::new(path)
        .strip_prefix(dir)
        .map_or_else(|_| path.to_owned(), |p| p.to_string_lossy().into_owned())
}

/// Create any missing parent dirs, then write the file. Shell scripts
/// (the scaffolded hooks) get the exec bit on unix — Cursor spawns them
/// directly; on Windows bash hooks fail open anyway, so nothing to set.
fn write_file(path: &str, body: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)?;
    #[cfg(unix)]
    if std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sh"))
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_display::theme::Theme;

    const PLAIN: Theme = Theme::new(false, false, false);

    fn stub_audit(path: &str) -> Outcome {
        Outcome::ok(format!("  ✔ audited (stub) ← {path}"))
    }
    fn stub_wire(client: &str, _dir: &str) -> Outcome {
        Outcome::ok(format!("{client}: wired (stub)"))
    }

    /// The old 7-arg `run` shape, test-side: scripted path with stubs
    /// (the door logic lives at the composition root now).
    fn run(
        dir: &str,
        force: bool,
        _yes: bool,
        recipe: Option<&str>,
        canvas: Option<CanvasTheme>,
        wires: &[&str],
        _theme: Theme,
    ) -> Outcome {
        scripted_run(
            dir,
            force,
            recipe,
            None,
            canvas,
            wires,
            &stub_audit,
            &stub_wire,
        )
    }

    /// The example lane: `--example 01-hello` founds the project around
    /// ONE verbatim lesson — file + generated index + proof + tailored
    /// next; `--recipe` AND `--example` together refuse honestly.
    #[test]
    fn example_lane_founds_around_one_lesson() {
        let tmp = std::env::temp_dir().join(format!("nika-init-example-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let dir = tmp.to_string_lossy().into_owned();

        let out = scripted_run(
            &dir,
            false,
            None,
            Some("01-hello"),
            None,
            &[],
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text.contains("created workflows/01-hello.nika.yaml"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("created workflows/README.md"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("nika run workflows/01-hello.nika.yaml"),
            "tailored next: {}",
            out.text
        );
        let body =
            std::fs::read_to_string(tmp.join("workflows/01-hello.nika.yaml")).expect("written");
        assert_eq!(
            body,
            nika_pack::example("01-hello").expect("embedded"),
            "verbatim"
        );

        let both = scripted_run(
            &dir,
            false,
            Some("agentic"),
            Some("01-hello"),
            None,
            &[],
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(both.code, codes::ENV);
        assert!(both.text.contains("not both"), "{}", both.text);

        let unknown = scripted_run(
            &dir,
            true,
            None,
            Some("nope"),
            None,
            &[],
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(unknown.code, codes::ENV, "{}", unknown.text);
        assert!(unknown.text.contains("nika try"), "{}", unknown.text);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn successful_init_hands_over_to_the_next_command() {
        // The 2026-07-05 beginner walk: init ended SILENTLY (4 files ·
        // no workflow · no next step). An onboarding surface must hand
        // over — the ok-path text carries the golden path.
        let tmp = std::env::temp_dir().join(format!("nika-init-handover-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            None,
            &[],
            PLAIN,
        );
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(out.code, codes::OK);
        assert!(out.text.contains("next ·"), "{}", out.text);
        assert!(out.text.contains("nika try 01-hello"));
        assert!(out.text.contains("nika check"));
    }

    /// `--yes` (and any non-terminal) is the byte-stable script shape —
    /// the report and the classic hand-off, zero prompts.
    #[test]
    fn yes_keeps_the_non_interactive_shape() {
        let tmp = std::env::temp_dir().join(format!("nika-init-yes-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            None,
            &[],
            PLAIN,
        );
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(out.code, codes::OK);
        assert!(out.text.contains("✔ created"), "{}", out.text);
        assert!(
            out.text.contains(NEXT_BLOCK),
            "the classic block survives verbatim: {}",
            out.text
        );
    }

    /// The trace cover lands in the scripted lane (T1): a fresh found
    /// lays `.gitignore` with the `.nika/traces/` entry and the report
    /// says so; a second run is the calm skip row and the bytes are
    /// identical (adds-only — init never rewrites the human's file).
    #[test]
    fn scripted_init_lays_the_traces_cover_adds_only() {
        let tmp = std::env::temp_dir().join(format!("nika-init-gitignore-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            None,
            &[],
            PLAIN,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text.contains("created ") && out.text.contains(".gitignore"),
            "the cover row rides the report: {}",
            out.text
        );
        let body = std::fs::read_to_string(tmp.join(".gitignore")).expect("gitignore written");
        assert!(body.contains(".nika/traces/"), "the cover entry: {body}");

        let again = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            None,
            &[],
            PLAIN,
        );
        assert_eq!(again.code, codes::OK, "{}", again.text);
        assert!(
            again
                .text
                .contains(".gitignore (.nika/traces/ already ignored)"),
            "the second run is a calm skip row: {}",
            again.text
        );
        assert_eq!(
            std::fs::read_to_string(tmp.join(".gitignore")).expect("read"),
            body,
            "adds-only: the second run changed nothing"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn plan_creates_both_when_nothing_exists() {
        let p = plan(".", &|_| false, false);
        assert_eq!(p.len(), 17);
        assert!(p.iter().all(|a| matches!(a, Action::Create { .. })));
        // Schema wiring + agent guide + per-client briefs are the targets.
        let paths: Vec<&str> = p
            .iter()
            .map(|a| match a {
                Action::Create { path, .. } | Action::Skip { path } => path.as_str(),
            })
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("settings.json")));
        assert!(paths.iter().any(|p| p.ends_with("AGENTS.md")));
        assert!(paths.iter().any(|p| p.ends_with("nika.mdc")));
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".agents/skills/nika-authoring/SKILL.md"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with(".github/copilot-instructions.md"))
        );
        assert!(paths.iter().any(|p| p.ends_with("CLAUDE.md")));
    }

    #[test]
    fn plan_skips_an_existing_file_without_force() {
        // AGENTS.md already there · settings.json/rules absent → one Skip, two Create.
        let p = plan(".", &|path| path.ends_with("AGENTS.md"), false);
        assert!(
            p.iter()
                .any(|a| matches!(a, Action::Skip { path } if path.ends_with("AGENTS.md")))
        );
        assert!(
            p.iter().any(
                |a| matches!(a, Action::Create { path, .. } if path.ends_with("settings.json"))
            )
        );
        assert!(
            p.iter()
                .any(|a| matches!(a, Action::Create { path, .. } if path.ends_with("nika.mdc")))
        );
    }

    #[test]
    fn force_overwrites_everything() {
        let p = plan(".", &|_| true, true);
        assert!(p.iter().all(|a| matches!(a, Action::Create { .. })));
    }

    #[test]
    fn join_respects_the_target_dir() {
        let p = plan("/tmp/proj", &|_| false, false);
        assert!(p.iter().any(|a| matches!(a, Action::Create { path, .. }
                if path == "/tmp/proj/.vscode/settings.json")));
        assert!(p.iter().any(|a| matches!(a, Action::Create { path, .. }
                if path == "/tmp/proj/.cursor/rules/nika.mdc")));
    }

    #[test]
    fn render_marks_created_and_skipped() {
        let out = render(&[
            ('✔', "created .vscode/settings.json".to_owned()),
            (
                '·',
                "skipped AGENTS.md (exists · --force to overwrite)".to_owned(),
            ),
        ]);
        assert!(out.contains("✔ created"));
        assert!(out.contains("· skipped"));
    }

    /// The clap vocabulary and the recipe register can never drift —
    /// same names, same order.
    #[test]
    fn recipe_names_mirror_the_register() {
        let register: Vec<&str> = recipes::RECIPES.iter().map(|r| r.name).collect();
        assert_eq!(RECIPE_NAMES.to_vec(), register);
    }

    /// `--recipe agentic --yes` scaffolds the curriculum, audits every
    /// file, and tailors the hand-off to the first workflow.
    #[test]
    fn scripted_recipe_scaffolds_audits_and_hands_over() {
        let tmp = std::env::temp_dir().join(format!("nika-init-recipe-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("agentic"),
            None,
            &[],
            PLAIN,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text
                .contains("✔ created workflows/01-hello-chain.nika.yaml"),
            "{}",
            out.text
        );
        assert!(
            out.text.matches("audited").count() >= 4,
            "all four workflows audited: {}",
            out.text
        );
        assert!(
            out.text
                .contains("nika run workflows/01-hello-chain.nika.yaml --model mock/echo"),
            "the hand-off names the first workflow: {}",
            out.text
        );
        assert!(tmp.join("workflows/04-agent-loop.nika.yaml").exists());
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `--recipe starter -y` used to land byte-identical to `--recipe
    /// minimal`, output included (measured 2026-08-03) — starter's whole
    /// content is the three-question conversation, which a pipe cannot
    /// answer. A flag that silently becomes another flag is the one thing
    /// a scriptable twin must never do, so the run says which half landed.
    ///
    /// The control is the point: minimal must NOT carry the note, or the
    /// assertion above would pass on a message printed unconditionally.
    #[test]
    fn scripted_starter_says_which_half_a_pipe_cannot_deliver() {
        let base = std::env::temp_dir().join(format!("nika-init-starter-{}", std::process::id()));
        let mut said = Vec::new();
        for recipe in ["starter", "minimal"] {
            let tmp = base.join(recipe);
            std::fs::remove_dir_all(&tmp).ok();
            std::fs::create_dir_all(&tmp).expect("mkdir");
            let out = run(
                tmp.to_str().expect("utf8"),
                false,
                true,
                Some(recipe),
                None,
                &[],
                PLAIN,
            );
            assert_eq!(out.code, codes::OK, "{}", out.text);
            said.push(out.text);
        }
        let (starter, minimal) = (&said[0], &said[1]);
        assert!(
            starter.contains("a script cannot answer it") && starter.contains("nika new"),
            "starter names the missing half and the twin: {starter}"
        );
        assert!(
            !minimal.contains("a script cannot answer it"),
            "the note is starter's alone, not printed to everyone: {minimal}"
        );
        assert_ne!(
            starter, minimal,
            "two different --recipe values must not render the same bytes"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// `--theme` stamps the DAG skin into a CREATED settings file — and
    /// the schema wiring survives the stamp.
    #[test]
    fn scripted_theme_stamps_the_settings() {
        let tmp = std::env::temp_dir().join(format!("nika-init-theme-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            Some(CanvasTheme::Nika),
            &[],
            PLAIN,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        let settings = std::fs::read_to_string(tmp.join(".vscode/settings.json")).expect("written");
        let parsed: serde_json::Value = serde_json::from_str(&settings).expect("valid json");
        assert_eq!(
            parsed.get("nika.dag.theme").and_then(|v| v.as_str()),
            Some("nika")
        );
        assert!(parsed.get("yaml.schemas").is_some());
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// An existing settings file is SKIPPED even when `--theme` asks for
    /// a stamp — the human keeps the hand; the skip line says so.
    #[test]
    fn theme_never_clobbers_an_existing_settings_file() {
        let tmp = std::env::temp_dir().join(format!("nika-init-keep-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join(".vscode")).expect("mkdir");
        std::fs::write(tmp.join(".vscode/settings.json"), "{\"mine\": true}\n").expect("seed");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            None,
            Some(CanvasTheme::Phosphor),
            &[],
            PLAIN,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert_eq!(
            std::fs::read_to_string(tmp.join(".vscode/settings.json")).expect("read"),
            "{\"mine\": true}\n",
            "skipped = untouched"
        );
        assert!(out.text.contains("skipped"), "{}", out.text);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The exec-bit stamp survives refactors — Cursor spawns the
    /// seatbelt scripts directly, so a write path that loses the bit
    /// ships dead hooks to every fresh repo (unix).
    #[cfg(unix)]
    #[test]
    fn scaffolded_hook_scripts_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = std::env::temp_dir().join(format!("nika-execbit-{}", std::process::id()));
        let dir = tmp.to_string_lossy().into_owned();
        let rows = apply_briefs(&dir, true, None);
        let mode = std::fs::metadata(tmp.join(".cursor/hooks-nika/guard-run.sh"))
            .expect("guard-run.sh written")
            .permissions()
            .mode();
        let plain = std::fs::metadata(tmp.join(".cursor/hooks.json"))
            .expect("hooks.json written")
            .permissions()
            .mode();
        std::fs::remove_dir_all(&tmp).ok();
        assert!(
            rows.iter()
                .all(|(_, o)| !matches!(o, BriefOutcome::Failed(_))),
            "all briefs land"
        );
        assert_eq!(mode & 0o111, 0o111, "script carries the exec bit");
        assert_eq!(plain & 0o111, 0, "manifest stays a plain file");
    }
}
