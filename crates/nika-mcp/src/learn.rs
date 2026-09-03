// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The LEARN half's pack lookups — `nika_examples` and `nika_template`
//! (the embedded showroom and the skeletons), split out of `tools.rs`
//! at the file cap when the builtin route landed (#1270).

use serde_json::{Value, json};

/// `nika_examples` — return the JSONL metadata index (`slug` · `form` ·
/// `one_line` · `cost`), one example's full workflow source (the LEARNING
/// surface: read the canonical example for a construct instead of guessing),
/// or — with `builtin` — the examples that call one `nika:*` tool.
pub(crate) fn examples(args: &Value) -> Result<String, String> {
    if let Some(builtin) = args.get("builtin").and_then(Value::as_str) {
        return examples_using(builtin);
    }
    match args.get("slug").and_then(Value::as_str) {
        None => example_index(),
        Some(slug) => match nika_pack::example(slug) {
            Some(body) => Ok(body.to_owned()),
            // RAMS-11: a PLAIN-WORDS miss routes through the SAME door
            // the CLI walks (`nika new <words>` · whole catalog) — one
            // router, one calibration. Only multi-word queries route:
            // a single token is a slug (typo'd or adversarial) and the
            // unknown-key contract holds — the router never sees the
            // traversal-shaped garbage the adversarial pin feeds.
            None if !slug.trim().contains(' ') => Err(format!(
                "unknown example `{slug}` — call nika_examples without arguments \
                 for the list"
            )),
            None => match nika_onboard::routing::route_query(slug) {
                nika_onboard::routing::RoutedEntry::Example(name) => {
                    let body = nika_pack::example(&name).unwrap_or_default();
                    Ok(format!("# routed: `{slug}` → example `{name}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Skeleton(name) => {
                    let body = nika_pack::template(&name).unwrap_or_default();
                    Ok(format!("# routed: `{slug}` → template `{name}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Clarify(candidates) => Err(format!(
                    "`{slug}` doesn't route confidently — closest: {} · call \
                     nika_examples without arguments for the list",
                    candidates.join(" · ")
                )),
            },
        },
    }
}

/// The « which example uses builtin X » route (#1270): one JSONL row per
/// embedded example whose body calls the tool as a WHOLE token (`nika:jq`
/// never matches `nika:jq_extra`), in pack order. The name is validated
/// against the SAME projection `nika_tools` serves — an unknown builtin is
/// a refusal that points at the catalog, never a silent empty; a key is a
/// key, never a path (the adversarial pin holds here too).
fn examples_using(query: &str) -> Result<String, String> {
    let name = if query.starts_with(nika_builtin::NAMESPACE) {
        query.to_owned()
    } else {
        format!("{}{query}", nika_builtin::NAMESPACE)
    };
    let tools = nika_builtin::tools_json();
    let known = tools["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["name"].as_str())
        .any(|n| n == name);
    if !known {
        return Err(format!(
            "unknown builtin `{name}` — read the names from nika_tools, never from memory"
        ));
    }
    let rows: Vec<String> = nika_pack::example_slugs()
        .into_iter()
        .filter_map(|slug| {
            let body = nika_pack::example(&slug)?;
            let sites = nika_migrate::repair::word_sites(body, &name).len();
            (sites > 0).then(|| {
                let meta = nika_pack::meta(&slug, body);
                json!({
                    "slug": slug,
                    "builtin": name,
                    "one_line": meta.title,
                    "sites": sites,
                })
                .to_string()
            })
        })
        .collect();
    if rows.is_empty() {
        return Ok(format!(
            "no embedded example calls `{name}` — read its contract from nika_tools and \
             the nearest form from nika_examples"
        ));
    }
    Ok(rows.join("\n"))
}

/// The retrieval-friendly example index: one JSON object per embedded file.
/// Every field derives from the same source the CLI showroom reads — no second
/// catalog to drift. Task count is the honest static cost available in-pack.
fn example_index() -> Result<String, String> {
    nika_pack::example_slugs()
        .into_iter()
        .map(|slug| {
            let body = nika_pack::example(&slug).ok_or_else(|| {
                format!("embedded example `{slug}` is listed but cannot be resolved")
            })?;
            let meta = nika_pack::meta(&slug, body);
            Ok(json!({
                "slug": slug,
                "form": meta.verbs,
                "one_line": meta.title,
                "cost": { "tasks": meta.tasks },
            })
            .to_string())
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|rows| rows.join("\n"))
}

/// `nika_template` — list the canonical skeleton names, or return one
/// skeleton's source (copy · fill the `# SLOT:` lines · never invent shape).
pub(crate) fn template(args: &Value) -> Result<String, String> {
    match args.get("name").and_then(Value::as_str) {
        None => Ok(nika_pack::template_names().join("\n")),
        Some(name) => match nika_pack::template(name) {
            Some(body) => Ok(body.to_owned()),
            // RAMS-11: a PLAIN-WORDS miss routes within the SKELETON set
            // — the CLI wizard's door (SLOTs to fill). Single tokens keep
            // the unknown-key contract (see nika_examples).
            None if !name.trim().contains(' ') => Err(format!(
                "unknown template `{name}` — call nika_template without arguments \
                 for the list"
            )),
            None => match nika_onboard::routing::route_skeleton_query(name) {
                nika_onboard::routing::RoutedEntry::Skeleton(routed) => {
                    let body = nika_pack::template(&routed).unwrap_or_default();
                    Ok(format!("# routed: `{name}` → template `{routed}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Example(routed) => {
                    let body = nika_pack::example(&routed).unwrap_or_default();
                    Ok(format!("# routed: `{name}` → example `{routed}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Clarify(candidates) => Err(format!(
                    "`{name}` doesn't route confidently — closest: {} · call \
                     nika_template without arguments for the list",
                    candidates.join(" · ")
                )),
            },
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::{Value, json};

    use crate::tools::execute;

    /// A naive whole-token scan of the embedded bodies — the oracle's
    /// answer must match it slug for slug, so the route can never
    /// hide a precedent or invent one.
    fn slugs_calling(tool: &str) -> Vec<String> {
        nika_pack::example_slugs()
            .into_iter()
            .filter(|slug| {
                let body = nika_pack::example(slug).expect("listed slug resolves");
                body.match_indices(tool).any(|(at, _)| {
                    !body[at + tool.len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                })
            })
            .collect()
    }

    #[test]
    fn examples_route_by_builtin_lists_every_example_that_calls_it() {
        let out = execute("nika_examples", &json!({ "builtin": "nika:jq" })).expect("routes");
        let rows: Vec<Value> = out
            .lines()
            .map(|line| serde_json::from_str(line).expect("each row is JSON"))
            .collect();
        let got: Vec<String> = rows
            .iter()
            .map(|r| r["slug"].as_str().expect("slug").to_owned())
            .collect();
        let expected = slugs_calling("nika:jq");
        assert!(!expected.is_empty(), "the showroom uses nika:jq");
        assert_eq!(
            got, expected,
            "one row per example that calls it, pack order"
        );
        for row in &rows {
            assert_eq!(row["builtin"], "nika:jq");
            assert!(row["one_line"].is_string(), "{row}");
            assert!(row["sites"].as_u64().expect("count") >= 1, "{row}");
            assert_eq!(
                row.as_object().expect("row").len(),
                4,
                "slug · builtin · one_line · sites"
            );
        }
    }

    #[test]
    fn examples_route_by_builtin_accepts_the_bare_name() {
        let namespaced = execute("nika_examples", &json!({ "builtin": "nika:jq" })).expect("ok");
        let bare = execute("nika_examples", &json!({ "builtin": "jq" })).expect("ok");
        assert_eq!(namespaced, bare);
    }

    #[test]
    fn examples_route_by_builtin_refuses_an_unknown_builtin_naming_the_catalog() {
        let err = execute("nika_examples", &json!({ "builtin": "nika:ghost" }))
            .expect_err("no such builtin");
        assert!(err.contains("unknown builtin `nika:ghost`"), "{err}");
        assert!(
            err.contains("nika_tools"),
            "names the catalog to read: {err}"
        );
        // A key is a KEY, never a path (the adversarial pin, this route).
        for evil in ["../../etc/passwd", "nika:../x", "nika:jq\n"] {
            let e = execute("nika_examples", &json!({ "builtin": evil })).expect_err("refused");
            assert!(e.contains("unknown builtin"), "{evil:?}: {e}");
        }
    }

    #[test]
    fn examples_route_by_builtin_says_when_no_example_uses_it() {
        let tools = nika_builtin::tools_json();
        let unused = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .find(|name| slugs_calling(name).is_empty());
        let Some(name) = unused else {
            return; // every builtin has a precedent — nothing to pin here
        };
        let out = execute("nika_examples", &json!({ "builtin": name })).expect("an honest empty");
        assert!(out.contains("no embedded example"), "{out}");
        assert!(out.contains(name), "{out}");
    }

    #[test]
    fn the_builtin_route_is_declared_on_nika_examples() {
        let listed = crate::tools::catalog();
        let tool = listed
            .as_array()
            .expect("array")
            .iter()
            .find(|t| t["name"] == "nika_examples")
            .expect("served");
        let builtin = &tool["inputSchema"]["properties"]["builtin"];
        assert_eq!(builtin["type"], json!("string"), "{tool:#}");
        assert!(
            builtin["description"]
                .as_str()
                .expect("described")
                .contains("nika:"),
            "{tool:#}"
        );
    }

    /// ADR-124 · the plugin's teaching surface is derived, never typed:
    /// every example slug the engine-owned authoring skill names resolves
    /// in the pack. Measured 2026-09-03: the intent table taught six
    /// `tN-` slugs no example carried (`t1-meeting-actions` …) and no test
    /// read it. This one does.
    #[test]
    fn the_authoring_skill_teaches_only_living_example_slugs() {
        let skill = include_str!("../../../.agents/plugins/nika/skills/nika-authoring/SKILL.md");
        let slug_shaped = |token: &str| {
            token.contains('-')
                && !token.starts_with('-')
                && token
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        // Every backticked token anywhere in the skill that wears the
        // example-slug shape — `NN-name` or `tN-name` — must resolve.
        let numbered = |token: &str| {
            let (head, _) = token.split_once('-').unwrap_or((token, ""));
            head.len() == 2
                && (head.bytes().all(|b| b.is_ascii_digit())
                    || (head.starts_with('t') && head.as_bytes()[1].is_ascii_digit()))
        };
        let mut table_rows = 0;
        let mut in_table = false;
        for line in skill.lines() {
            if line.starts_with("### Which example answers which intent") {
                in_table = true;
                continue;
            }
            if in_table && line.starts_with("###") {
                in_table = false;
            }
            for token in line.split('`').skip(1).step_by(2) {
                if !slug_shaped(token) {
                    continue;
                }
                let must_resolve = numbered(token) || (in_table && line.starts_with('|'));
                if !must_resolve {
                    continue;
                }
                if in_table && line.starts_with('|') {
                    table_rows += 1;
                }
                assert!(
                    nika_pack::example(token).is_some(),
                    "the skill teaches `{token}`, which no example carries: {line}"
                );
            }
        }
        assert!(
            table_rows >= 15,
            "the intent table was read ({table_rows} slugs)"
        );
    }
}
