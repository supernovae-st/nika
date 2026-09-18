// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Init bootstrap helpers: explicit recipe/model choices and terminal prompts.
use nika_display::theme::{Role, Theme};
use std::io::BufRead;

fn yaml_scalar(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-');
    if !value.is_empty() && value.chars().all(plain) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

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

pub(crate) fn template_takes_model(body: &str) -> bool {
    body.lines().any(|l| l.starts_with("model: "))
}

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

const fn ollama_note_for(override_active: bool) -> &'static str {
    if override_active {
        "sovereign · zero key · custom endpoint"
    } else {
        "local · sovereign · zero key"
    }
}

#[allow(clippy::disallowed_methods)] // presence-only bootstrap connection configuration
fn ollama_endpoint_overridden() -> bool {
    ["NIKA_OLLAMA_BASE_URL", "OLLAMA_HOST"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|val| !val.is_empty()))
}

pub(crate) fn model_menu() -> Vec<(String, &'static str)> {
    let export = nika_catalog::export::catalog_export();
    [
        ("ollama", ollama_note_for(ollama_endpoint_overridden())),
        ("mock", "simulated inference · zero model key"),
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

pub(crate) fn default_model(menu: &[(String, &'static str)]) -> String {
    menu.get(1)
        .map_or_else(|| "mock/echo".to_owned(), |(m, _)| m.clone())
}

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

#[cfg(test)]
mod tests;
