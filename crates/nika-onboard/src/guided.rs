// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika new --from <template|intent> <dest>` — instantiate one of the
//! embedded skeletons (spec §2). Refuses to overwrite (the human keeps
//! the hand); `--force` is the explicit override. The written file is
//! the template VERBATIM — slots stay visible so the author fills them
//! deliberately.
//!
//! `--from` resolves in two rungs:
//! 1. **exact template name** — instant, unchanged;
//! 2. **intent routing** — anything else is BM25-ranked (the admitted
//!    `nika-bm25` crate · Robertson-Zaragoza Okapi) against the
//!    template names + bodies, with a small everyday-word → Nika-vocab
//!    alias bridge on the QUERY side (« scrape » → fetch · « summarize »
//!    → infer · « parallel » → fan-out). The top match instantiates and
//!    the routing is SAID (`routed intent → template …`); a zero-score
//!    query gets the honest unknown-template error. Deterministic,
//!    zero-LLM — the floor of « the binary generates the best workflow
//!    for the intent » (editor LLM loops build ON this rung).

use std::fmt::Write as _;
use std::io::{BufRead, IsTerminal};
use std::path::Path;

use nika_bm25::{BmIndex, BmParams};

use nika_display::theme::{Role, Theme};

use crate::{Audit, Outcome, codes};

/// Emit a value as a SAFE YAML scalar. A plain token (`provider/model`,
/// a one-word description) stays bare; anything with a YAML-significant
/// char — a space, a `:`, a `\` (a Windows path · a regex), a quote —
/// is SINGLE-quoted with `''` escaping. Single-quoted YAML takes every
/// other byte literally (no backslash-escape maze), so a user's intent
/// prose or an exotic model string can never turn the stamped file
/// invalid (the rust-pro review's HIGH: user strings reached YAML
/// unescaped → a fresh scaffold failed its OWN check under a green ✔).
fn yaml_scalar(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-');
    if !value.is_empty() && value.chars().all(plain) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

/// POSIX shell-quote a path for a COPY-PASTEABLE command suggestion — a
/// workflow named « My Cool Flow » becomes `My Cool Flow.nika.yaml`, and
/// the wizard's `nika run <dest>` hint would parse as four arguments and
/// fail the moment the user pastes it. Only quote when a shell-special
/// char is present (the common kebab-case path stays bare). Single
/// quotes with the `'\''` escape — the one form that is total.
fn shell_quote(path: &str) -> std::borrow::Cow<'_, str> {
    let safe =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '@' | '+');
    if !path.is_empty() && path.chars().all(safe) {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(format!("'{}'", path.replace('\'', "'\\''")))
    }
}

/// `nika new` — resolve the missing `--from` per clig.dev: a terminal
/// gets the guided flow (prompt for the missing argument) · a pipe/CI
/// fails fast naming the flag (never REQUIRE interactivity).
#[must_use]
pub fn dispatch(
    from: Option<&str>,
    dest: Option<&str>,
    force: bool,
    theme: Theme,
    audit: &Audit<'_>,
) -> Outcome {
    match from {
        Some(f) => run(f, dest, force),
        None if interactive() => {
            let stdin = std::io::stdin();
            wizard_io(
                ".",
                dest,
                force,
                theme,
                &mut stdin.lock(),
                &mut std::io::stdout(),
                audit,
            )
        }
        None => Outcome {
            text: format!(
                "nothing to scaffold — pass --from <template|intent> (`nika new --from '?'` lists the set)\nembedded set: {}",
                nika_pack::template_names().join(" · ")
            ),
            code: codes::FILE,
        },
    }
}

/// Both ends of the conversation are a terminal — the only state in
/// which any nika surface may prompt (clig.dev interactivity rule).
fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The `nika new` verb.
#[must_use]
pub fn run(template: &str, dest: Option<&str>, force: bool) -> Outcome {
    // `?` is the documented discovery query — a question answered is a
    // SUCCESS, not a finding (it used to reuse the unknown-template error
    // and exit 2, which read as failure to a human following the help).
    if template == "?" {
        return discovery();
    }
    // ONE resolution ladder for ONE intention («a file of mine») with two
    // sources: template skeletons (SLOTs to fill) and complete examples
    // (lessons, verbatim). Exact names first — templates, then examples
    // (slug or filename · `showcase/` browsable) — then plain-words
    // intent. `nika examples copy` stays the showroom-side handle of the
    // same gesture.
    if let Some(body) = nika_pack::example(template) {
        return write_example(template, body, dest, force);
    }
    let (name, body, routed) = match nika_pack::template(template) {
        Some(body) => (template.to_owned(), body, false),
        None => match route_intent(template) {
            Some(name) => {
                // route_intent only returns names from template_names() —
                // the lookup is total; an empty body would be a pack bug
                // surfaced as the honest unknown error, never a panic.
                let Some(body) = nika_pack::template(&name) else {
                    return unknown(template);
                };
                (name, body, true)
            }
            None => return unknown(template),
        },
    };
    // A known template needs a destination to instantiate into. The
    // `--from '?'` (or any unknown) discovery query already returned above
    // WITHOUT touching `dest` — that is the editor-integration wire
    // contract (`unknown()`), so listing the set must not require a dummy
    // path (the help promises `nika new --from '?'` works bare).
    let Some(dest) = dest else {
        return Outcome {
            text: format!(
                "template `{name}` resolved — pass a destination: nika new --from {template} <dest>.nika.yaml"
            ),
            code: codes::ENV,
        };
    };
    if Path::new(dest).exists() && !force {
        return Outcome {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: codes::ENV,
        };
    }
    if let Err(e) = std::fs::write(dest, body) {
        return Outcome {
            text: format!("cannot write {dest}: {e}"),
            code: codes::ENV,
        };
    }
    let routing = if routed { "routed intent → " } else { "" };
    Outcome {
        text: format!(
            "{dest} ← {routing}template `{name}` · fill the `# SLOT:` lines then `nika check {q}`",
            q = shell_quote(dest)
        ),
        code: codes::OK,
    }
}

/// An EXAMPLE source lands verbatim (a lesson, complete — no SLOTs);
/// the receipt says the check-then-run road. The default destination is
/// the slug's basename (`showcase/t1-price-watch` → `t1-price-watch
/// .nika.yaml` — the tiering belongs to the pack, your workspace is
/// flat).
fn write_example(slug: &str, body: &str, dest: Option<&str>, force: bool) -> Outcome {
    let clean = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    let base = clean.rsplit('/').next().unwrap_or(clean);
    let fallback = format!("{base}.nika.yaml");
    let dest = dest.unwrap_or(&fallback);
    if Path::new(dest).exists() && !force {
        return Outcome {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: codes::ENV,
        };
    }
    if let Err(e) = std::fs::write(dest, body) {
        return Outcome {
            text: format!("cannot write {dest}: {e}"),
            code: codes::ENV,
        };
    }
    Outcome {
        text: format!(
            "{dest} ← example `{clean}` · yours now — `nika check {q}` then `nika run {q}`",
            q = shell_quote(dest)
        ),
        code: codes::OK,
    }
}

/// The unknown-template finding. The `embedded set:` line is a WIRE
/// CONTRACT — editor integrations probe `nika new --from '?'` and parse
/// the set from exactly this shape; never reword it.
fn unknown(template: &str) -> Outcome {
    Outcome {
        text: format!(
            "no template or intent matches `{template}` — embedded set: {}\n  hint: name one, describe the job with its verbs (fetch · summarize · parallel · approve…), or pass an example slug from `nika examples list`",
            nika_pack::template_names().join(" · ")
        ),
        code: codes::FILE,
    }
}

/// First-class `--from '?'` — the listing a human reads (name + tagline
/// derived from each template's own `# TEMPLATE` header · no second
/// source) with the `embedded set:` WIRE-CONTRACT line kept verbatim for
/// the editor probes. Exit 0: a discovery query answered is a success.
fn discovery() -> Outcome {
    let names = nika_pack::template_names();
    let mut text = String::from("the embedded template skeletons ·\n");
    for name in &names {
        let tag = nika_pack::template(name).map_or_else(String::new, |b| tagline(name, b));
        let _ = writeln!(text, "  {name:<18} {tag}");
    }
    let _ = write!(text, "\nembedded set: {}", names.join(" · "));
    text.push_str(
        "\n\nexamples work here too (complete lessons · verbatim) ·\n  nika new --from 01-hello my-hello.nika.yaml    # any slug from `nika examples list`",
    );
    text.push_str(
        "\n\ntry ·\n  nika new --from chain my-first.nika.yaml\n  nika new --from \"describe the job in plain words\" my.nika.yaml   # routes to the closest skeleton\n  nika new                                                          # guided (terminal only)",
    );
    Outcome {
        text,
        code: codes::OK,
    }
}

/// The one-line tagline out of a template's `# TEMPLATE · <name> · …`
/// header. Empty when a body carries no header — the listing degrades
/// gracefully instead of inventing prose. A header that wraps to the
/// next comment line gets an honest `…` instead of ending mid-thought.
pub(crate) fn tagline(name: &str, body: &str) -> String {
    body.lines()
        .find_map(|l| l.strip_prefix("# TEMPLATE"))
        .map_or_else(String::new, |rest| {
            let rest = rest.trim_start_matches([' ', '·']);
            let rest = rest.strip_prefix(name).unwrap_or(rest);
            let rest = rest.trim_start_matches([' ', '·', ':']).trim_end();
            let clean = rest.trim_end_matches([',', ' ']);
            if rest.ends_with('.') {
                clean.to_owned()
            } else {
                format!("{clean}…")
            }
        })
}

// ─── The guided flow (the wizard) ────────────────────────────────────
//
// Three questions, every one Enter-skippable, reachable from two doors:
// `nika new` bare on a terminal (missing argument → prompt · clig.dev)
// and `nika init`'s hand-off (gh-repo-create shape: bare on a TTY is
// guided · flags and pipes keep the exact old behavior). The answers
// land in the SAME file the flag form writes — plus the three slots the
// wizard can stamp honestly (id · description · model); the remaining
// `# SLOT:` lines stay visible so the author fills them deliberately.

/// The wizard's harvest — decoupled from the terminal (answers arrive
/// through any `BufRead` · tests inject a cursor).
struct Wizard {
    template: String,
    routed: bool,
    dest: String,
    /// `None` = the skeleton carries no top-level `model:` (its models
    /// are per-task) — the wizard neither asked nor stamps one.
    model: Option<String>,
    intent: String,
}

/// Whether a template takes the wizard's model answer — a top-level
/// `model:` line at column 0 (the stamp's own anchor). Skeletons whose
/// models live per-task get NO model question: asking and then not
/// stamping would promise what the file doesn't carry.
pub(crate) fn template_takes_model(body: &str) -> bool {
    body.lines().any(|l| l.starts_with("model: "))
}

/// The collision-free default destination — `my-first`, then `my-second`,
/// `my-third`, then numbered. A re-run of the wizard must never dead-end
/// on « exists — pass --force » AFTER the human answered every question.
fn wizard_default_dest(base: &str) -> String {
    let candidates = ["my-first", "my-second", "my-third"];
    let free = |stem: &str| {
        let name = format!("{stem}.nika.yaml");
        !Path::new(base).join(&name).exists()
    };
    for stem in candidates {
        if free(stem) {
            return format!("{stem}.nika.yaml");
        }
    }
    let mut n = 4;
    loop {
        let stem = format!("my-{n}");
        if free(&stem) {
            return format!("{stem}.nika.yaml");
        }
        n += 1;
    }
}

/// One prompt · one line back. `None` = EOF (the human left — cancel,
/// never loop). The `>` is the single accent — the conversation's
/// running line, same semantic slot as the run render's active task.
pub(crate) fn ask(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
    prompt: &str,
) -> std::io::Result<Option<String>> {
    write!(out, "{prompt}\n{} ", theme.paint(Role::Accent, ">"))?;
    out.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_owned()))
}

/// Intent → template: exact name first, BM25 routing second, the
/// `chain` default third (empty answer = the golden path).
fn resolve_template(intent: &str) -> (String, bool) {
    if intent.is_empty() {
        return ("chain".to_owned(), false);
    }
    if nika_pack::template(intent).is_some() {
        return (intent.to_owned(), false);
    }
    match route_intent(intent) {
        Some(name) => (name, true),
        None => ("chain".to_owned(), false),
    }
}

/// The provider menu, DERIVED from the embedded catalog (no hardcoded
/// model names to drift) in the doctrine presentation order · local
/// first · offline mock · EU open-weight · then the US clouds.
pub(crate) fn model_menu() -> Vec<(String, &'static str)> {
    let export = nika_catalog::export::catalog_export();
    [
        ("ollama", "local · sovereign · zero key"),
        ("mock", "offline preview · zero key · always works"),
        ("mistral", "EU · open-weight"),
        ("anthropic", ""),
        ("openai", ""),
    ]
    .iter()
    .filter_map(|(id, note)| {
        export.providers.iter().find(|p| p.id == *id).map(|p| {
            // `mock/echo` is THE teaching example on every other surface
            // (the help footer · the init hand-off · AGENTS.md · docs) —
            // the menu must not introduce a second mock spelling.
            let model = if p.id == "mock" {
                "echo"
            } else {
                p.default_model
            };
            (format!("{}/{model}", p.id), *note)
        })
    })
    .collect()
}

/// A menu number, a full `provider/model`, or Enter → the offline mock
/// (the one answer that succeeds with zero keys and zero network — the
/// first run must not be able to fail).
pub(crate) fn resolve_model(pick: &str, menu: &[(String, &'static str)]) -> String {
    let fallback = || {
        menu.get(1)
            .map_or_else(|| "mock/echo".to_owned(), |(m, _)| m.clone())
    };
    if pick.is_empty() {
        return fallback();
    }
    if let Ok(n) = pick.parse::<usize>()
        && let Some((m, _)) = n.checked_sub(1).and_then(|i| menu.get(i))
    {
        return m.clone();
    }
    if pick.contains('/') {
        return pick.to_owned();
    }
    fallback()
}

/// A kebab workflow id out of the destination file name.
pub(crate) fn workflow_id(dest: &str) -> String {
    let base = Path::new(dest)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = base.strip_suffix(".nika.yaml").unwrap_or(&base);
    let id: String = stem
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id.trim_matches('-').to_owned();
    if id.is_empty() {
        "my-first".to_owned()
    } else {
        id
    }
}

/// Stamp the answers the wizard KNOWS into the template — id ·
/// description · model (the last only when the wizard asked, i.e. the
/// skeleton carries a top-level `model:` at column 0 — the stamp's
/// anchor). Never stamp what wasn't answered.
pub(crate) fn stamp(body: &str, id: &str, description: &str, model: Option<&str>) -> String {
    let mut out: String = body
        .lines()
        .map(|line| {
            if line.starts_with("workflow: ") {
                format!("workflow: {id}")
            } else if line.starts_with("description: ") && !description.is_empty() {
                format!("description: {}", yaml_scalar(description))
            } else if let (true, Some(model)) = (line.starts_with("model: "), model) {
                format!("model: {}", yaml_scalar(model))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// Run the (at most) three questions over injected io. `Ok(None)` =
/// cancelled. Styling stays inside the semantic seam (theme.rs): the
/// brand mark on the header · dim for defaults/metadata · the accent on
/// resolutions. Every register without colour keeps byte-identical text
/// (paint is a no-op), so the conversation reads the same in a
/// transcript. The model question only fires for skeletons that carry a
/// top-level `model:` — the others say so instead of asking.
/// The model beat — menu (catalog-derived · local first) then one pick.
/// `Ok(None)` = EOF (cancelled), consistent with `ask`.
pub(crate) fn ask_model(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<String>> {
    let menu = model_menu();
    writeln!(
        out,
        "\nmodel {}",
        theme.paint(
            Role::Dim,
            "— the same file runs on any provider (`nika catalog` names them all)"
        )
    )?;
    for (i, (m, note)) in menu.iter().enumerate() {
        writeln!(
            out,
            "  {}  {m:<30} {}",
            theme.paint(Role::Strong, &(i + 1).to_string()),
            theme.paint(Role::Dim, note),
        )?;
    }
    let Some(pick) = ask(
        input,
        out,
        theme,
        &format!(
            "a number, or any provider/model {}",
            theme.paint(Role::Dim, "[2]")
        ),
    )?
    else {
        return Ok(None);
    };
    let model = resolve_model(&pick, &menu);
    writeln!(out, "  → model `{}`", theme.paint(Role::Accent, &model))?;
    Ok(Some(model))
}

fn read_wizard(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    base: &str,
    dest_hint: Option<&str>,
    theme: Theme,
) -> std::io::Result<Option<Wizard>> {
    writeln!(
        out,
        "{} nika · {} {}",
        theme.logo(),
        theme.paint(Role::Strong, "your first workflow"),
        theme.paint(Role::Dim, "— Enter accepts a default · Ctrl-C exits"),
    )?;
    let Some(intent) = ask(
        input,
        out,
        theme,
        &format!(
            "\nwhat should it do? {}",
            theme.paint(
                Role::Dim,
                "— a plain sentence routes to the closest skeleton [chain]"
            )
        ),
    )?
    else {
        return Ok(None);
    };
    let (template, routed) = resolve_template(&intent);
    let tag = nika_pack::template(&template).map_or_else(String::new, |b| tagline(&template, b));
    writeln!(
        out,
        "  → template `{}` {}",
        theme.paint(Role::Accent, &template),
        theme.paint(Role::Dim, &format!("— {tag}")),
    )?;

    let fallback = wizard_default_dest(base);
    let default_dest = dest_hint.unwrap_or(&fallback);
    let Some(mut dest) = ask(
        input,
        out,
        theme,
        &format!(
            "\nfile {}",
            theme.paint(Role::Dim, &format!("[{default_dest}]"))
        ),
    )?
    else {
        return Ok(None);
    };
    if dest.is_empty() {
        default_dest.clone_into(&mut dest);
    }
    if !dest.ends_with(".nika.yaml") {
        dest.push_str(".nika.yaml");
    }

    let model = if nika_pack::template(&template).is_some_and(template_takes_model) {
        match ask_model(input, out, theme)? {
            Some(model) => Some(model),
            None => return Ok(None),
        }
    } else {
        // Asking would promise a stamp the file doesn't carry — say
        // where the models actually live instead.
        writeln!(
            out,
            "  {}",
            theme.paint(
                Role::Dim,
                "models are per-task in this skeleton — set them at the `model:` slots in the file"
            )
        )?;
        None
    };
    Ok(Some(Wizard {
        template,
        routed,
        dest,
        model,
        intent,
    }))
}

/// The wizard over real io: converse, then materialize the file.
pub fn wizard_io(
    base: &str,
    dest_hint: Option<&str>,
    force: bool,
    theme: Theme,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    audit: &Audit<'_>,
) -> Outcome {
    match read_wizard(input, out, base, dest_hint, theme) {
        Err(e) => Outcome {
            text: format!("wizard i/o failed: {e}"),
            code: codes::ENV,
        },
        Ok(None) => Outcome {
            text: "cancelled — nothing written".to_owned(),
            code: codes::ENV,
        },
        Ok(Some(w)) => materialize(base, &w, force, theme, audit),
    }
}

/// Write the stamped template, then RUN the audit and show the ladder —
/// the wizard hands over a CHECKED workflow, not a suggestion to check
/// (audit-before-run is the differentiator; the first minute must show
/// it, not name it). Also teach the scriptable form of what the wizard
/// just did — reproducibility is part of the contract.
fn materialize(base: &str, w: &Wizard, force: bool, theme: Theme, audit: &Audit<'_>) -> Outcome {
    let dest = if Path::new(&w.dest).is_absolute() || base == "." {
        w.dest.clone()
    } else {
        Path::new(base).join(&w.dest).to_string_lossy().into_owned()
    };
    let Some(body) = nika_pack::template(&w.template) else {
        return unknown(&w.template);
    };
    if Path::new(&dest).exists() && !force {
        return Outcome {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: codes::ENV,
        };
    }
    let id = workflow_id(&dest);
    let description = w.intent.as_str();
    let stamped = stamp(body, &id, description, w.model.as_deref());
    if let Err(e) = std::fs::write(&dest, &stamped) {
        return Outcome {
            text: format!("cannot write {dest}: {e}"),
            code: codes::ENV,
        };
    }
    let routing = if w.routed { "routed intent → " } else { "" };
    // The summary claims exactly what the file carries — a stamped model
    // when the wizard asked, the per-task truth when it didn't.
    let model_said = w
        .model
        .as_ref()
        .map_or_else(|| "models per-task".to_owned(), |m| format!("model `{m}`"));
    let wrote = format!(
        "{} {dest} ← {routing}template `{tpl}` · stamped workflow `{id}` · {model_said}",
        theme.paint(Role::Good, "✔"),
        tpl = w.template,
    );
    // The audit runs NOW — the ladder on screen inside the first minute
    // is the product's argument (a red ladder would honestly propagate,
    // but a fresh scaffold checks clean by the templates' own-corpus law).
    let audit = audit(&dest);
    let q = shell_quote(&dest);
    let next = format!(
        "next ·\n  $EDITOR {q}                   # fill the remaining `# SLOT:` lines\n  nika run {q}                  # execute · live render (mock is offline · $0.00)\n\n{}",
        theme.paint(
            Role::Dim,
            &format!("scriptable form · nika new --from {} {q}", w.template)
        ),
    );
    Outcome {
        text: format!("{wrote}\n\n{}\n\n{next}", audit.text.trim_end()),
        code: audit.code,
    }
}

/// Everyday intent words → the Nika vocabulary the template bodies
/// actually carry. Query-side only (documents are never expanded) — the
/// evidenced cheap recall upgrade at tiny corpus scale, in place of
/// embeddings (BM25 stays the ranker).
const ALIASES: &[(&str, &[&str])] = &[
    ("scrape", &["fetch", "read"]),
    ("crawl", &["fetch"]),
    ("download", &["fetch"]),
    ("http", &["fetch"]),
    ("url", &["fetch"]),
    ("website", &["fetch"]),
    ("page", &["fetch"]),
    ("api", &["fetch", "invoke"]),
    ("llm", &["infer"]),
    ("ai", &["infer"]),
    ("model", &["infer"]),
    ("prompt", &["infer"]),
    ("summarize", &["infer", "think"]),
    ("classify", &["infer"]),
    ("generate", &["infer"]),
    ("save", &["write", "persist", "state"]),
    ("shell", &["exec"]),
    ("command", &["exec"]),
    ("script", &["exec"]),
    ("build", &["exec"]),
    ("test", &["exec", "verify"]),
    ("deploy", &["ship", "act", "exec"]),
    ("release", &["ship", "act"]),
    ("parallel", &["for_each", "fan", "merge"]),
    ("concurrent", &["for_each", "fan"]),
    ("batch", &["for_each", "items"]),
    ("each", &["for_each", "items"]),
    ("every", &["for_each", "items"]),
    ("loop", &["agent", "for_each"]),
    ("iterate", &["agent", "for_each"]),
    ("agentic", &["agent"]),
    ("autonomous", &["agent", "budgeted"]),
    ("review", &["gate", "verify"]),
    ("approve", &["gate", "human"]),
    ("approval", &["gate", "human"]),
    ("confirm", &["gate", "human"]),
    ("pipeline", &["chain", "gather", "think"]),
    ("sequence", &["chain"]),
    ("transform", &["jq", "process"]),
    ("json", &["jq"]),
    ("state", &["state", "diff", "delta"]),
    ("incremental", &["state", "diff", "delta"]),
];

/// Function words + Nika envelope keywords that carry zero routing signal —
/// stripped from the query so an all-boilerplate `--from` (`the` · `workflow`
/// · `template`) lists the set instead of spuriously routing (every template
/// shares `workflow:`/`tasks:`/… so those terms separate nothing).
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "the", "to", "of", "in", "on", "for", "with", "that", "this", "then", "than",
    "into", "from", "by", "as", "at", "is", "are", "be", "it", "its", "or", "i", "me", "my", "we",
    "you", "no", "such", "nika", "workflow", "model", "vars", "tasks", "id", "template", "slot",
    "kebab", "case",
];

/// BM25-route a free-form intent to the best embedded template.
/// `None` when no template shares a single term with the (alias-
/// expanded) query — routing on zero evidence would be a guess.
fn route_intent(intent: &str) -> Option<String> {
    let names = nika_pack::template_names();
    let mut index = BmIndex::new(BmParams::default());
    for (i, name) in names.iter().enumerate() {
        let body = nika_pack::template(name).unwrap_or_default();
        // Index the template's MEANINGFUL vocabulary — verbs · tools ·
        // structure — but STRIP `#` comments. The `# SLOT: kebab-case
        // workflow id` scaffolding prose otherwise pollutes the index, so
        // boilerplate/stopword queries ("slot" · "kebab" · "fill" · "the")
        // spuriously route instead of listing the set. Real intent routes
        // on the YAML verbs/tools + the ALIASES, not the comment prose.
        let meaningful: String = body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .map(|l| l.split_once(" #").map_or(l, |(before, _)| before))
            .collect::<Vec<_>>()
            .join("\n");
        index.add_document(u32::try_from(i).ok()?, &format!("{name}\n{meaningful}"));
    }
    index.finalize();

    // Keep only signal-bearing tokens (drop stopwords + Nika boilerplate);
    // an all-boilerplate query routes NOWHERE → the honest unknown · list.
    let tokens: Vec<String> = intent
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|t| !t.is_empty() && !STOPWORDS.contains(&t.as_str()))
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let mut query = tokens.join(" ");
    for token in &tokens {
        for (word, expansions) in ALIASES {
            if token == *word {
                for e in *expansions {
                    query.push(' ');
                    query.push_str(e);
                }
            }
        }
    }

    let ranked = index.top_k(&query, 1);
    let (doc, score) = ranked.first()?;
    if *score <= 0.0 {
        return None;
    }
    names.get(*doc as usize).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// The test audit stub — the ladder's SHAPE without the ladder
    /// (integration against the real check lives at the composition
    /// root and in nika-onboard's own dev-dep ratchets).
    fn stub_audit(path: &str) -> Outcome {
        Outcome::ok(format!("✔ audited (stub) ← {path}"))
    }

    #[test]
    fn shell_quote_wraps_only_when_needed() {
        // A kebab path stays bare (the common case · no visual noise);
        // a spaced path is single-quoted so `nika run <it>` survives the
        // copy-paste (a wizard hand-off must not emit a broken command).
        assert_eq!(shell_quote("my-flow.nika.yaml"), "my-flow.nika.yaml");
        assert_eq!(shell_quote("a/b/c.nika.yaml"), "a/b/c.nika.yaml");
        assert_eq!(
            shell_quote("My Cool Flow.nika.yaml"),
            "'My Cool Flow.nika.yaml'"
        );
        // The `'` escape is the total POSIX form (close · escaped · open).
        assert_eq!(shell_quote("it's.nika.yaml"), r"'it'\''s.nika.yaml'");
        // Shell metacharacters (a `;`/`$`/`&` in a pasted name) are quoted.
        assert_eq!(shell_quote("a;rm -rf.yaml"), "'a;rm -rf.yaml'");
    }

    /// A unique EMPTY dir per test — the collision-aware default reads
    /// the base, so shared temp dirs would leak state between tests.
    fn fresh_base(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-wiz-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn dest(tag: &str) -> String {
        std::env::temp_dir()
            .join(format!("nika-new-{}-{tag}.nika.yaml", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn exact_template_name_stays_the_fast_path() {
        let d = dest("exact");
        let out = run("chain", Some(&d), true);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(!out.text.contains("routed"), "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    /// The ladder's second rung: an example slug (or filename · or
    /// showcase path) lands VERBATIM at dest — and the default dest
    /// flattens the tiering to the basename.
    #[test]
    fn example_sources_land_verbatim_through_new() {
        let dir = std::env::temp_dir().join(format!("nika-new-example-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dest = dir.join("mine.nika.yaml");
        let dest_s = dest.to_string_lossy().into_owned();

        let out = run("01-hello", Some(&dest_s), false);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(out.text.contains("example `01-hello`"), "{}", out.text);
        assert!(out.text.contains("nika check"), "{}", out.text);
        assert_eq!(
            std::fs::read_to_string(&dest).expect("written"),
            nika_pack::example("01-hello").expect("embedded"),
            "verbatim — a lesson has no SLOTs"
        );
        // Filename form + showcase tiering resolve too; overwrite refuses.
        assert_eq!(
            run("01-hello.nika.yaml", Some(&dest_s), true).code,
            codes::OK
        );
        assert_eq!(run("01-hello", Some(&dest_s), false).code, codes::ENV);
        let show = run("showcase/t1-price-watch", Some(&dest_s), true);
        assert_eq!(show.code, codes::OK, "{}", show.text);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The wire contract: `nika new --from '?'` (NO dest) names the set —
    /// and a discovery query answered is a SUCCESS (exit 0 · it used to
    /// reuse the unknown-template error, so the documented command read
    /// as a failure). The `embedded set:` line survives verbatim (the
    /// editor probes regex exactly that grammar).
    #[test]
    fn discovery_query_lists_the_set_without_a_dest() {
        let out = run("?", None, false);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(out.text.contains("embedded set:"), "{}", out.text);
        for name in nika_pack::template_names() {
            assert!(out.text.contains(&name), "lists `{name}`: {}", out.text);
        }
    }

    /// The listing derives its taglines from the template bodies' own
    /// `# TEMPLATE` headers — no second prose source to drift.
    #[test]
    fn discovery_taglines_come_from_the_bodies() {
        let body = nika_pack::template("chain").expect("chain embedded");
        let tag = tagline("chain", body);
        assert!(!tag.is_empty(), "chain header carries a tagline");
        assert!(
            run("?", None, false).text.contains(&tag),
            "the listing shows it"
        );
    }

    /// A REAL template with no dest can't be instantiated — ask for one
    /// (don't silently no-op or panic).
    #[test]
    fn known_template_without_dest_asks_for_a_path() {
        let out = run("chain", None, false);
        assert_eq!(out.code, codes::ENV, "{}", out.text);
        assert!(out.text.contains("pass a destination"), "{}", out.text);
    }

    #[test]
    fn parallel_intent_routes_to_fanout() {
        let d = dest("par");
        let out = run("summarize every item in parallel", Some(&d), true);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text.contains("routed intent → template `fanout`"),
            "{}",
            out.text
        );
        // Own-corpus by construction: the instantiated file IS the
        // embedded template verbatim.
        let written = std::fs::read_to_string(&d).expect("file written");
        assert_eq!(Some(written.as_str()), nika_pack::template("fanout"));
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn agentic_intent_routes_to_agent_loop() {
        let d = dest("agent");
        let out = run(
            "an autonomous budgeted agent that researches a topic",
            Some(&d),
            true,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(out.text.contains("template `agent-loop`"), "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn approval_intent_routes_to_a_gated_template() {
        let d = dest("gate");
        let out = run(
            "verify then wait for human approval before deploy",
            Some(&d),
            true,
        );
        assert_eq!(out.code, codes::OK, "{}", out.text);
        assert!(
            out.text.contains("`human-gated-ship`") || out.text.contains("`gate-and-act`"),
            "a gated template must win: {}",
            out.text
        );
        std::fs::remove_file(&d).ok();
    }

    #[test]
    fn zero_evidence_intent_keeps_the_wire_contract_error() {
        // Gibberish shares no term with any template — the honest unknown
        // (exit 2) still names the set on the `embedded set:` wire line.
        let out = run("zzzz qqqq xxxx", Some(&dest("zero")), true);
        assert_eq!(out.code, codes::FILE, "{}", out.text);
        assert!(out.text.contains("embedded set:"), "{}", out.text);
    }

    // NOTE · the bare-`nika new`-in-a-pipe contract (fail fast naming
    // `--from`) is pinned at the BINARY plane (bin_smoke) — is_terminal
    // inside `cargo test` reflects the invoking terminal, so an
    // in-process assert would flip between a laptop run and CI.

    #[test]
    fn dispatch_with_a_template_is_the_flag_path_unchanged() {
        let d = dest("dispatch");
        let out = dispatch(Some("chain"), Some(&d), true, PLAIN, &stub_audit);
        assert_eq!(out.code, codes::OK, "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    // ─── The wizard's pure parts ─────────────────────────────────────

    #[test]
    fn resolve_template_covers_the_three_rungs() {
        assert_eq!(resolve_template(""), ("chain".to_owned(), false));
        assert_eq!(resolve_template("fanout"), ("fanout".to_owned(), false));
        let (name, routed) = resolve_template("summarize every item in parallel");
        assert_eq!(name, "fanout");
        assert!(routed);
        // Zero evidence → the chain default, never a dead end.
        assert_eq!(resolve_template("zzzz qqqq"), ("chain".to_owned(), false));
    }

    #[test]
    fn the_model_menu_derives_from_the_catalog_local_first() {
        let menu = model_menu();
        assert!(menu.len() >= 2, "catalog carries the menu providers");
        assert!(
            menu[0].0.starts_with("ollama/"),
            "local first (presentation order): {menu:?}"
        );
        assert!(menu[1].0.starts_with("mock/"), "offline second: {menu:?}");
        // Every entry is a full provider/model wire id from the catalog.
        assert!(menu.iter().all(|(m, _)| m.contains('/')), "{menu:?}");
    }

    #[test]
    fn resolve_model_defaults_to_the_offline_mock() {
        let menu = model_menu();
        let default = resolve_model("", &menu);
        assert!(
            default.starts_with("mock/"),
            "Enter must never fail: {default}"
        );
        assert_eq!(resolve_model("1", &menu), menu[0].0);
        assert_eq!(
            resolve_model("ollama/llama3.2:3b", &menu),
            "ollama/llama3.2:3b"
        );
        // A number off the menu or a word without `/` falls back safe.
        assert!(resolve_model("99", &menu).starts_with("mock/"));
        assert!(resolve_model("gpt", &menu).starts_with("mock/"));
    }

    #[test]
    fn workflow_id_is_a_kebab_of_the_file_name() {
        assert_eq!(workflow_id("my-first.nika.yaml"), "my-first");
        assert_eq!(workflow_id("dir/Sub/PR Review.nika.yaml"), "pr-review");
        assert_eq!(workflow_id(".nika.yaml"), "my-first");
    }

    #[test]
    fn yaml_scalar_keeps_plain_bare_and_single_quotes_the_rest() {
        assert_eq!(yaml_scalar("mock/echo"), "mock/echo");
        assert_eq!(yaml_scalar("summarize"), "summarize");
        // Space · colon · backslash · quote → single-quoted, literal.
        assert_eq!(
            yaml_scalar("save to C:\\Users\\me"),
            "'save to C:\\Users\\me'"
        );
        assert_eq!(yaml_scalar("foo/bar: baz"), "'foo/bar: baz'");
        assert_eq!(yaml_scalar("it's a test"), "'it''s a test'");
        assert_eq!(yaml_scalar(""), "''");
    }

    #[test]
    fn stamped_file_survives_a_hostile_intent_and_model_string() {
        // The rust-pro HIGH: a backslash in the intent (a Windows path ·
        // a regex) and a YAML-significant model pick BOTH reached the
        // scalar unescaped → the fresh scaffold failed its OWN check
        // under a green ✔. Every stamp must now round-trip through the
        // REAL parser+check clean.
        let body = nika_pack::template("chain").expect("embedded");
        for (desc, model) in [
            ("save to C:\\Users\\me", "mock/echo"),
            ("match \\d+ then ship", "mock/echo"),
            ("it's a \"quoted\" job", "mock/echo"),
            ("a job", "foo/bar: baz"),
        ] {
            let stamped = stamp(body, "hostile", desc, Some(model));
            let parsed = nika_schema::parse(
                &stamped,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            );
            assert!(
                parsed.is_ok(),
                "desc={desc:?} model={model:?} must parse: {parsed:?}"
            );
            // The check ladder must not choke either (a dirty audit is
            // fine; a PARSE error at this point is the bug).
            let wf = parsed.expect("asserted ok above");
            let _ = nika_schema::check::check(&wf);
        }
    }

    #[test]
    fn every_embedded_template_audits_clean_or_is_a_documented_gap() {
        // The own-corpus law (#261): every embedded skeleton a fresh
        // scaffold can produce MUST audit clean — a red ladder on a first
        // scaffold is the self-contradiction the wizard exists to avoid.
        // This ratchet was MISSING (pack-integrity only hashes text), so
        // `api-upload-and-create` shipped in #257 failing its OWN
        // SECRETS-egress check, unnoticed, until a user-sim caught it.
        //
        // KNOWN GAP — an operator design call, NOT a template typo: the
        // ADR-092 flow model taints an authenticated `invoke`'s OUTPUT (a
        // secret in a fetch auth-header taints the response, exactly as a
        // secret in the body would), with no `infer`/`agent`-style prompt
        // exception and no output-declassification construct. So
        // EMPTY since 2026-07-10: the one former gap
        // (`api-upload-and-create` — a secret-authed response piped to
        // `outputs:` had NO sanctioned path) resolved via the
        // output-declassification this ratchet's note called for:
        // `egress: [{ to: "outputs" }]` (spec 01-envelope §egress · the
        // owner declassifies the workflow boundary itself). Every template
        // now passes its own audit; a dirty one fails this ratchet unless
        // a genuine flow-model design gap is documented here.
        const KNOWN_GAP: &[&str] = &[];
        let mut clean = 0_usize;
        for name in nika_pack::template_names() {
            let body = nika_pack::template(&name).expect("template embedded");
            let parsed = nika_schema::parse(
                body,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            );
            assert!(parsed.is_ok(), "{name}: template must parse: {parsed:?}");
            let wf = parsed.expect("asserted ok above");
            let is_gap = KNOWN_GAP.contains(&name.as_str());
            if nika_schema::check::check(&wf).is_clean() {
                assert!(
                    !is_gap,
                    "{name}: now audits CLEAN — remove it from KNOWN_GAP, the design gap is resolved"
                );
                clean += 1;
            } else {
                assert!(
                    is_gap,
                    "{name}: a fresh scaffold FAILS its own `nika check` (own-corpus law · #261) — \
                     fix the template, or (if a genuine flow-model design gap) document it in KNOWN_GAP"
                );
            }
        }
        assert!(clean >= 8, "expected >= 8 clean templates, got {clean}");
    }

    #[test]
    fn stamp_fills_exactly_the_three_known_slots() {
        for name in nika_pack::template_names() {
            let body = nika_pack::template(&name).expect("embedded");
            let stamped = stamp(body, "field-demo", "their problem", Some("mock/echo"));
            assert!(
                stamped.contains("workflow: field-demo"),
                "{name}: id stamped"
            );
            assert!(
                !stamped.contains("-template "),
                "{name}: no template id remnant"
            );
            assert!(
                stamped.contains("description: 'their problem'"),
                "{name}: description stamped (single-quoted YAML scalar)"
            );
            if body.lines().any(|l| l.starts_with("model: ")) {
                assert!(
                    stamped.contains("model: mock/echo"),
                    "{name}: model stamped"
                );
            }
        }
    }

    /// The whole conversation over an injected cursor: three Enters =
    /// the golden path (chain · default name · offline mock).
    #[test]
    fn read_wizard_three_enters_is_the_golden_path() {
        let base = fresh_base("golden");
        let mut input = std::io::Cursor::new(b"\n\n\n".to_vec());
        let mut out = Vec::new();
        let w = read_wizard(
            &mut input,
            &mut out,
            base.to_str().expect("utf8"),
            None,
            PLAIN,
        )
        .expect("io ok")
        .expect("not cancelled");
        assert_eq!(w.template, "chain");
        assert_eq!(w.dest, "my-first.nika.yaml");
        assert!(w.model.as_deref().is_some_and(|m| m.starts_with("mock/")));
        let shown = String::from_utf8(out).expect("utf8");
        assert!(shown.contains("template `chain`"), "{shown}");
        assert!(shown.contains("ollama/"), "menu shows local first: {shown}");
        std::fs::remove_dir_all(&base).ok();
    }

    /// A skeleton without a top-level `model:` gets NO model question —
    /// asking would promise a stamp the file doesn't carry. Two answers
    /// complete the flow and the conversation says where models live.
    #[test]
    fn read_wizard_skips_the_model_question_when_the_template_takes_none() {
        let base = fresh_base("permodel");
        // gate-and-act carries no top-level model line (exact name = rung 1).
        let mut input = std::io::Cursor::new(b"gate-and-act\n\n".to_vec());
        let mut out = Vec::new();
        let w = read_wizard(
            &mut input,
            &mut out,
            base.to_str().expect("utf8"),
            None,
            PLAIN,
        )
        .expect("io ok")
        .expect("not cancelled");
        assert_eq!(w.template, "gate-and-act");
        assert_eq!(w.model, None, "no model harvested");
        let shown = String::from_utf8(out).expect("utf8");
        assert!(
            shown.contains("models are per-task in this skeleton"),
            "{shown}"
        );
        assert!(
            !shown.contains("a number, or any provider/model"),
            "the question must not fire: {shown}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// The default file name walks past collisions — a wizard re-run must
    /// never dead-end on « exists » AFTER every question was answered.
    #[test]
    fn wizard_default_dest_walks_past_collisions() {
        let base = fresh_base("collide");
        let b = base.to_str().expect("utf8");
        assert_eq!(wizard_default_dest(b), "my-first.nika.yaml");
        std::fs::write(base.join("my-first.nika.yaml"), "x").expect("seed");
        assert_eq!(wizard_default_dest(b), "my-second.nika.yaml");
        std::fs::write(base.join("my-second.nika.yaml"), "x").expect("seed");
        std::fs::write(base.join("my-third.nika.yaml"), "x").expect("seed");
        assert_eq!(wizard_default_dest(b), "my-4.nika.yaml");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn template_takes_model_matches_every_body() {
        for name in nika_pack::template_names() {
            let body = nika_pack::template(&name).expect("embedded");
            let has = body.lines().any(|l| l.starts_with("model: "));
            assert_eq!(template_takes_model(body), has, "{name}");
        }
        // The split is real: both kinds exist in the embedded set.
        let kinds: Vec<bool> = nika_pack::template_names()
            .iter()
            .map(|n| template_takes_model(nika_pack::template(n).expect("embedded")))
            .collect();
        assert!(kinds.contains(&true) && kinds.contains(&false));
    }

    /// An intent answer routes; EOF mid-flow cancels instead of looping.
    #[test]
    fn read_wizard_routes_and_cancels_honestly() {
        let base = fresh_base("routes");
        let mut input =
            std::io::Cursor::new(b"process every item in parallel\nbatch\n2\n".to_vec());
        let mut out = Vec::new();
        let w = read_wizard(
            &mut input,
            &mut out,
            base.to_str().expect("utf8"),
            None,
            PLAIN,
        )
        .expect("io ok")
        .expect("not cancelled");
        assert_eq!(w.template, "fanout");
        assert_eq!(w.dest, "batch.nika.yaml", "the suffix is appended");

        let mut eof = std::io::Cursor::new(Vec::new());
        let mut out2 = Vec::new();
        assert!(
            read_wizard(
                &mut eof,
                &mut out2,
                base.to_str().expect("utf8"),
                None,
                PLAIN
            )
            .expect("io ok")
            .is_none(),
            "EOF = cancelled"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// Answering everything and THEN hitting an existing typed dest is
    /// the one wizard dead-end left by design (the refuse-overwrite law
    /// on a HUMAN-chosen name) — pin the honest ENV exit + the --force
    /// override (rust-pro e2e review finding #4).
    #[test]
    fn wizard_io_refuses_a_typed_existing_dest_and_force_overrides() {
        let base = fresh_base("refuse");
        std::fs::write(base.join("my-first.nika.yaml"), "taken").expect("seed");

        let mut input = std::io::Cursor::new(b"\nmy-first\n\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            base.to_str().expect("utf8"),
            None,
            false,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
        );
        assert_eq!(v.code, codes::ENV, "{}", v.text);
        assert!(
            v.text.contains("--force"),
            "teaches the override: {}",
            v.text
        );
        assert_eq!(
            std::fs::read_to_string(base.join("my-first.nika.yaml")).expect("read"),
            "taken",
            "refused = untouched"
        );

        let mut input2 = std::io::Cursor::new(b"\nmy-first\n\n".to_vec());
        let mut out2 = Vec::new();
        let v2 = wizard_io(
            base.to_str().expect("utf8"),
            None,
            true,
            PLAIN,
            &mut input2,
            &mut out2,
            &stub_audit,
        );
        assert_eq!(v2.code, codes::OK, "{}", v2.text);
        assert!(
            std::fs::read_to_string(base.join("my-first.nika.yaml"))
                .expect("read")
                .contains("workflow: my-first"),
            "--force overwrote with the stamped template"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// End-to-end over injected io: the file lands stamped + checkable.
    #[test]
    fn wizard_io_materializes_a_stamped_file() {
        let dir = std::env::temp_dir().join(format!("nika-wizard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let mut input = std::io::Cursor::new(b"\nfirst\n\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            dir.to_str().expect("utf8"),
            None,
            true,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
        );
        assert_eq!(v.code, codes::OK, "{}", v.text);
        assert!(v.text.contains("scriptable form"), "{}", v.text);
        // The wow contract: the wizard SHOWS the audit ladder — the file
        // arrives already checked, not with a suggestion to check.
        assert!(v.text.contains("audited"), "the ladder ran: {}", v.text);
        let written = std::fs::read_to_string(dir.join("first.nika.yaml")).expect("file written");
        assert!(written.contains("workflow: first"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn boilerplate_and_stopwords_do_not_route() {
        // Regression: route_intent indexed the `# SLOT:` scaffolding prose +
        // accepted any score > 0, so envelope/stopword queries spuriously
        // routed to a scaffold. They carry no SIGNAL → unknown · list.
        for garbage in [
            "the",
            "workflow",
            "template",
            "slot",
            "fill the lines",
            "a workflow",
        ] {
            assert!(
                route_intent(garbage).is_none(),
                "`{garbage}` must not route · got {:?}",
                route_intent(garbage)
            );
        }
        // …but a real intent still routes on its signal terms.
        assert!(route_intent("scrape a website and summarize").is_some());
        assert!(route_intent("review and approve before deploy").is_some());
    }
}
