// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika new <template|intent> <dest>` — instantiate one of the
//! embedded skeletons (spec §2). Refuses to overwrite (the human keeps
//! the hand); `--force` is the explicit override. The written file is
//! the template VERBATIM — slots stay visible so the author fills them
//! deliberately.
//!
//! `--from` resolves in two rungs:
//! 1. **exact template name** — instant, unchanged;
//! 2. **intent routing** — anything else is read into an `IntentContract`
//!    FIRST (crates/nika-onboard/src/intent.rs · deterministic lexicon,
//!    zero LLM), then BM25-ranked (the admitted `nika-bm25` crate ·
//!    Robertson-Zaragoza Okapi) against the template names + bodies, with
//!    a small everyday-word → Nika-vocab alias bridge on the QUERY side
//!    (« scrape » → fetch · « summarize » → infer · « parallel » →
//!    fan-out). A candidate must carry every capability the contract
//!    requires, the winner must clear an absolute score floor AND a 1.3×
//!    margin over the runner-up: below that bar the answer is an honest
//!    clarification naming the closest skeletons (P0-1 · P0-10) — never
//!    a silent guess, and NOTHING is written. A confident route
//!    instantiates as a DRAFT (the message says so) and hands over to
//!    `nika check`; a zero-evidence query gets the honest
//!    unknown-template error. The routing is SAID
//!    (`routed intent → template …`). Deterministic, zero-LLM — the floor
//!    of « the binary generates the best workflow for the intent »
//!    (editor LLM loops build ON this rung).

use std::fmt::Write as _;
use std::io::{BufRead, IsTerminal};
use std::path::Path;

use nika_display::theme::{Role, Theme};

use crate::intent::RoutingOutcome;
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
        // The third door (V5 grammar): a lone `<name>.nika.yaml` that
        // resolves to NO embedded example names a DESTINATION, not a
        // source — the extension is the tell. A terminal gets the wizard
        // with the given name as the file default; a pipe gets the
        // honest pointer (never a silent intent-route on a filename).
        Some(f)
            if dest.is_none() && f.ends_with(".nika.yaml") && nika_pack::example(f).is_none() =>
        {
            if interactive() {
                let stdin = std::io::stdin();
                wizard_io(
                    ".",
                    Some(f),
                    force,
                    theme,
                    &mut stdin.lock(),
                    &mut std::io::stdout(),
                    audit,
                )
            } else {
                Outcome {
                    text: format!(
                        "`{f}` names a destination — say what it should DO: nika new \"<intent>\" {f}"
                    ),
                    code: codes::FILE,
                }
            }
        }
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
                "nothing to scaffold — pass an intent or a template name (`nika new '?'` lists the set)\nembedded set: {}",
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
    // (slug or filename) — then plain-words intent. The showroom side
    // (`nika try <slug>`) runs the same corpus without taking it.
    if let Some(body) = nika_pack::example(template) {
        return write_example(template, body, dest, force);
    }
    let (name, body, routed) = match nika_pack::template(template) {
        Some(body) => (template.to_owned(), body, false),
        None => match crate::intent::route(template) {
            RoutingOutcome::Routed { template: name, .. } => {
                // route only returns names from template_names() —
                // the lookup is total; an empty body would be a pack bug
                // surfaced as the honest unknown error, never a panic.
                let Some(body) = nika_pack::template(&name) else {
                    return unknown(template);
                };
                (name, body, true)
            }
            RoutingOutcome::NeedsClarification { candidates, .. } => {
                return clarify(template, &candidates);
            }
        },
    };
    // A known template needs a destination to instantiate into. The
    // `--from '?'` (or any unknown) discovery query already returned above
    // WITHOUT touching `dest` — that is the editor-integration wire
    // contract (`unknown()`), so listing the set must not require a dummy
    // path (the help promises `nika new '?'` works bare).
    let Some(dest) = dest else {
        return Outcome {
            text: format!(
                "template `{name}` resolved — pass a destination: nika new {template} <dest>.nika.yaml"
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
    // A ROUTED file is a draft by construction (P0-10): the intent was
    // interpreted, not understood — the message says « draft », never
    // « ready », and hands over to `nika check` before any run.
    let text = if routed {
        format!(
            "{dest} ← {routing}template `{name}` — a DRAFT scaffold · fill the `# SLOT:` lines, then `nika check {q}` before any run",
            q = shell_quote(dest)
        )
    } else {
        format!(
            "{dest} ← template `{name}` · fill the `# SLOT:` lines then `nika check {q}`",
            q = shell_quote(dest)
        )
    };
    Outcome {
        text,
        code: codes::OK,
    }
}

/// The below-the-bar finding (P0-10): a weak or ambiguous route writes
/// NOTHING — the honest error names the 2-3 closest skeletons so the
/// human picks one explicitly (or rephrases). Zero candidates = zero
/// evidence → the unknown-template wire contract (editor probes parse
/// its `embedded set:` line). Same exit family as the unknown finding
/// (`codes::FILE`): the file the human asked for does not exist yet.
fn clarify(intent: &str, candidates: &[String]) -> Outcome {
    if candidates.is_empty() {
        return unknown(intent);
    }
    Outcome {
        text: format!(
            "`{intent}` doesn't route confidently — closest skeletons: {}\n  hint: name one explicitly (`nika new {} <dest>.nika.yaml`) or rephrase with the job's verbs (fetch · summarize · parallel · approve…)",
            candidates.join(" · "),
            candidates.first().map_or("chain", String::as_str),
        ),
        code: codes::FILE,
    }
}

/// An EXAMPLE source lands verbatim (a lesson, complete — no SLOTs);
/// the receipt says the check-then-run road. The default destination is
/// the slug's basename (a nested slug flattens — the tiering belongs to
/// the pack, your workspace is flat).
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
    let ingredients = match crate::fixtures::materialize(body, Path::new(dest)) {
        Ok((0, 0)) => String::new(),
        Ok((written, kept)) => {
            let kept_note = if kept > 0 {
                format!(" · {kept} already yours, kept")
            } else {
                String::new()
            };
            format!(
                "\n  examples/fixtures · {written} file{} (the recipe's ingredients){kept_note}",
                if written == 1 { "" } else { "s" },
            )
        }
        Err(e) => {
            return Outcome {
                text: format!("cannot write a fixture beside {dest}: {e}"),
                code: codes::ENV,
            };
        }
    };
    Outcome {
        text: format!(
            "{dest} ← example `{clean}` · yours now — `nika check {q}` then `nika run {q}`{ingredients}",
            q = shell_quote(dest)
        ),
        code: codes::OK,
    }
}

/// The unknown-template finding. The `embedded set:` line is a WIRE
/// CONTRACT — editor integrations probe `nika new '?'` and parse
/// the set from exactly this shape; never reword it.
fn unknown(template: &str) -> Outcome {
    Outcome {
        text: format!(
            "no template or intent matches `{template}` — embedded set: {}\n  hint: name one, describe the job with its verbs (fetch · summarize · parallel · approve…), or pass an example slug from `nika try`",
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
        "\n\nexamples work here too (complete lessons · verbatim) ·\n  nika new 01-hello my-hello.nika.yaml    # any slug from `nika try`",
    );
    text.push_str(
        "\n\ntry ·\n  nika new chain my-first.nika.yaml\n  nika new \"describe the job in plain words\" my.nika.yaml   # routes to the closest skeleton\n  nika new                                                          # guided (terminal only)",
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

/// One prompt, re-asked until the answer RESOLVES (P0-8): Enter takes
/// the announced default (unchanged behavior) · a NON-EMPTY answer the
/// parser refuses is SAID (« unrecognized ») and asked again — a typo
/// must never become a silent default · EOF cancels, honestly (`None`,
/// never a loop). The parser sees only non-empty answers.
pub(crate) fn ask_validated<T>(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
    prompt: &str,
    hint: &str,
    parse: impl Fn(&str) -> Option<T>,
    default: impl Fn() -> T,
) -> std::io::Result<Option<T>> {
    loop {
        let Some(raw) = ask(input, out, theme, prompt)? else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Ok(Some(default()));
        }
        if let Some(value) = parse(&raw) {
            return Ok(Some(value));
        }
        let said = format!("unrecognized `{raw}` — {hint}");
        writeln!(out, "  {}", theme.paint(Role::Bad, &said))?;
    }
}

/// The wizard's template rung: exact name first, confident intent route
/// second, the announced Enter default third — and the SAID fallback
/// last (P0-1: the chain fallback used to be SILENT; a below-the-bar
/// intent now names its closest candidates before starting from the
/// generic spine, so the human sees the guess instead of inheriting it).
enum WizardRoute {
    /// The answer IS a template name.
    Exact(String),
    /// The intent cleared the confidence bar.
    Routed(String),
    /// Empty answer — the golden path the prompt announces (`[chain]`).
    Default,
    /// Below the bar: `chain`, but SAID, with the closest candidates.
    Fallback {
        /// The 2-3 closest skeletons (empty on zero evidence).
        candidates: Vec<String>,
    },
}

/// Intent → template rung, per [`WizardRoute`].
fn resolve_template(intent: &str) -> WizardRoute {
    if intent.is_empty() {
        return WizardRoute::Default;
    }
    if nika_pack::template(intent).is_some() {
        return WizardRoute::Exact(intent.to_owned());
    }
    match crate::intent::route(intent) {
        RoutingOutcome::Routed { template, .. } => WizardRoute::Routed(template),
        RoutingOutcome::NeedsClarification { candidates, .. } => {
            WizardRoute::Fallback { candidates }
        }
    }
}

/// The ollama menu note. « local » is a TOPOLOGY claim (P0-20): with an
/// endpoint override active (`NIKA_OLLAMA_BASE_URL` · `OLLAMA_HOST`) the
/// engine may be a LAN box, so the note drops « local » and says what is
/// actually known — the sovereign protocol · zero key · a custom
/// endpoint. The env probe stays out of this pure pick (testable, no
/// `set_var` race).
const fn ollama_note_for(override_active: bool) -> &'static str {
    if override_active {
        "sovereign · zero key · custom endpoint"
    } else {
        "local · sovereign · zero key"
    }
}

/// Presence-only read of the ollama endpoint-override family — the value
/// is connection config and is never bound (the probe layer's
/// PRESENT-NOT-PRINTED discipline).
#[allow(clippy::disallowed_methods)] // presence-only · an endpoint override is config, not a secret
fn ollama_endpoint_overridden() -> bool {
    ["NIKA_OLLAMA_BASE_URL", "OLLAMA_HOST"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|val| !val.is_empty()))
}

/// The provider menu, DERIVED from the embedded catalog (no hardcoded
/// model names to drift) in the doctrine presentation order · local
/// first · offline mock · EU open-weight · then the US clouds.
pub(crate) fn model_menu() -> Vec<(String, &'static str)> {
    let export = nika_catalog::export::catalog_export();
    [
        ("ollama", ollama_note_for(ollama_endpoint_overridden())),
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

/// A menu number or a full `provider/model` → the model. `None` = the
/// answer is NEITHER (a bare word like « gpt » · a number off the menu)
/// — the caller re-asks instead of inheriting a silent mock (P0-8).
/// Enter never reaches here: the ask loop maps it to [`default_model`].
pub(crate) fn resolve_model(pick: &str, menu: &[(String, &'static str)]) -> Option<String> {
    if let Ok(n) = pick.parse::<usize>() {
        return n
            .checked_sub(1)
            .and_then(|i| menu.get(i))
            .map(|(m, _)| m.clone());
    }
    if pick.contains('/') {
        return Some(pick.to_owned());
    }
    None
}

/// The Enter default — the offline mock (the one answer that succeeds
/// with zero keys and zero network — the first run must not be able to
/// fail).
pub(crate) fn default_model(menu: &[(String, &'static str)]) -> String {
    menu.get(1)
        .map_or_else(|| "mock/echo".to_owned(), |(m, _)| m.clone())
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
/// anchor). Never stamp what wasn't answered. W1 « the map »: the id and
/// description live INSIDE the workflow object (`  id:` · `  description:`
/// at indent 2, under the `workflow:` head).
pub(crate) fn stamp(body: &str, id: &str, description: &str, model: Option<&str>) -> String {
    let mut out: String = body
        .lines()
        .map(|line| {
            if line.starts_with("  id: ") {
                format!("  id: {id}")
            } else if line.starts_with("  description: ") && !description.is_empty() {
                format!("  description: {}", yaml_scalar(description))
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
    let Some(model) = ask_validated(
        input,
        out,
        theme,
        &format!(
            "a number, or any provider/model {}",
            theme.paint(Role::Dim, "[2]")
        ),
        &format!("choose 1-{} or a provider/model", menu.len()),
        |raw| resolve_model(raw, &menu),
        || default_model(&menu),
    )?
    else {
        return Ok(None);
    };
    writeln!(out, "  → model `{}`", theme.paint(Role::Accent, &model))?;
    Ok(Some(model))
}

/// Resolve the wizard's template from the intent — the fallback is
/// SAID (P0-1): the human reads WHY chain and what almost matched
/// before the file exists.
fn routed_template(
    intent: &str,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<(String, bool)> {
    Ok(match resolve_template(intent) {
        WizardRoute::Exact(name) => (name, false),
        WizardRoute::Routed(name) => (name, true),
        WizardRoute::Default => ("chain".to_owned(), false),
        WizardRoute::Fallback { candidates } => {
            let note = if candidates.is_empty() {
                "no skeleton matches that intent — starting from `chain`, the generic spine"
                    .to_owned()
            } else {
                format!(
                    "no confident match (closest: {}) — starting from `chain`, the generic spine",
                    candidates.join(" · ")
                )
            };
            writeln!(out, "  {}", theme.paint(Role::Dim, &note))?;
            ("chain".to_owned(), false)
        }
    })
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
    let (template, routed) = routed_template(&intent, out, theme)?;
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
            &format!("scriptable form · nika new {} {q}", w.template)
        ),
    );
    Outcome {
        text: format!("{wrote}\n\n{}\n\n{next}", audit.text.trim_end()),
        code: audit.code,
    }
}

#[cfg(test)]
mod tests;
