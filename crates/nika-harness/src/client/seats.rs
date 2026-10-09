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

/// A match by the exact requested id first, since a session may advertise the qualified
/// `provider/name` verbatim; then by the provider-less name `wanted` keeps, the compatibility
/// mapping. Either way only an id or a name the peer offered matches: nothing is guessed.
pub(super) fn exact_first<T>(
    requested: Option<&str>,
    wanted: &str,
    find: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    (requested.map(str::trim))
        .filter(|id| !same(id, wanted))
        .and_then(&find)
        .or_else(|| find(wanted))
}

/// Match only a value or display name the live peer offered. Model identifiers
/// are opaque: a family substring cannot authorize dropping a requested suffix.
/// Exact config choices remain preferred by the caller; otherwise an exact
/// legacy choice can carry the complete request through `session/set_model`.
fn choose<'a>(
    choices: impl Iterator<Item = &'a Value> + Clone,
    value_key: &str,
    wanted: &str,
) -> Option<&'a Value> {
    choices
        .clone()
        .find(|c| {
            c.get(value_key)
                .and_then(Value::as_str)
                .is_some_and(|v| same(v, wanted))
        })
        .or_else(|| {
            choices.into_iter().find(|c| {
                c.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| same(name, wanted))
            })
        })
}

/// A select option's choices with its groups flattened (ACP lets `options` be either flat
/// values or `{group, name, options}` groups, never mixed), in advertised order.
pub(super) fn choices(option: &Value) -> Vec<&Value> {
    let Some(items) = option.get("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .flat_map(|item| match item.get("options").and_then(Value::as_array) {
            Some(group) if item.get("group").is_some() => group.iter().collect::<Vec<_>>(),
            _ => vec![item],
        })
        .collect()
}

/// Every advertised value of a select option, verbatim, groups flattened.
pub(super) fn choice_values(option: &Value) -> Vec<String> {
    choices(option)
        .into_iter()
        .filter_map(|c| c.get("value").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// The (config id, value) that names the wanted model among the option's choices, by value
/// or by display name.
pub(super) fn offered_option(option: Option<&Value>, wanted: &str) -> Option<(String, Value)> {
    let option = option?;
    let id = option.get("id")?.as_str()?.to_owned();
    let choice = choose(choices(option).into_iter(), "value", wanted)?;
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
    let mut names: Vec<String> = option.map(choice_values).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A session advertising the qualified id verbatim (as value and display name, or as a legacy
    /// `modelId`) is matched by the exact request before the provider prefix is dropped; a session
    /// offering only the bare name still matches through the compatibility mapping; and a near
    /// name is never taken for the requested one.
    #[test]
    fn the_exact_advertised_id_is_matched_before_the_provider_prefix_is_dropped() {
        let qualified = json!({"id": "model", "currentValue": "openai/gpt-5.4",
            "options": [{"value": "openai/gpt-5.4", "name": "openai/gpt-5.4"},
                        {"value": "openai/gpt-5.5", "name": "openai/gpt-5.5"}]});
        let requested = Some("openai/gpt-5.5");
        let wanted = wanted(requested).expect("a model is asked");
        assert_eq!(wanted, "gpt-5.5");
        let option = |name: &str| offered_option(Some(&qualified), name);
        assert_eq!(
            exact_first(requested, &wanted, option).map(|(_, value)| value),
            Some(json!("openai/gpt-5.5"))
        );
        assert_eq!(
            option(&wanted),
            None,
            "the stripped name alone finds nothing here"
        );
        let legacy = json!({"currentModelId": "openai/gpt-5.4",
            "availableModels": [{"modelId": "openai/gpt-5.5", "name": "openai/gpt-5.5"}]});
        let model = |name: &str| offered_model(Some(&legacy), name);
        assert_eq!(
            exact_first(requested, &wanted, model).as_deref(),
            Some("openai/gpt-5.5")
        );
        let bare = json!({"id": "model", "options": [{"value": "gpt-5.5", "name": "GPT-5.5"}]});
        let bare_option = |name: &str| offered_option(Some(&bare), name);
        assert_eq!(
            exact_first(requested, &wanted, bare_option).map(|(_, value)| value),
            Some(json!("gpt-5.5")),
            "the compatibility mapping still holds"
        );
        let near = json!({"id": "model", "options": [{"value": "openai/gpt-5.5-mini"}]});
        let near_option = |name: &str| offered_option(Some(&near), name);
        assert_eq!(
            exact_first(requested, &wanted, near_option),
            None,
            "no guess"
        );
    }
}
