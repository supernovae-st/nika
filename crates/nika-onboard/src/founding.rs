// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init [dir]` — found a repo for Nika workflows.
//!
//! Two doors, one law. **Bare on a terminal** the founding wizard runs
//! (`wizard.rs` — recipe · model · canvas · agents · scaffold · proof ·
//! the ready panel). **Any flag, `--yes`, a pipe, or CI** is the
//! scriptable twin: `--recipe` scaffolds a workflow set, `--theme`
//! stamps the VS Code DAG skin, `--wire` connects agent clients — and
//! plain `--yes` emits file receipts and a first-workflow hand-off.
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
use crate::{Audit, Outcome, Wire, briefs, codes, gitignore, project_file};

pub use briefs::agents_md;

/// The `--recipe` vocabulary for clap (`value_parser`) — pinned against
/// the register by test so the two can never drift.
pub const RECIPE_NAMES: [&str; 5] = ["agentic", "starter", "ship", "content", "minimal"];

/// The workflow a plain `nika init` founds around (#1283): the hello
/// lesson `nika try` rehearses — one file that audits clean on this
/// binary and runs offline under `--model mock/echo`. `--recipe minimal`
/// is the explicit no-workflow door.
pub const DEFAULT_EXAMPLE: &str = "01-hello";

/// The `--theme` vocabulary — `nika.dag.theme`'s own enum (the VS Code
/// extension's canvas skin), stamped into `.vscode/settings.json`. The
/// composition root hands the clap word to [`CanvasTheme::parse`]; here
/// it stays plain (no CLI-framework dependency below the root).
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

/// The `--theme` vocabulary for clap (`value_parser`), in [`CanvasTheme`]
/// order — pinned against [`CanvasTheme::parse`] by test.
pub const CANVAS_THEMES: [&str; 4] = ["nika", "editor", "phosphor", "auto"];

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

    /// The wire word back to the skin (`--theme` arrives as a clap string
    /// at the root · the wizard's menu resolves its own way).
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Nika, Self::Editor, Self::Phosphor, Self::Auto]
            .into_iter()
            .find(|c| c.as_str() == word)
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
/// familiar row prefixes; the wizard has
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
/// 10s → scaffold → audit-before-tokens. The shape scripts have seen
/// since #158, now the hand-off of the workflow-less founds only
/// (`--recipe minimal` · a headless `starter`): a plain init founds
/// around [`DEFAULT_EXAMPLE`] and hands over to THAT file (#1283).
pub(crate) const NEXT_BLOCK: &str = "next ·\n  nika try 01-hello   # offline proof · zero keys\n  nika new                                       # your first workflow — guided on a terminal\n  nika new chain my-first.nika       # the same, scriptable\n  nika check my-first.nika                  # audit before a single token";

/// The scriptable path — briefs report with purposes, then each
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
    // The trace cover and the project file ride the same report —
    // adds-only (`gitignore.rs` · `project_file.rs`), so a re-run or the
    // human's own entry is a calm skip row, and a write failure is the
    // same exit-3 class as a brief's.
    let git = gitignore::ensure(dir);
    let project = project_file::ensure(dir, force);
    let failed = rows
        .iter()
        .any(|(_, o)| matches!(o, BriefOutcome::Failed(_)))
        || matches!(git.1, gitignore::Outcome::Failed(_))
        || matches!(project.1, project_file::Outcome::Failed(_));
    // Keep joined paths and receipt prefixes; explain each newly created
    // brief so the transcript helps humans review the generated setup.
    let mut lines: Vec<(char, String)> = rows
        .iter()
        .map(|(path, outcome)| match outcome {
            BriefOutcome::Created => (
                '✔',
                format!("created {path} — {}", briefs::purpose(&rel_to(dir, path))),
            ),
            BriefOutcome::Skipped => (
                '·',
                format!("skipped {path} (exists · --force to overwrite)"),
            ),
            BriefOutcome::Failed(e) => ('✖', format!("{path}: {e}")),
        })
        .collect();
    lines.push(gitignore::report(&git.0, &git.1));
    lines.push(project_file::report(&project.0, &project.1));
    let mut text = render(&lines);
    if failed {
        return Outcome::env(text);
    }

    let (first_workflow, worst, has_drafts) =
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
             The project files were scaffolded; run bare `nika init` on a terminal for the questions, \
             or `nika new \"<your job in plain words>\" <file>.nika` for the twin."
        );
    }

    let wiring = wire_receipts(dir, wires, wire);
    text.push_str(&wiring.text);
    text.push_str("\nteam ·\n  nika.yaml                        # shared cost ceiling + trace retention · commented until your team edits it\n  Git: commit reviewed workflows, goldens, guides, nika.yaml and shared settings.\n  Keep credentials, signing private keys and raw traces local; review artifacts.\n  Read NIKA.md for the file map and first workflow.\n");

    let next = first_workflow.map_or_else(
        || NEXT_BLOCK.to_owned(),
        |first| {
            if has_drafts {
                format!(
                    "next ·\n  $EDITOR {first}                   # fill the remaining `<SLOT: …>` values\n  nika check {first}                    # audit before a single token\n  nika run {first} --model mock/echo   # mocked envelope inference; task model pins, tools and effects remain real\n  nika explain <NIKA-XXXX>              # every finding teaches"
                )
            } else {
                format!(
                "next ·\n  nika check {first}                    # audit before a single token\n  nika run {first} --model mock/echo   # mocked envelope inference; task model pins, tools and effects remain real\n  nika explain <NIKA-XXXX>              # every finding teaches"
                )
            }
        },
    );
    Outcome {
        text: format!("{text}\n{next}"),
        code: worst.max(wiring.code),
    }
}

/// Resolve the ONE founding source into a scaffold set, speak its
/// report, and hand back the first workflow + the worst audit code.
///
/// The example lane (a verbatim lesson) and a recipe (a template set)
/// are two doors to the same ladder — the report and proof below are
/// byte-identical between them. Neither door taken (plain `--yes`)
/// founds around [`DEFAULT_EXAMPLE`] (#1283 · a project with zero
/// workflows taught nothing); `--recipe minimal` is the briefs-only door.
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
) -> Result<(Option<String>, u8, bool), Outcome> {
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
        (None, None) => recipes::scaffold_example(dir, DEFAULT_EXAMPLE, force),
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
) -> Result<(Option<String>, u8, bool), Outcome> {
    let mut created: Vec<String> = Vec::new();
    for (path, status) in scaffolded {
        let rel = rel_to(dir, path);
        match status {
            ScaffoldStatus::Created => {
                let _ = writeln!(text, "✔ created {rel} — {}", briefs::purpose(&rel));
                // The proof ladder audits WORKFLOWS — the generated
                // index rides the report but never the check.
                if nika_source::is_canonical_program_path(path) {
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
    let mut has_drafts = false;
    for receipt in proof_receipts(dir, &created, audit) {
        worst = worst.max(receipt.code);
        has_drafts |= receipt.draft;
        let _ = writeln!(text, "{}", receipt.line);
    }
    Ok((first, worst, has_drafts))
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
/// settings file and producer version in a CREATED session hook. Skipped
/// files retain their content and provenance.
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
                let stamped;
                let body = if path.ends_with("hooks-nika/session-context.sh") {
                    stamped = body.replacen(
                        "scaffold_version=\"\"",
                        concat!("scaffold_version=\"", env!("CARGO_PKG_VERSION"), "\""),
                        1,
                    );
                    stamped.as_str()
                } else {
                    body
                };
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
pub(crate) struct ProofReceipt {
    pub(crate) line: String,
    pub(crate) code: u8,
    pub(crate) draft: bool,
}

pub(crate) fn proof_receipts(
    dir: &str,
    created: &[impl AsRef<str>],
    audit: &Audit<'_>,
) -> Vec<ProofReceipt> {
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
                    .find(|l| l.contains("audited") || l.contains("not a workflow yet"))
                    .map_or_else(|| "audited clean".to_owned(), |l| l.trim().to_owned());
                ProofReceipt {
                    draft: tail.contains("not a workflow yet"),
                    line: format!("  {tail} ← {rel}"),
                    code: codes::OK,
                }
            } else {
                ProofReceipt {
                    line: format!("{}\n✖ {rel} — findings above", audit.text.trim_end()),
                    code: audit.code,
                    draft: false,
                }
            }
        })
        .collect()
}

/// Connect the picked agent clients through the REAL `wire` verb —
/// each client's own receipt, indented under one header.
pub(crate) fn wire_receipts(dir: &str, wires: &[&str], wire: &Wire<'_>) -> Outcome {
    if wires.is_empty() {
        return Outcome {
            text: String::new(),
            code: codes::OK,
        };
    }
    let mut text = "wiring ·\n".to_owned();
    let mut code = codes::OK;
    for client in wires {
        let out = wire(client, dir);
        code = code.max(out.code);
        for l in out.text.lines() {
            let _ = writeln!(text, "  {l}");
        }
    }
    Outcome { text, code }
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

    /// The clap vocabulary and the enum cannot drift: every word parses
    /// to the skin that speaks it, and nothing else parses.
    #[test]
    fn canvas_theme_words_round_trip_through_the_register() {
        for word in CANVAS_THEMES {
            assert_eq!(
                CanvasTheme::parse(word).map(CanvasTheme::as_str),
                Some(word)
            );
        }
        assert_eq!(CanvasTheme::parse("neon"), None);
    }

    #[test]
    fn scripted_init_preserves_a_wire_refusal() {
        let dir =
            std::env::temp_dir().join(format!("nika-init-wire-refusal-{}", std::process::id()));
        let out = scripted_run(
            dir.to_str().expect("path"),
            false,
            None,
            None,
            None,
            &["all"],
            &stub_audit,
            &|_, _| Outcome::env("wiring requires a decision".to_owned()),
        );
        assert_eq!(out.code, codes::ENV, "{}", out.text);
        assert!(out.text.contains("wiring requires a decision"));
        assert!(!out.text.contains("wired ·"));
        assert!(
            dir.join("AGENTS.md").exists(),
            "completed scaffold is retained"
        );
        std::fs::remove_dir_all(dir).expect("remove owned fixture");
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
            out.text.contains("created workflows/01-hello.nika"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("created workflows/README.md"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("nika run workflows/01-hello.nika"),
            "tailored next: {}",
            out.text
        );
        let body = std::fs::read_to_string(tmp.join("workflows/01-hello.nika")).expect("written");
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
    fn effectful_scaffolds_teach_check_before_run_without_an_offline_promise() {
        for draft in [false, true] {
            let tmp = std::env::temp_dir().join(format!(
                "nika-init-effectful-{}-{draft}",
                std::process::id()
            ));
            let audit = |_: &str| {
                Outcome::ok(
                    if draft {
                        "not a workflow yet"
                    } else {
                        "audited clean"
                    }
                    .to_owned(),
                )
            };
            let out = scripted_run(
                tmp.to_str().expect("path"),
                false,
                None,
                Some("03-exec-pipeline"),
                None,
                &[],
                &audit,
                &stub_wire,
            );
            let source = std::fs::read_to_string(tmp.join("workflows/03-exec-pipeline.nika"))
                .expect("effectful example");
            assert!(
                source.contains("exec:"),
                "the fixture has real subprocess effects"
            );
            let next = out.text.split("next ·").last().expect("handover");
            let check = next.find("nika check ").expect("inspection command");
            let run = next.find("nika run ").expect("execution command");
            assert!(check < run, "inspection precedes real execution: {next}");
            assert!(
                !next.contains("offline proof") && !next.contains("zero keys"),
                "mock does not remove effects or task model pins: {next}"
            );
            assert!(
                next.contains("effects remain real"),
                "handover names the execution boundary: {next}"
            );
            std::fs::remove_dir_all(tmp).expect("remove temp project");
        }
    }

    #[test]
    fn successful_init_hands_over_to_the_next_command() {
        // The 2026-07-05 beginner walk: init ended SILENTLY (4 files ·
        // no workflow · no next step). An onboarding surface must hand
        // over — the ok-path text hands over to the founded workflow.
        let tmp = std::env::temp_dir().join(format!("nika-init-handover-{}", std::process::id()));
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
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(out.code, codes::OK);
        assert!(out.text.contains("next ·"), "{}", out.text);
        assert!(out.text.contains("nika check workflows/01-hello.nika"));
        assert!(
            out.text
                .contains("nika run workflows/01-hello.nika --model mock/echo")
        );
    }

    /// #1283 · a plain scripted init founds a COMPLETE project: the
    /// project file (adds-only · the skip row on a re-run) and the hello
    /// lesson as the first workflow, every created row saying why it
    /// exists; `--recipe minimal` stays the briefs-only door with the
    /// classic hand-off.
    #[test]
    fn plain_init_lays_the_project_file_and_the_hello_lesson() {
        let tmp = std::env::temp_dir().join(format!("nika-init-default-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let d = tmp.to_str().expect("utf8");
        let out = run(d, false, true, None, None, &[], PLAIN);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text
                .lines()
                .any(|l| l.starts_with("✔ created ") && l.contains("nika.yaml — team defaults")),
            "the project file row says why: {}",
            out.text
        );
        assert!(
            out.text
                .contains("✔ created workflows/01-hello.nika — a workflow"),
            "the workflow row says why: {}",
            out.text
        );
        assert!(tmp.join("nika.yaml").exists() && tmp.join("workflows/01-hello.nika").exists());
        assert!(
            !out.text.contains(NEXT_BLOCK) && !out.text.contains("--project-file"),
            "the hand-off is the founded file, the team block names the laid file: {}",
            out.text
        );
        let again = run(d, false, true, None, None, &[], PLAIN);
        assert!(
            again
                .text
                .lines()
                .any(|l| l.starts_with("· skipped ") && l.contains("nika.yaml (exists")),
            "adds-only on a re-run: {}",
            again.text
        );
        let minimal = run(d, false, true, Some("minimal"), None, &[], PLAIN);
        assert!(minimal.text.contains(NEXT_BLOCK), "{}", minimal.text);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `--yes` (and any non-terminal) keeps the scriptable file report —
    /// the report and the classic hand-off, zero prompts.
    #[test]
    fn yes_keeps_the_non_interactive_shape() {
        let tmp = std::env::temp_dir().join(format!("nika-init-yes-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("minimal"),
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
    fn fresh_project_routes_to_the_shipped_authoring_resources() {
        let tmp = std::env::temp_dir().join(format!("nika-authoring-route-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("temp project");
        let rows = apply_briefs(tmp.to_str().expect("path"), false, None);
        assert!(
            rows.iter()
                .all(|(_, outcome)| matches!(outcome, BriefOutcome::Created))
        );
        let entry = std::fs::read_to_string(tmp.join("AGENTS.md")).expect("entry");
        let skill_path = ".agents/skills/nika-authoring/SKILL.md";
        assert!(
            entry.contains(&format!("]({skill_path})")),
            "the loaded entry must route to the local skill"
        );
        let skill = std::fs::read_to_string(tmp.join(skill_path)).expect("local skill");
        let mut references = 0;
        for part in skill.split("](references/").skip(1) {
            let relative = part.split(')').next().expect("reference link");
            let path = tmp
                .join(".agents/skills/nika-authoring/references")
                .join(relative);
            assert!(
                std::fs::read_to_string(path).is_ok(),
                "reference {relative} is readable"
            );
            references += 1;
        }
        assert_eq!(
            references, 6,
            "all conditional guides reachable from the entry"
        );
        std::fs::remove_dir_all(tmp).expect("remove temp project");
    }

    #[test]
    fn project_hook_carries_its_generator_version_and_reruns_preserve_it() {
        let tmp = std::env::temp_dir().join(format!("nika-hook-provenance-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("temp project");
        apply_briefs(tmp.to_str().expect("path"), false, None);
        let hook = tmp.join(".cursor/hooks-nika/session-context.sh");
        let body = std::fs::read_to_string(&hook).expect("project hook");
        assert!(
            body.contains(concat!(
                "scaffold_version=\"",
                env!("CARGO_PKG_VERSION"),
                "\""
            )),
            "the relocated hook must retain its producer version"
        );
        std::fs::write(&hook, "# user-owned hook\n").expect("custom hook");
        apply_briefs(tmp.to_str().expect("path"), false, None);
        assert_eq!(
            std::fs::read_to_string(&hook).expect("preserved hook"),
            "# user-owned hook\n"
        );
        std::fs::remove_dir_all(tmp).expect("remove temp project");
    }

    #[test]
    fn plan_creates_both_when_nothing_exists() {
        let p = plan(".", &|_| false, false);
        assert_eq!(p.len(), 28);
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
            out.text.contains("✔ created workflows/01-hello-chain.nika"),
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
                .contains("nika run workflows/01-hello-chain.nika --model mock/echo"),
            "the hand-off names the first workflow: {}",
            out.text
        );
        assert!(tmp.join("workflows/04-agent-loop.nika").exists());
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
