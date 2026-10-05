// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure ACP model and mode projections; existing selectors moved without changing behavior.
use serde_json::Value;

/// The `category: "model"` config option, when advertised.
pub(super) fn model_option(config_options: Option<&Value>) -> Option<&Value> {
    config_options?
        .as_array()?
        .iter()
        .find(|o| o.get("category").and_then(Value::as_str) == Some("model"))
}

/// The model the session serves now: the option's current value, else the legacy list's.
pub(super) fn current_model(option: Option<&Value>, models: Option<&Value>) -> Option<String> {
    option
        .and_then(|o| o.get("currentValue"))
        .and_then(Value::as_str)
        .or_else(|| models?.get("currentModelId")?.as_str())
        .map(str::to_owned)
}

/// The model the caller wants (`provider/name` keeps its name; `default` and an empty name
/// leave the harness's own choice).
pub(super) fn wanted(requested: Option<&str>) -> Option<String> {
    let requested = requested?.trim();
    let name = requested.rsplit('/').next().unwrap_or(requested).trim();
    (!name.is_empty() && !name.eq_ignore_ascii_case("default")).then(|| name.to_owned())
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Whether an offered alias stands as a whole segment of the requested name — bounded by
/// the name's edges or its separators (`sonnet` in `claude-sonnet-4-5`, `grok-4.7` in
/// `xai-grok-4.7`), never a substring (`son` matches nothing) and never a bare number.
fn family_word(offered: &str, wanted: &str) -> bool {
    let offered = offered.trim();
    if offered.is_empty() || !offered.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let haystack = wanted.to_ascii_lowercase();
    let needle = offered.to_ascii_lowercase();
    let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_ascii_alphanumeric());
    let mut from = 0;
    while let Some(at) = haystack[from..].find(&needle) {
        let start = from + at;
        let end = start + needle.len();
        if boundary(haystack[..start].chars().next_back())
            && boundary(haystack[end..].chars().next())
        {
            return true;
        }
        from = end;
    }
    false
}

/// The choice that names the wanted model: exactly (value or name) first, then by its
/// family word.
fn choose<'a>(
    choices: impl Iterator<Item = &'a Value> + Clone,
    value_key: &str,
    wanted: &str,
) -> Option<&'a Value> {
    let exact = choices.clone().find(|c| {
        c.get(value_key)
            .and_then(Value::as_str)
            .is_some_and(|v| same(v, wanted))
            || c.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| same(n, wanted))
    });
    exact.or_else(|| {
        choices.into_iter().find(|c| {
            c.get(value_key)
                .and_then(Value::as_str)
                .is_some_and(|v| family_word(v, wanted))
        })
    })
}

/// The (config id, value) that names the wanted model among the option's choices, by value
/// or by display name.
pub(super) fn offered_option(option: Option<&Value>, wanted: &str) -> Option<(String, Value)> {
    let option = option?;
    let id = option.get("id")?.as_str()?.to_owned();
    let choice = choose(option.get("options")?.as_array()?.iter(), "value", wanted)?;
    Some((id, choice.get("value")?.clone()))
}

/// The legacy list's `modelId` that names the wanted model, by id or by display name.
pub(super) fn offered_model(models: Option<&Value>, wanted: &str) -> Option<String> {
    choose(
        models?.get("availableModels")?.as_array()?.iter(),
        "modelId",
        wanted,
    )?
    .get("modelId")?
    .as_str()
    .map(str::to_owned)
}

/// Every model the agent offers, for the refusal's teaching line.
pub(super) fn offered_names(option: Option<&Value>, models: Option<&Value>) -> String {
    let mut names: Vec<String> = Vec::new();
    if let Some(choices) = option
        .and_then(|o| o.get("options"))
        .and_then(Value::as_array)
    {
        names.extend(
            choices
                .iter()
                .filter_map(|c| c.get("value").and_then(Value::as_str).map(str::to_owned)),
        );
    }
    if let Some(list) = models
        .and_then(|m| m.get("availableModels"))
        .and_then(Value::as_array)
    {
        names.extend(
            list.iter()
                .filter_map(|m| m.get("modelId").and_then(Value::as_str).map(str::to_owned)),
        );
    }
    if names.is_empty() {
        "(it advertises no model choice; `default` is its own)".to_owned()
    } else {
        names.join(" · ")
    }
}

/// How a mode is set: the v1 config option when advertised, else the deprecated method.
pub(super) enum ModeDoor {
    Config { config_id: String, value: Value },
    Mode { mode_id: String },
}

/// Whether an advertised mode fits the intent (`read-only` → a plan / read-only mode).
fn fits(intent: &str, id: &str, name: &str) -> bool {
    let id = id.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    match intent {
        "read-only" => {
            id.ends_with("plan")
                || id.contains("read-only")
                || id.contains("read_only")
                || name.contains("plan")
                || name.contains("read only")
                || name.contains("read-only")
        }
        other => id == other.to_ascii_lowercase() || name == other.to_ascii_lowercase(),
    }
}

/// The door to the mode that fits the intent, if the agent advertises one.
pub(super) fn mode_door(session: &super::wire::NewSessionResult, intent: &str) -> Option<ModeDoor> {
    let by_option = session
        .config_options
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|options| {
            options
                .iter()
                .find(|o| o.get("category").and_then(Value::as_str) == Some("mode"))
        })
        .and_then(|option| {
            let id = option.get("id")?.as_str()?.to_owned();
            let choice = option.get("options")?.as_array()?.iter().find(|c| {
                fits(
                    intent,
                    c.get("value").and_then(Value::as_str).unwrap_or(""),
                    c.get("name").and_then(Value::as_str).unwrap_or(""),
                )
            })?;
            Some(ModeDoor::Config {
                config_id: id,
                value: choice.get("value")?.clone(),
            })
        });
    if by_option.is_some() {
        return by_option;
    }
    session
        .modes
        .as_ref()
        .and_then(|m| m.get("availableModes"))
        .and_then(Value::as_array)
        .and_then(|modes| {
            modes.iter().find(|m| {
                fits(
                    intent,
                    m.get("id").and_then(Value::as_str).unwrap_or(""),
                    m.get("name").and_then(Value::as_str).unwrap_or(""),
                )
            })
        })
        .and_then(|m| {
            m.get("id")?.as_str().map(|id| ModeDoor::Mode {
                mode_id: id.to_owned(),
            })
        })
}
