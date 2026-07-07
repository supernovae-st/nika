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

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, exit};

/// `nika new` — resolve the missing `--from` per clig.dev: a terminal
/// gets the guided flow (prompt for the missing argument) · a pipe/CI
/// fails fast naming the flag (never REQUIRE interactivity).
#[must_use]
pub fn dispatch(from: Option<&str>, dest: Option<&str>, force: bool, theme: Theme) -> VerbOutput {
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
            )
        }
        None => VerbOutput {
            text: format!(
                "nothing to scaffold — pass --from <template|intent> (`nika new --from '?'` lists the set)\nembedded set: {}",
                nika_pack::template_names().join(" · ")
            ),
            code: exit::FILE,
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
pub fn run(template: &str, dest: Option<&str>, force: bool) -> VerbOutput {
    // `?` is the documented discovery query — a question answered is a
    // SUCCESS, not a finding (it used to reuse the unknown-template error
    // and exit 2, which read as failure to a human following the help).
    if template == "?" {
        return discovery();
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
        return VerbOutput {
            text: format!(
                "template `{name}` resolved — pass a destination: nika new --from {template} <dest>.nika.yaml"
            ),
            code: exit::ENV,
        };
    };
    if Path::new(dest).exists() && !force {
        return VerbOutput {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: exit::ENV,
        };
    }
    if let Err(e) = std::fs::write(dest, body) {
        return VerbOutput {
            text: format!("cannot write {dest}: {e}"),
            code: exit::ENV,
        };
    }
    let routing = if routed { "routed intent → " } else { "" };
    VerbOutput {
        text: format!(
            "{dest} ← {routing}template `{name}` · fill the `# SLOT:` lines then `nika check {dest}`"
        ),
        code: exit::OK,
    }
}

/// The unknown-template finding. The `embedded set:` line is a WIRE
/// CONTRACT — editor integrations probe `nika new --from '?'` and parse
/// the set from exactly this shape; never reword it.
fn unknown(template: &str) -> VerbOutput {
    VerbOutput {
        text: format!(
            "unknown template `{template}` — embedded set: {}",
            nika_pack::template_names().join(" · ")
        ),
        code: exit::FILE,
    }
}

/// First-class `--from '?'` — the listing a human reads (name + tagline
/// derived from each template's own `# TEMPLATE` header · no second
/// source) with the `embedded set:` WIRE-CONTRACT line kept verbatim for
/// the editor probes. Exit 0: a discovery query answered is a success.
fn discovery() -> VerbOutput {
    let names = nika_pack::template_names();
    let mut text = String::from("the embedded template skeletons ·\n");
    for name in &names {
        let tag = nika_pack::template(name).map_or_else(String::new, |b| tagline(name, b));
        let _ = writeln!(text, "  {name:<18} {tag}");
    }
    let _ = write!(text, "\nembedded set: {}", names.join(" · "));
    text.push_str(
        "\n\ntry ·\n  nika new --from chain my-first.nika.yaml\n  nika new --from \"describe the job in plain words\" my.nika.yaml   # routes to the closest skeleton\n  nika new                                                          # guided (terminal only)",
    );
    VerbOutput {
        text,
        code: exit::OK,
    }
}

/// The one-line tagline out of a template's `# TEMPLATE · <name> · …`
/// header. Empty when a body carries no header — the listing degrades
/// gracefully instead of inventing prose. A header that wraps to the
/// next comment line gets an honest `…` instead of ending mid-thought.
fn tagline(name: &str, body: &str) -> String {
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
    model: String,
    intent: String,
}

/// The default first-workflow file name (shared with the init hand-off).
const WIZARD_DEST: &str = "my-first.nika.yaml";

/// One prompt · one line back. `None` = EOF (the human left — cancel,
/// never loop). The `>` is the single accent — the conversation's
/// running line, same semantic slot as the run render's active task.
fn ask(
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
fn model_menu() -> Vec<(String, &'static str)> {
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
fn resolve_model(pick: &str, menu: &[(String, &'static str)]) -> String {
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
fn workflow_id(dest: &str) -> String {
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

/// Stamp the three answers the wizard KNOWS into the template — id ·
/// description · model. Line-anchored on the template contract (every
/// skeleton opens those keys at column 0); templates without a top-level
/// `model:` (per-task models) are left alone.
fn stamp(body: &str, id: &str, description: &str, model: &str) -> String {
    let mut out: String = body
        .lines()
        .map(|line| {
            if line.starts_with("workflow: ") {
                format!("workflow: {id}")
            } else if line.starts_with("description: ") && !description.is_empty() {
                format!("description: \"{description}\"")
            } else if line.starts_with("model: ") {
                format!("model: {model}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// Run the three questions over injected io. `Ok(None)` = cancelled.
/// Styling stays inside the semantic seam (theme.rs): the brand mark on
/// the header · dim for defaults/metadata · the accent on resolutions.
/// Every register without colour keeps byte-identical text (paint is a
/// no-op), so the conversation reads the same in a transcript.
fn read_wizard(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
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

    let default_dest = dest_hint.unwrap_or(WIZARD_DEST);
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
    Ok(Some(Wizard {
        template,
        routed,
        dest,
        model,
        intent,
    }))
}

/// The wizard over real io: converse, then materialize the file.
pub(crate) fn wizard_io(
    base: &str,
    dest_hint: Option<&str>,
    force: bool,
    theme: Theme,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
) -> VerbOutput {
    match read_wizard(input, out, dest_hint, theme) {
        Err(e) => VerbOutput {
            text: format!("wizard i/o failed: {e}"),
            code: exit::ENV,
        },
        Ok(None) => VerbOutput {
            text: "cancelled — nothing written".to_owned(),
            code: exit::ENV,
        },
        Ok(Some(w)) => materialize(base, &w, force, theme),
    }
}

/// `nika init`'s hand-off question — one keypress, Enter = yes (the
/// golden path). `None` = declined (init prints its classic next block).
pub(crate) fn offer_first_workflow(base: &str, report: &str, theme: Theme) -> Option<VerbOutput> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut stdout = std::io::stdout();
    let out: &mut dyn std::io::Write = &mut stdout;
    // The scaffold report prints HERE (through the writer, macro-free —
    // the lib bans println!): the conversation happens below it.
    write!(out, "{report}").ok()?;
    let answer = ask(
        &mut input,
        &mut *out,
        theme,
        &format!(
            "\nscaffold your first workflow now? {}",
            theme.paint(Role::Dim, "[Y/n]")
        ),
    )
    .ok()??;
    if answer.eq_ignore_ascii_case("n") || answer.eq_ignore_ascii_case("no") {
        return None;
    }
    Some(wizard_io(base, None, false, theme, &mut input, out))
}

/// Write the stamped template, then RUN the audit and show the ladder —
/// the wizard hands over a CHECKED workflow, not a suggestion to check
/// (audit-before-run is the differentiator; the first minute must show
/// it, not name it). Also teach the scriptable form of what the wizard
/// just did — reproducibility is part of the contract.
fn materialize(base: &str, w: &Wizard, force: bool, theme: Theme) -> VerbOutput {
    let dest = if Path::new(&w.dest).is_absolute() || base == "." {
        w.dest.clone()
    } else {
        Path::new(base).join(&w.dest).to_string_lossy().into_owned()
    };
    let Some(body) = nika_pack::template(&w.template) else {
        return unknown(&w.template);
    };
    if Path::new(&dest).exists() && !force {
        return VerbOutput {
            text: format!("{dest} exists — pass --force to overwrite"),
            code: exit::ENV,
        };
    }
    let id = workflow_id(&dest);
    let description = w.intent.replace('"', "");
    let stamped = stamp(body, &id, &description, &w.model);
    if let Err(e) = std::fs::write(&dest, &stamped) {
        return VerbOutput {
            text: format!("cannot write {dest}: {e}"),
            code: exit::ENV,
        };
    }
    let routing = if w.routed { "routed intent → " } else { "" };
    let wrote = format!(
        "{} {dest} ← {routing}template `{tpl}` · stamped workflow `{id}` · model `{model}`",
        theme.paint(Role::Good, "✔"),
        tpl = w.template,
        model = w.model,
    );
    // The audit runs NOW — the ladder on screen inside the first minute
    // is the product's argument (a red ladder would honestly propagate,
    // but a fresh scaffold checks clean by the templates' own-corpus law).
    let audit = crate::verbs::check::run(&dest, false, false, theme);
    let next = format!(
        "next ·\n  $EDITOR {dest}                   # fill the remaining `# SLOT:` lines\n  nika run {dest}                  # execute · live render (mock is offline · $0.00)\n\n{}",
        theme.paint(
            Role::Dim,
            &format!("scriptable form · nika new --from {} {dest}", w.template)
        ),
    );
    VerbOutput {
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
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(!out.text.contains("routed"), "{}", out.text);
        std::fs::remove_file(&d).ok();
    }

    /// The wire contract: `nika new --from '?'` (NO dest) names the set —
    /// and a discovery query answered is a SUCCESS (exit 0 · it used to
    /// reuse the unknown-template error, so the documented command read
    /// as a failure). The `embedded set:` line survives verbatim (the
    /// editor probes regex exactly that grammar).
    #[test]
    fn discovery_query_lists_the_set_without_a_dest() {
        let out = run("?", None, false);
        assert_eq!(out.code, exit::OK, "{}", out.text);
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
        assert_eq!(out.code, exit::ENV, "{}", out.text);
        assert!(out.text.contains("pass a destination"), "{}", out.text);
    }

    #[test]
    fn parallel_intent_routes_to_fanout() {
        let d = dest("par");
        let out = run("summarize every item in parallel", Some(&d), true);
        assert_eq!(out.code, exit::OK, "{}", out.text);
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
        assert_eq!(out.code, exit::OK, "{}", out.text);
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
        assert_eq!(out.code, exit::OK, "{}", out.text);
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
        assert_eq!(out.code, exit::FILE, "{}", out.text);
        assert!(out.text.contains("embedded set:"), "{}", out.text);
    }

    // NOTE · the bare-`nika new`-in-a-pipe contract (fail fast naming
    // `--from`) is pinned at the BINARY plane (bin_smoke) — is_terminal
    // inside `cargo test` reflects the invoking terminal, so an
    // in-process assert would flip between a laptop run and CI.

    #[test]
    fn dispatch_with_a_template_is_the_flag_path_unchanged() {
        let d = dest("dispatch");
        let out = dispatch(Some("chain"), Some(&d), true, PLAIN);
        assert_eq!(out.code, exit::OK, "{}", out.text);
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
    fn stamp_fills_exactly_the_three_known_slots() {
        for name in nika_pack::template_names() {
            let body = nika_pack::template(&name).expect("embedded");
            let stamped = stamp(body, "field-demo", "their problem", "mock/echo");
            assert!(
                stamped.contains("workflow: field-demo"),
                "{name}: id stamped"
            );
            assert!(
                !stamped.contains("-template "),
                "{name}: no template id remnant"
            );
            assert!(
                stamped.contains("description: \"their problem\""),
                "{name}: description stamped"
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
        let mut input = std::io::Cursor::new(b"\n\n\n".to_vec());
        let mut out = Vec::new();
        let w = read_wizard(&mut input, &mut out, None, PLAIN)
            .expect("io ok")
            .expect("not cancelled");
        assert_eq!(w.template, "chain");
        assert_eq!(w.dest, WIZARD_DEST);
        assert!(w.model.starts_with("mock/"));
        let shown = String::from_utf8(out).expect("utf8");
        assert!(shown.contains("template `chain`"), "{shown}");
        assert!(shown.contains("ollama/"), "menu shows local first: {shown}");
    }

    /// An intent answer routes; EOF mid-flow cancels instead of looping.
    #[test]
    fn read_wizard_routes_and_cancels_honestly() {
        let mut input =
            std::io::Cursor::new(b"process every item in parallel\nbatch\n2\n".to_vec());
        let mut out = Vec::new();
        let w = read_wizard(&mut input, &mut out, None, PLAIN)
            .expect("io ok")
            .expect("not cancelled");
        assert_eq!(w.template, "fanout");
        assert_eq!(w.dest, "batch.nika.yaml", "the suffix is appended");

        let mut eof = std::io::Cursor::new(Vec::new());
        let mut out2 = Vec::new();
        assert!(
            read_wizard(&mut eof, &mut out2, None, PLAIN)
                .expect("io ok")
                .is_none(),
            "EOF = cancelled"
        );
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
        );
        assert_eq!(v.code, exit::OK, "{}", v.text);
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
