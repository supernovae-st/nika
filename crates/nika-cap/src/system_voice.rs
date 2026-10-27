// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The program a builtin starts ITSELF as part of its own contract — never
//! the workflow's `exec:` (stdlib §`nika:tts_generate` · `provider: system`).
//!
//! One home for the names: the composition root probes them, `nika check`
//! names them, so what the check says the run starts is what it starts. No
//! `permits.exec` is required or consulted for them; the run journals each
//! start as the builtin's own effect (`permit_checked` · `plane: builtin`).

/// macOS's synthesizer — an absolute, system-protected path.
pub const SYSTEM_VOICE_SAY: &str = "/usr/bin/say";
/// Linux's synthesizer — the first `espeak-ng` on the engine's `PATH`.
pub const SYSTEM_VOICE_ESPEAK: &str = "espeak-ng";

/// The synthesizer a `system` call on THIS build's host starts, as the check
/// names it: `/usr/bin/say` on macOS, `espeak-ng` on Linux, `None` on any
/// other OS (where the call refuses `NIKA-BUILTIN-TTS_GENERATE-002`).
#[must_use]
pub const fn system_voice_program() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some(SYSTEM_VOICE_SAY)
    } else if cfg!(target_os = "linux") {
        Some(SYSTEM_VOICE_ESPEAK)
    } else {
        None
    }
}

/// Does this call start the host synthesizer? A LITERAL `provider: system`
/// on `nika:tts_generate` — a templated provider is the run's question.
#[must_use]
pub fn starts_system_voice(tool: &str, args: Option<&serde_json::Value>) -> bool {
    tool == "nika:tts_generate"
        && args
            .and_then(|a| a.get("provider"))
            .and_then(serde_json::Value::as_str)
            == Some("system")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_literal_system_provider_starts_the_host_synthesizer() {
        let system = json!({"provider": "system", "text": "hi"});
        assert!(starts_system_voice("nika:tts_generate", Some(&system)));
        for args in [
            json!({"provider": "mock"}),
            json!({"provider": "${{ p }}"}),
            json!({}),
        ] {
            assert!(
                !starts_system_voice("nika:tts_generate", Some(&args)),
                "{args}"
            );
        }
        assert!(!starts_system_voice("nika:image_generate", Some(&system)));
        assert!(!starts_system_voice("nika:tts_generate", None));
    }

    #[test]
    fn the_named_program_is_this_hosts() {
        let named = system_voice_program();
        if cfg!(target_os = "macos") {
            assert_eq!(named, Some("/usr/bin/say"));
        } else if cfg!(target_os = "linux") {
            assert_eq!(named, Some("espeak-ng"));
        } else {
            assert_eq!(named, None);
        }
    }
}
