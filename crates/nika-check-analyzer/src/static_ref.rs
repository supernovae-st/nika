// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared static-value resolver — substrate half of the check lanes.
//!
//! Descended from `nika-check`'s `walk.rs` 2026-08-25: the resolver is a
//! pure AST read every lane shares (cost ceiling · read-path lint · the
//! MODELS rung's via-default judgment · the thinking-seat law), and the
//! 15k wall on `nika-check` pushed the lanes that need it down here —
//! the substrate everything reads, beside the DAG edges. `nika-check`
//! re-exports [`static_literal_of`] at its historical path, so no lane's
//! call site moved.

use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;

/// THE shared static-value resolver: a whole-string bare
/// `${{ <authority>.<name> }}` over the two value authorities whose
/// declared value is static (`const.` · `inputs.`), resolved
/// to the LITERAL it declares — an untyped value, or a typed
/// declaration's literal `default:`. One resolver, every lane: the cost
/// ceiling counts `for_each` fan-outs through it, the read-path lint
/// resolves `nika:read` args through it, and the CLI's MODELS rung
/// judges a templated `model:`'s declared default through it. A private
/// per-lane copy is how lanes drift (measured 2026-07-29: the cost lane
/// resolved `${{ const.model }}`-class refs while MODELS skipped them
/// wholesale — same expression, two verdicts).
///
/// `None` for everything else — navigation (`.field` · `[0]`),
/// operators, concatenations, task refs, a name outside the expression
/// identifier grammar (`[A-Za-z0-9_]`), or a typed declaration with no
/// `default:` — which stays statically unknown: analysis never guesses.
///
/// The CEL index form `${{ inputs['name'] }}` / `${{ inputs["name"] }}`
/// is the same binding as the dotted form (N01 / issue 1319) — one
/// resolver, both spellings.
#[must_use]
pub fn static_literal_of<'w>(wf: &'w RawWorkflow, expr: &str) -> Option<&'w serde_json::Value> {
    let (authority, name) = bare_static_ref(expr)?;
    let block = match authority {
        "const." => &wf.consts,
        _ => &wf.inputs,
    };
    let (_, decl) = block.iter().find(|(k, _)| k.value == name)?;
    match decl {
        VarDecl::Untyped(v)
        | VarDecl::Typed {
            default: Some(v), ..
        } => Some(v),
        VarDecl::Typed { default: None, .. } => None,
    }
}

/// The parse half of [`static_literal_of`]: a whole-string bare
/// `${{ <authority>.<ident> }}` over the two IMMUTABLE value
/// authorities → `(authority-with-dot, name)`. Two identical such refs
/// denote the same runtime value even when no literal is declared —
/// inputs bind once per run, const never changes — which is what
/// the write-conflict scan keys on.
///
/// Also admits the CEL index spelling `${{ inputs['ident'] }}` /
/// `${{ inputs["ident"] }}` (and the `const` twin) — same name, same
/// value. Further navigation (`.field` · `[0]`), operators, or a name
/// outside the identifier grammar → `None`.
#[must_use]
pub fn bare_static_ref(expr: &str) -> Option<(&'static str, &str)> {
    let inner = expr.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    if let Some(name) = cel_index(inner, "inputs") {
        return Some(("inputs.", name));
    }
    if let Some(name) = cel_index(inner, "const") {
        return Some(("const.", name));
    }
    let (authority, name) = ["const.", "inputs."]
        .into_iter()
        .find_map(|ns| inner.strip_prefix(ns).map(|n| (ns, n)))?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some((authority, name))
}

/// `${{ inputs['name'] }}` / `${{ inputs["name"] }}` → the ident.
fn cel_index<'a>(inner: &'a str, ns: &str) -> Option<&'a str> {
    let rest = inner.strip_prefix(ns)?.trim_start();
    let rest = rest.strip_prefix('[')?.strip_suffix(']')?;
    let rest = rest.trim();
    let name = rest
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')))?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    #[test]
    fn dotted_and_index_forms_name_the_same_input() {
        assert_eq!(
            bare_static_ref("${{ inputs.model }}"),
            Some(("inputs.", "model"))
        );
        assert_eq!(
            bare_static_ref("${{ inputs['model'] }}"),
            Some(("inputs.", "model"))
        );
        assert_eq!(
            bare_static_ref("${{ inputs[\"model\"] }}"),
            Some(("inputs.", "model"))
        );
        assert_eq!(
            bare_static_ref("${{ const['seat'] }}"),
            Some(("const.", "seat"))
        );
        assert!(bare_static_ref("${{ inputs.provider }}/${{ inputs.name }}").is_none());
        assert!(bare_static_ref("${{ with.model }}").is_none());
        assert!(bare_static_ref("${{ inputs[''] }}").is_none());
        assert!(bare_static_ref("${{ inputs[model] }}").is_none());
    }

    #[test]
    fn index_form_resolves_the_declared_default() {
        let wf = parse(
            "nika: w\ninputs:\n  model: { type: string, default: \"mock/echo\" }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"ok\"] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("parses");
        let v = static_literal_of(&wf, "${{ inputs['model'] }}").expect("resolves");
        assert_eq!(v.as_str(), Some("mock/echo"));
        assert_eq!(
            static_literal_of(&wf, "${{ inputs.model }}").and_then(|x| x.as_str()),
            Some("mock/echo")
        );
    }
}
