// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the picker says of an API key, by name only: the variable that supplies it (the first
//! set one in the order a run reads them), a warning when two set variables hold different
//! values (the probe recorded the flag at startup; no value is read or kept here), and the
//! verdict of a key check the person asked for. A key's value never reaches this module.

use nika_cli_host::probe::ProviderProbe;
use nika_session::KeyCheck;

use crate::lang::{Voice, fill};

/// A provider's set key variables by name, in the order a run reads them, and whether two hold
/// different values, as this machine's probe recorded them at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::session) struct KeyNames {
    /// The provider's id.
    pub(in crate::session) provider: String,
    /// The set variables' names; the first supplies the key.
    pub(in crate::session) vars: Vec<String>,
    /// Two of them hold different values.
    pub(in crate::session) conflict: bool,
}

/// The key names of each probed provider that has a key variable set.
pub(in crate::session) fn names_of(probes: &[ProviderProbe]) -> Vec<KeyNames> {
    (probes.iter())
        .filter(|probe| !probe.key_vars.is_empty())
        .map(|probe| KeyNames {
            provider: probe.id.clone(),
            vars: probe.key_vars.clone(),
            conflict: probe.key_conflict,
        })
        .collect()
}

/// The key words of `provider`: `key: VAR`, or the warning when two set variables differ;
/// `None` when no key variable is set.
pub(super) fn words(
    keys: &[KeyNames],
    provider: &str,
    voice: crate::lang::Voice,
) -> Option<String> {
    let set = keys.iter().find(|names| names.provider == provider)?;
    let said = named(&set.vars, set.conflict)?;
    // The variable's name stays as the run reads it; the word before it is the person's.
    Some(match said.strip_prefix("key: ") {
        Some(var) => crate::lang::fill(voice.say("key: {var}"), &[("{var}", var)]),
        None => said,
    })
}

/// The words of the set key variables `vars` (in the run's order) and their conflict flag.
fn named(vars: &[String], conflict: bool) -> Option<String> {
    let used = vars.first()?;
    if conflict && vars.len() > 1 {
        let others = vars.get(1..).unwrap_or_default().join(" and ");
        return Some(format!("⚠ {used} and {others} differ · Nika uses {used}"));
    }
    Some(format!("key: {used}"))
}

/// The verdict of one key check in a word or two (`valid`, `refused (401)`, `unreachable`,
/// `unknown`) and the one plain line said beside it, both in `voice`: never the key, and never
/// the provider's own words or an error's command hints (live gauntlet 6, r02 B2: a refused
/// key's line carried the provider error's `--model` / `--access <seat>` hint). A check that
/// cannot judge the key keeps why, as its checker said it.
pub(in crate::session) fn verdict(
    provider: &str,
    checked: &Result<KeyCheck, String>,
    voice: Voice,
) -> (String, String) {
    let name = super::names::provider(provider);
    let said = |words: &str, (status, why): (&str, &str)| {
        let values = [
            ("{name}", name.as_str()),
            ("{status}", status),
            ("{why}", why),
        ];
        fill(voice.say(words), &values)
    };
    match checked {
        Ok(KeyCheck::Valid) => (
            voice.say("valid").to_owned(),
            said(
                "key accepted by {name} · it listed its models, no model ran",
                ("", ""),
            ),
        ),
        Ok(KeyCheck::Refused { status, .. }) => {
            let status = status.to_string();
            (
                said("refused ({status})", (&status, "")),
                said(REFUSED, (&status, "")),
            )
        }
        Ok(KeyCheck::Unreachable { why }) => (
            voice.say("unreachable").to_owned(),
            said("{name} could not be reached · {why}", ("", why)),
        ),
        Ok(KeyCheck::Unknown { why }) => (
            voice.say("unknown").to_owned(),
            said("key not judged by {name} · {why}", ("", why)),
        ),
        Ok(_) => (
            voice.say("unknown").to_owned(),
            said("key not judged by {name}", ("", "")),
        ),
        Err(why) => (
            voice.say("unknown").to_owned(),
            said("the key check of {name} could not run · {why}", ("", why)),
        ),
    }
}

/// The line of a key the provider refused.
const REFUSED: &str =
    "key refused by {name} ({status}) · check the key or choose another connection";

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_key_reads_by_the_variable_that_supplies_it_and_a_conflict_warns() {
        let both = vars(&["NIKA_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"]);
        assert_eq!(
            named(&both, true).as_deref(),
            Some(
                "⚠ NIKA_ANTHROPIC_API_KEY and ANTHROPIC_API_KEY differ · Nika uses NIKA_ANTHROPIC_API_KEY"
            )
        );
        assert_eq!(
            named(&both, false).as_deref(),
            Some("key: NIKA_ANTHROPIC_API_KEY"),
            "agreeing values: the first supplies it"
        );
        assert_eq!(
            named(&vars(&["DEEPSEEK_API_KEY"]), true).as_deref(),
            Some("key: DEEPSEEK_API_KEY"),
            "one variable cannot differ from itself"
        );
        assert_eq!(named(&[], false), None, "no variable set");
        assert_eq!(
            words(&[], "anthropic", crate::lang::Voice::default()),
            None,
            "no probe row"
        );
    }

    #[test]
    fn a_verdict_reads_in_one_word_and_never_carries_a_key() {
        let cases = [
            (Ok(KeyCheck::Valid), "valid"),
            (
                Ok(KeyCheck::Refused {
                    status: 401,
                    said: "invalid x-api-key".to_owned(),
                }),
                "refused (401)",
            ),
            (
                Ok(KeyCheck::Unreachable {
                    why: "connection refused".to_owned(),
                }),
                "unreachable",
            ),
            (
                Ok(KeyCheck::Unknown {
                    why: "HTTP 500".to_owned(),
                }),
                "unknown",
            ),
            (Err("no client".to_owned()), "unknown"),
        ];
        for (checked, word) in cases {
            let (said, sentence) = verdict("anthropic", &checked, Voice::default());
            assert_eq!(said, word);
            assert!(sentence.contains("Anthropic"), "{sentence}");
        }
    }

    /// A refused key reads as one plain line in the person's language, its status kept (live
    /// gauntlet 6, r02 B2): never the provider's words, an error code or a command's flags.
    #[test]
    fn a_refused_key_reads_as_one_plain_line_in_the_persons_language() {
        let refused = Ok(KeyCheck::Refused {
            status: 401,
            said: "authentication failed (HTTP 401); code=invalid_api_key; usage and billing \
                   unknown — The run could not reach that model. `--model` … `--access <seat>`"
                .to_owned(),
        });
        let french = Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        let cases = [
            (
                Voice::default(),
                "refused (401)",
                "key refused by OpenAI (401) · check the key or choose another connection",
            ),
            (
                french,
                "refusée (401)",
                "clé refusée par OpenAI (401) · vérifiez la clé ou choisissez une autre connexion",
            ),
        ];
        for (voice, word, line) in cases {
            let (said, sentence) = verdict("openai", &refused, voice);
            assert_eq!((said.as_str(), sentence.as_str()), (word, line));
        }
    }
}
