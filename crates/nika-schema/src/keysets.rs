// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parser's own key vocabularies, exposed as ONE read surface.
//!
//! Every list here is the EXACT const the parser validates against
//! (and builds its did-you-mean suggestions from) — a consumer that
//! offers keys (the LSP · a form UI) reads THIS door and can never
//! drift from what the parser accepts. A new field lands in the
//! parser const; every surface learns it for free (the born-stale
//! law, key edition).

use crate::parser::{envelope, tasks, verbs};

/// The child keys legal inside the block introduced by `block_key` —
/// `None` when the key introduces no closed keyset this door knows
/// (free-form maps like `args:` / `with:` / `env:`, or an unknown
/// key). `parent` is the block's OWN enclosing key when known —
/// `fs:`/`net:` are permits vocabulary ONLY under `permits:` (an
/// `fs:` argument of some tool stays free-form).
#[must_use]
pub fn known_child_keys(block_key: &str, parent: Option<&str>) -> Option<&'static [&'static str]> {
    match block_key {
        "retry" => Some(tasks::RETRY_KEYS),
        "on_error" => Some(tasks::ON_ERROR_KEYS),
        "on_finally" => Some(tasks::FINALLY_KEYS),
        "infer" => Some(verbs::INFER_KEYS),
        "exec" => Some(verbs::EXEC_KEYS),
        "invoke" => Some(verbs::INVOKE_KEYS),
        "agent" => Some(verbs::AGENT_KEYS),
        "thinking" => Some(verbs::THINKING_KEYS),
        "permits" => Some(envelope::PERMITS_KEYS),
        "fs" if parent == Some("permits") => Some(envelope::PERMITS_FS_KEYS),
        "net" if parent == Some("permits") => Some(envelope::PERMITS_NET_KEYS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The door serves the parser's OWN lists — spot-pin the members
    /// that define each register (full parity is the consts themselves:
    /// this door returns them by reference, no copy exists to drift).
    #[test]
    fn door_returns_the_parser_consts() {
        let retry = known_child_keys("retry", None).expect("retry keyset");
        assert!(retry.contains(&"max_attempts") && retry.contains(&"backoff_strategy"));
        let infer = known_child_keys("infer", None).expect("infer keyset");
        assert!(infer.contains(&"prompt") && infer.contains(&"thinking"));
        assert!(
            known_child_keys("args", None).is_none(),
            "free-form maps stay free"
        );
    }

    /// `fs:`/`net:` are permits vocabulary ONLY under `permits:` — an
    /// `fs:` key anywhere else is not this door's business.
    #[test]
    fn fs_and_net_require_the_permits_parent() {
        assert!(known_child_keys("fs", Some("permits")).is_some());
        assert!(known_child_keys("fs", None).is_none());
        assert!(known_child_keys("net", Some("args")).is_none());
    }
}
