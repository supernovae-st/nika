// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Lexical reference typos. These heads are diagnostics, never resolved edges.

use crate::scan_islands;

/// Find dotted Nika reference heads after a single `${` opener.
///
/// Genuine `${{ ... }}` islands, including quoted bodies, are opaque. An
/// escaped opener has no island extent. Currency, shell parameters and namespace-prefix
/// collisions are not reference typos. A missing closing brace does not hide a
/// recognized head. Consumers may diagnose it or withhold unused-name advice;
/// they must never resolve it as a real reference.
#[must_use]
pub fn single_brace_reference_heads(text: &str) -> Vec<&str> {
    let (islands, end) = match scan_islands(text) {
        Ok(islands) => (islands, text.len()),
        // The unterminated real island is already a VAR-008 refusal. Only
        // inspect its closed prefix; the shared lexer defines every mask.
        Err(error) => (
            scan_islands(&text[..error.offset()]).unwrap_or_default(),
            error.offset(),
        ),
    };
    let mut heads = Vec::new();
    let mut cursor = 0;
    for island in islands {
        gap_heads(&text[cursor..island.start], &mut heads);
        cursor = island.end;
    }
    gap_heads(&text[cursor..end], &mut heads);
    heads
}

fn gap_heads<'a>(text: &'a str, heads: &mut Vec<&'a str>) {
    for (start, _) in text.match_indices("${") {
        if !text[start..].starts_with("${{")
            && let Some(head) = reference_head(&text[start + 2..])
        {
            heads.push(head);
        }
    }
}

fn reference_head(body: &str) -> Option<&str> {
    let body = body.trim_start();
    let root_end = body.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')?;
    if !matches!(
        &body[..root_end],
        "inputs" | "const" | "secrets" | "with" | "tasks" | "group" | "item" | "index"
    ) {
        return None;
    }
    let rest = body[root_end..]
        .trim_start()
        .strip_prefix('.')?
        .trim_start();
    let name_len = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    Some(&body[..body.len() - rest.len() + name_len])
}

#[cfg(test)]
mod tests {
    use super::single_brace_reference_heads as heads;

    #[test]
    fn locates_only_dotted_namespace_heads_without_resolving_them() {
        assert_eq!(
            heads("é ${ const.seed } and ${tasks.left.output}"),
            ["const.seed", "tasks.left"]
        );
        assert_eq!(heads("${inputs . topic"), ["inputs . topic"]);
        assert_eq!(heads("${ const. }"), ["const. "]);
        assert_eq!(heads("${\u{00a0}const .seed}"), ["const .seed"]);
        assert!(heads("${\u{001c}const.seed}").is_empty());
        assert!(
            heads("$5 ${HOME} ${name:-default} ${items[0]} ${constants.seed} ${index}").is_empty()
        );
    }

    #[test]
    fn only_real_islands_mask_their_bodies() {
        assert!(heads(r"${{ '${ const.seed }' }}").is_empty());
        assert_eq!(heads(r"\${{ '${ const.seed }' }}"), ["const.seed"]);
        assert_eq!(heads(r"${{ '}}' }} ${const.seed}"), ["const.seed"]);
        assert_eq!(heads(r"\${{ inputs.x }} ${ const.seed }"), ["const.seed"]);
        assert!(heads("${{ '${ const.seed }'").is_empty());
    }

    #[test]
    fn escaped_openers_never_hide_later_typos() {
        assert_eq!(heads(r"\${{ don't }} then ${ const.seed }"), ["const.seed"]);
        assert_eq!(
            heads(r"\${{ don't }} ${const.seed} then ' }}"),
            ["const.seed"]
        );
        assert!(heads(r"\${{ don't }} then $5").is_empty());
        assert_eq!(heads("${const.seed} then ${{ dangling"), ["const.seed"]);
    }
}
