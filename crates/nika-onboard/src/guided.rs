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

use crate::intent::{IntentContract, RoutingOutcome};
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

fn routing_prefix(contract: Option<&IntentContract>) -> &'static str {
    if contract.is_some() {
        "routed intent → "
    } else {
        ""
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
                "nika new hello   # one file that runs\nnothing to scaffold — pass an intent or a template name (`nika new '?'` lists the set)\nembedded set: {}",
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
    // B01 · `hello` is the 01-hello lesson. One file, one dest stem, one
    // model. The CLI first-wow door still intercepts `nika new hello`
    // before this crate; this ladder is the pack/onboard contract.
    let from = canonical_source(template);
    if let Some(body) = nika_pack::example(from) {
        return write_example(from, body, dest, force);
    }
    let (name, body, contract) = match nika_pack::template(from) {
        Some(body) => (from.to_owned(), body, None),
        None => match crate::intent::route(template) {
            RoutingOutcome::Routed {
                template: name,
                contract,
                ..
            } => {
                // route returns names from the WHOLE catalog (G-16: the
                // 26 human-worded jobs used to be invisible to the one
                // surface that takes human words) — a routed EXAMPLE
                // lands verbatim (the take gesture · fixtures follow via
                // the shared materializer), a routed skeleton
                // instantiates.
                if let Some(body) = nika_pack::example(&name) {
                    return with_cadence_note(write_example(&name, body, dest, force), &contract);
                }
                // A skeleton name — the lookup is total for the template
                // set; an empty body would be a pack bug surfaced as the
                // honest unknown error, never a panic.
                let Some(body) = nika_pack::template(&name) else {
                    return unknown(template);
                };
                (name, body, Some(contract))
            }
            RoutingOutcome::NeedsClarification { candidates, .. } => {
                return clarify(template, &candidates);
            }
        },
    };
    instantiate_skeleton(&name, body, dest, force, template, contract.as_ref())
}

/// Write a resolved skeleton: dest is required (the `?` discovery door
/// already returned), refuse an existing file without `--force`, and
/// a routed file is a DRAFT by construction (P0-10).
fn instantiate_skeleton(
    name: &str,
    body: &str,
    dest: Option<&str>,
    force: bool,
    uttered: &str,
    contract: Option<&IntentContract>,
) -> Outcome {
    // A known template needs a destination to instantiate into. The
    // `--from '?'` (or any unknown) discovery query already returned above
    // WITHOUT touching `dest` — that is the editor-integration wire
    // contract (`unknown()`), so listing the set must not require a dummy
    // path (the help promises `nika new '?'` works bare).
    let Some(dest) = dest else {
        // The taught line must be the command that WORKS pasted back: a
        // plain-words intent carries spaces, so it is re-echoed QUOTED
        // (gauntlet 2026-08-01 · Marco: the unquoted echo was a
        // different shell command than the one that resolved).
        return Outcome {
            text: format!(
                "template `{name}` resolved — pass a destination: nika new {} <dest>.nika.yaml",
                shell_quote(uttered)
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
    let routing = routing_prefix(contract);
    // A ROUTED file is a draft by construction (P0-10): the intent was
    // interpreted, not understood — the message says « draft », never
    // « ready », and hands over to `nika check` before any run.
    let text = if contract.is_some() {
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
    let out = Outcome {
        text,
        code: codes::OK,
    };
    match contract {
        Some(contract) => with_cadence_note(out, contract),
        None => out,
    }
}

/// A cadence clause in a routed intent gets its honest note: the file
/// owns the WORK, a scheduler owns WHEN (the trigger/workflow split —
/// a deployed schedule is cron/CI territory, never a YAML side
/// effect). Half of « Chaque lundi, analyser les tickets » used to
/// vanish without a word (gauntlet 08-01, Camille) — routing keeps
/// its focus on the work, and the dropped half is now NAMED. The
/// already-extracted contract is the single cadence decision; this
/// display must not run a second, broader lexicon over the utterance.
fn with_cadence_note(mut out: Outcome, contract: &IntentContract) -> Outcome {
    if out.code != codes::OK {
        return out;
    }
    if let Some(cadence) = contract
        .constraints
        .iter()
        .find_map(|constraint| constraint.strip_prefix("cadence:"))
    {
        let clause: String = cadence.chars().take(24).collect();
        let clause = clause.trim_end();
        let _ = write!(
            out.text,
            "\n  note · \u{ab}{clause}\u{2026}\u{bb} is a schedule — this file owns the WORK; a scheduler (cron · CI) owns WHEN it runs"
        );
    }
    out
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
    // The faceted top-3 (entry-doors redesign R1): each candidate names
    // its facet + its own one-line description — recognition, not
    // recall. A description line is the entry's OWN wording (the yaml
    // `description:`), the closest thing to the human's sentence.
    let rows: String = candidates
        .iter()
        .map(|name| {
            let facet = crate::intent::facet_of(name).noun();
            match description_of(name) {
                Some(d) => format!("\n    {name}  ({facet}) — {d}"),
                None => format!("\n    {name}  ({facet})"),
            }
        })
        .collect();
    // The taught command must WORK in the taught context (gauntlet
    // 2026-08-01 · Priya: `nika new human-gated-ship` — a skeleton —
    // exited rc=3 asking for the destination the hint never mentioned).
    // An example lands bare; a skeleton carries its dest.
    let first = candidates.first().map_or("chain", String::as_str);
    let taught = if nika_pack::example(first).is_some() {
        format!("nika new {first}")
    } else {
        format!("nika new {first} <dest>.nika.yaml")
    };
    Outcome {
        text: format!(
            "`{intent}` doesn't route confidently — closest matches:{rows}\n  hint: take one by name (`{taught}`) or rephrase with what goes in and what comes out",
        ),
        code: codes::FILE,
    }
}

/// One line in the entry's OWN words — its banner sentence, clipped at
/// 72 chars (the clarify row must stay a row). Source of truth for the
/// wording, and WHY it moved there: `banner`.
fn description_of(name: &str) -> Option<String> {
    let body = nika_pack::template(name).or_else(|| nika_pack::example(name))?;
    let text = crate::banner::sentence(body)?;
    Some(if text.chars().count() > 72 {
        let mut short: String = text.chars().take(72).collect();
        short.push('…');
        short
    } else {
        text
    })
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
    // The pack's self-referential path (`# Run · nika run
    // examples/<slug>.nika.yaml`) becomes the OWNED destination: a
    // taught command inside the user's own file must work in the
    // user's own workspace (gauntlet 08-01: pasting the copied
    // comment exited 3 — the example path exists only in the pack).
    let body = body.replace(&format!("examples/{clean}.nika.yaml"), dest);
    if let Err(e) = std::fs::write(dest, &body) {
        return Outcome {
            text: format!("cannot write {dest}: {e}"),
            code: codes::ENV,
        };
    }
    let body = body.as_str();
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
    // A scaffold that needs a model seat names the keyless lane BESIDE
    // the real one — never inside it. On a machine with no model wired,
    // the bare `nika run` this line teaches is the FIRST thing the
    // reader tries and the first thing that fails: the run stops at
    // NIKA-INFER-001 before any wire call (measured 2026-08-03,
    // first-run review — the tool walked its own reader into a wall it
    // had just built). The run error does teach the same escape, but a
    // next step should not need a rescue.
    let keyless = if needs_a_seat(body) {
        " · no model wired yet? add `--model mock/echo` to rehearse keyless"
    } else {
        ""
    };
    Outcome {
        text: format!(
            "{dest} ← example `{clean}` · yours now — `nika check {q}` then `nika run {q}`{keyless}{ingredients}",
            q = shell_quote(dest)
        ),
        code: codes::OK,
    }
}

/// Whether the scaffolded body has a task that needs a model seat —
/// `infer:` or `agent:`, the two verbs that reach a provider.
///
/// A SCAN, not a parse, and the honest reason is the dependency graph:
/// `nika-schema` is a dev-dependency of this crate (the own-corpus
/// ratchet uses it test-side), and pulling the parser into the scaffold
/// path to decide one courtesy line is not a trade this line earns.
/// The scan is narrow — an action key at the head of its line, comments
/// skipped — and both ways it can be wrong are cheap: a task literally
/// named `infer` gets an unneeded hint, an unreachable phrasing gets
/// none. Neither can block or mislead a run.
fn needs_a_seat(body: &str) -> bool {
    body.lines().any(|line| {
        let head = line.trim_start();
        !head.starts_with('#') && (head.starts_with("infer:") || head.starts_with("agent:"))
    })
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
        "\n\ntry ·\n  nika new chain my-first.nika.yaml\n  nika new \"describe the job in plain words\" my.nika.yaml   # routes across jobs · lessons · skeletons\n  nika new                                                          # guided (terminal only)",
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
// wizard can stamp honestly (id · model); the remaining
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
    // The wizard's corpus IS the skeleton set (its whole gesture is a
    // file of SLOTs to fill) — it routes within it; route() itself now
    // sees the whole catalog (G-16 · the guided door's world).
    match crate::intent::route_skeletons(intent) {
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

/// `hello` / `hello.nika.yaml` → the 01-hello lesson (B01 · one hello).
fn canonical_source(template: &str) -> &str {
    let t = template.strip_suffix(".nika.yaml").unwrap_or(template);
    match t {
        "hello" => "01-hello",
        other => other,
    }
}

/// Inline comment that MATCHES the stamped seat (B16). A leftover
/// « local » next to `openai/…` is the lie this exists to refuse.
fn model_line_comment(model: &str) -> &'static str {
    if model == "mock/echo" || model.starts_with("mock/") {
        "rehearsal · zero key · swap for any catalog seat"
    } else if model.starts_with("ollama/")
        || model.starts_with("llamacpp/")
        || model.starts_with("vllm/")
        || model.starts_with("native/")
    {
        "local · zero key · swap for any catalog seat"
    } else {
        "catalog seat · swap for mock/echo to rehearse keyless"
    }
}

/// Stamp the answers the wizard KNOWS into the template — id · model
/// (the second only when the wizard asked, i.e. the skeleton carries a
/// top-level `model:` at column 0 — the stamp's anchor). Never stamp
/// what wasn't answered.
///
/// The identity rides `nika:` at column 0. There is no description slot
/// any more: the envelope stopped carrying one, and stamping the human's
/// intent sentence into a comment would be a promise the file cannot
/// keep (nothing reads it back).
pub(crate) fn stamp(body: &str, id: &str, model: Option<&str>) -> String {
    let mut out: String = body
        .lines()
        .map(|line| {
            if line.starts_with("nika: ") {
                format!("nika: {id}")
            } else if let (true, Some(model)) = (line.starts_with("model: "), model) {
                format!(
                    "model: {}   # {}",
                    yaml_scalar(model),
                    model_line_comment(model)
                )
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
    let stamped = stamp(body, &id, w.model.as_deref());
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
    // The run hint's parenthetical follows the CHOSEN seat — the old
    // literal « (mock is offline · $0.00) » rode every handoff, even
    // an Ollama pick (checkpoint fixture journey-fr-posthog-local),
    // and « $0.00 » is the banned free-shaped claim: a rehearsal is a
    // rehearsal, local compute is unpriced, neither is a price.
    let run_note = match w.model.as_deref() {
        Some(m) if m.starts_with("mock/") => {
            "mock rehearsal · offline, not a real answer".to_owned()
        }
        Some(m) => format!("model `{m}` · cost per its catalog seat"),
        None => "models per-task".to_owned(),
    };
    let next = format!(
        "next ·\n  $EDITOR {q}                   # fill the remaining `# SLOT:` lines\n  nika run {q}                  # execute · live render ({run_note})\n\n{}",
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
