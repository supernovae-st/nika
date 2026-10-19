// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The name a person reads for a model id, derived deterministically from the
//! id alone: `anthropic/claude-sonnet-5-5` reads « `Claude Sonnet 5.5` »,
//! `openai/gpt-6.1-sol` « `GPT-6.1 Sol` », `deepseek/deepseek-flash`
//! « `DeepSeek Flash` ». Only families this table knows are renamed; any other id is shown
//! as it is (its last path segment), so no name is ever invented. A display
//! name selects nothing: the offer's key stays the exact id.

/// The families whose ids read as a name, by their first id word.
const FAMILIES: &[(&str, &str)] = &[
    ("claude", "Claude"),
    ("gpt", "GPT"),
    ("deepseek", "DeepSeek"),
    ("gemini", "Gemini"),
    ("gemma", "Gemma"),
    ("grok", "Grok"),
    ("mistral", "Mistral"),
    ("codestral", "Codestral"),
    ("llama", "Llama"),
    ("qwen", "Qwen"),
    ("glm", "GLM"),
    ("kimi", "Kimi"),
    ("mimo", "MiMo"),
    ("jamba", "Jamba"),
    ("command", "Command"),
    ("phi", "Phi"),
    ("jev", "Jev"),
];

/// The providers and seats whose ids read as a name.
const PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "Anthropic"),
    ("openai", "OpenAI"),
    ("deepseek", "DeepSeek"),
    ("mistral", "Mistral"),
    ("gemini", "Gemini"),
    ("xai", "xAI"),
    ("groq", "Groq"),
    ("openrouter", "OpenRouter"),
    ("scaleway", "Scaleway"),
    ("huggingface", "Hugging Face"),
    ("together", "Together"),
    ("fireworks", "Fireworks"),
    ("cerebras", "Cerebras"),
    ("sambanova", "SambaNova"),
    ("cohere", "Cohere"),
    ("ai21", "AI21"),
    ("bedrock", "Bedrock"),
    ("azure", "Azure"),
    ("ollama", "Ollama"),
    ("lmstudio", "LM Studio"),
    ("claude-code", "Claude Code"),
    ("codex", "Codex"),
];

/// The display name of a provider or seat id; any other id as it is.
#[must_use]
pub fn provider(id: &str) -> String {
    (PROVIDERS.iter())
        .find(|(known, _)| known.eq_ignore_ascii_case(id))
        .map_or_else(|| id.to_owned(), |(_, name)| (*name).to_owned())
}

/// The display name of `model` (`provider/name`, or a bare name).
#[must_use]
pub fn display(model: &str) -> String {
    let name = model.rsplit('/').next().unwrap_or(model);
    let mut words = name.split('-').filter(|word| !word.is_empty()).peekable();
    let Some(first) = words.next() else {
        return name.to_owned();
    };
    let Some((_, family)) = FAMILIES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(first))
    else {
        return name.to_owned();
    };
    let mut parts: Vec<String> = vec![(*family).to_owned()];
    // GPT joins its version or its `oss` with a hyphen: GPT-6.1, GPT-4o, GPT-OSS.
    if first.eq_ignore_ascii_case("gpt")
        && let Some(next) = words.next_if(|word| version(word) || word.eq_ignore_ascii_case("oss"))
    {
        let joined = if next.eq_ignore_ascii_case("oss") {
            "OSS".to_owned()
        } else {
            next.to_owned()
        };
        parts[0] = format!("GPT-{joined}");
    }
    while let Some(word) = words.next() {
        if short_number(word) {
            // `5-5` reads 5.5: consecutive short numbers form one version.
            let mut joined = word.to_owned();
            while let Some(more) = words.next_if(|next| short_number(next)) {
                joined.push('.');
                joined.push_str(more);
            }
            parts.push(joined);
        } else if word.len() == 8 && word.bytes().all(|b| b.is_ascii_digit()) {
            parts.push(format!("({word})"));
        } else {
            parts.push(word_name(word));
        }
    }
    parts.join(" ")
}

/// What sorts a model among its family, read from its id the way [`display`] reads it: the
/// family's name, the major version when the id states one, and whether the id is a dated
/// snapshot (`-20251001`). Unknown families and versionless ids state nothing to compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    /// The family's display name (`Claude`, `GPT`, `DeepSeek`).
    pub family: String,
    /// The major version, when the id states one (`5` for `claude-sonnet-5-5`).
    pub major: Option<u32>,
    /// The minor version beside that major, read as a decimal in thousandths (`500` for
    /// `claude-sonnet-5-5`, `700` for `grok-4.7`, `200` for `grok-4.20`, `0` for `grok-4`), so
    /// 4.7 sorts after 4.20 as a person reads them.
    pub minor: u32,
    /// The id carries a snapshot date.
    pub dated: bool,
}

/// The generation of `model`, when its family is known.
pub fn generation(model: &str) -> Option<Generation> {
    let shown = display(model);
    let mut words = shown.split(' ');
    let first = words.next()?;
    let (mut family, inline) = first
        .split_once('-')
        .map_or((first, None), |(f, v)| (f, Some(v)));
    if !FAMILIES.iter().any(|(_, name)| *name == family) {
        return None;
    }
    if inline == Some("OSS") {
        family = first;
    }
    let rest: Vec<&str> = words.collect();
    let stated = (inline.filter(|word| version_major(word).is_some())).or_else(|| {
        rest.iter()
            .copied()
            .find(|word| version_major(word).is_some())
    });
    let major = stated.and_then(version_major);
    let minor = stated.map_or(0, version_minor);
    let dated = rest
        .iter()
        .any(|word| (word.len() == 10 && word.starts_with('(')) || snapshot_date(word));
    Some(Generation {
        family: family.to_owned(),
        major,
        minor,
        dated,
    })
}

/// The minor of a version word as a decimal in thousandths (`700` for `4.7`, `200` for
/// `4.20`, `50` for `1.05`), `0` when it states none: a minor is read as the digits after the
/// point, never as a count, so `4.7` is newer than `4.20`.
fn version_minor(word: &str) -> u32 {
    let (_, after) = word.split_once('.').unwrap_or((word, ""));
    let digits: String = (after.chars().take_while(char::is_ascii_digit))
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect();
    digits.parse().unwrap_or(0)
}

/// A snapshot date written in digits alone after a model's name: `0528` (month and day),
/// `250731` (year, month and day). A date is never a version (live gauntlet r07 B5: « R1 0528 »
/// read as version 528 and was marked the newest `DeepSeek`).
fn snapshot_date(word: &str) -> bool {
    matches!(word.len(), 4 | 6) && word.bytes().all(|b| b.is_ascii_digit())
}

/// The major of a version word (`5.5`, `4o`, `V4`, `2.6`); `None` for any other word (a size
/// such as `120B`, a name, a date such as `0528`: three digits or more are never a major).
fn version_major(word: &str) -> Option<u32> {
    let bare = word.strip_prefix(['V', 'v']).unwrap_or(word);
    let digits: String = bare.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() > 2 {
        return None;
    }
    let tail = &bare[digits.len()..];
    let tail = tail.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let versioned =
        tail.is_empty() || (tail.len() == 1 && tail.chars().all(|c| c.is_ascii_lowercase()));
    digits.parse::<u32>().ok().filter(|_| versioned)
}

/// One or two digits: a part of a version (`5`, `12`).
fn short_number(word: &str) -> bool {
    (1..=2).contains(&word.len()) && word.bytes().all(|b| b.is_ascii_digit())
}

/// A version word: digits with dots, or digits then a letter (`6.1`, `4o`).
fn version(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_digit())
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// A word as it reads: a size (`120b` → 120B), a version tag (`v4` → V4), a
/// number kept as written, any other word capitalised.
fn word_name(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    if let Some(size) = lower.strip_suffix('b')
        && !size.is_empty()
        && size.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return format!("{size}B");
    }
    if let Some(tag) = lower.strip_prefix('v')
        && tag.starts_with(|c: char| c.is_ascii_digit())
    {
        return format!("V{tag}");
    }
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{Generation, display, generation};

    #[test]
    fn a_generation_reads_the_family_the_major_and_a_snapshot_date() {
        let read = |id: &str| generation(id).map(|g| (g.family, g.major, g.dated));
        let of =
            |family: &str, major: Option<u32>, dated: bool| Some((family.to_owned(), major, dated));
        assert_eq!(
            read("anthropic/claude-sonnet-5-5"),
            of("Claude", Some(5), false)
        );
        assert_eq!(
            read("anthropic/claude-3-5-sonnet"),
            of("Claude", Some(3), false)
        );
        assert_eq!(
            read("anthropic/claude-haiku-4-5-20251001"),
            of("Claude", Some(4), true)
        );
        assert_eq!(read("openai/gpt-6.1-sol"), of("GPT", Some(6), false));
        assert_eq!(read("openai/gpt-4o-mini"), of("GPT", Some(4), false));
        assert_eq!(
            read("openai/gpt-oss-120b"),
            of("GPT-OSS", None, false),
            "a size is no version"
        );
        assert_eq!(
            read("groq/llama-3.3-70b-versatile"),
            of("Llama", Some(3), false)
        );
        assert_eq!(
            read("deepseek/deepseek-v4-pro"),
            of("DeepSeek", Some(4), false)
        );
        assert_eq!(read("deepseek/deepseek-flash"), of("DeepSeek", None, false));
        assert_eq!(
            read("huggingface/deepseek-ai/deepseek-r1-0528"),
            of("DeepSeek", None, true),
            "a date is no version"
        );
        assert_eq!(
            read("scaleway/deepseek-v4-flash-0731"),
            of("DeepSeek", Some(4), true)
        );
        assert_eq!(
            read("xai/grok-4.20-0309-non-reasoning"),
            of("Grok", Some(4), true)
        );
        let minor = |id: &str| generation(id).map(|g| g.minor);
        assert!(
            minor("xai/grok-4.7") > minor("xai/grok-4.20"),
            "4.7 is newer than 4.20"
        );
        assert_eq!(minor("anthropic/claude-sonnet-5-5"), Some(500));
        assert_eq!(minor("xai/grok-4"), Some(0));
        assert_eq!(
            read("openrouter/stepfun/step-5-preview"),
            None,
            "an unknown family"
        );
        let _: Option<Generation> = generation("");
    }

    #[test]
    fn known_families_read_as_names_and_any_other_id_stays_as_it_is() {
        for (id, name) in [
            ("anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ("anthropic/claude-opus-5-5", "Claude Opus 5.5"),
            (
                "anthropic/claude-haiku-4-5-20251001",
                "Claude Haiku 4.5 (20251001)",
            ),
            ("anthropic/claude-3-5-sonnet", "Claude 3.5 Sonnet"),
            ("openai/gpt-6.1-sol", "GPT-6.1 Sol"),
            ("openai/gpt-4o-mini", "GPT-4o Mini"),
            ("openai/gpt-5-mini", "GPT-5 Mini"),
            ("openai/gpt-oss-120b", "GPT-OSS 120B"),
            ("deepseek/deepseek-flash", "DeepSeek Flash"),
            ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro"),
            ("scaleway/deepseek-v4-flash-0731", "DeepSeek V4 Flash 0731"),
            ("gemini/gemini-2.5-flash", "Gemini 2.5 Flash"),
            ("xai/grok-4.7", "Grok 4.7"),
            ("mistral/mistral-large-latest", "Mistral Large Latest"),
            ("groq/llama-3.3-70b-versatile", "Llama 3.3 70B Versatile"),
            ("openrouter/z-ai/glm-5.3-flash", "GLM 5.3 Flash"),
            ("openrouter/xiaomi/mimo-v2.6-flash", "MiMo V2.6 Flash"),
            ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ("openrouter/stepfun/step-5-preview", "step-5-preview"),
            ("acme/imaginary-1", "imaginary-1"),
            ("typesafe/jev-1.13.0", "Jev 1.13.0"),
            ("", ""),
        ] {
            assert_eq!(display(id), name, "{id}");
        }
    }
}
