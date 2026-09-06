// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's intelligence — WHICH reasoning path the human chose
//! (an AI app they already have · an API · a local engine · none) and
//! whether THIS machine can serve it now. The choice is the human's,
//! persisted beside the other user files (`~/.nika/`), shared by every
//! install channel; the census is deterministic (presence, never a dial);
//! an explicit choice this machine cannot serve is REFUSED with its fix,
//! never silently replaced. Each path names where the project context
//! goes before a human reasons over it (the data locus).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The kind of intelligence the human chose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntelligenceKind {
    /// An AI app the human already has (a harness seat · `codex` · …).
    Harness {
        /// The seat id (`codex` · `claude-code` · …).
        seat: String,
    },
    /// A metered API (`openai` · `mistral` · …).
    Api {
        /// The provider id.
        provider: String,
    },
    /// A local engine on this machine (`ollama` · `lmstudio` · …).
    Local {
        /// The provider id.
        provider: String,
    },
    /// No conversational intelligence: the deterministic facts alone.
    None,
}

/// The persisted choice (`~/.nika/session-intelligence.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UserIntelligencePreference {
    /// The path the human chose.
    pub kind: IntelligenceKind,
    /// The model the human named, when any (`<provider>/<name>`).
    pub model: Option<String>,
    /// When the choice was made (RFC 3339, the wall clock).
    pub chosen_at: String,
}

impl UserIntelligencePreference {
    /// A fresh choice, stamped now.
    #[must_use]
    pub fn new(kind: IntelligenceKind, model: Option<String>) -> Self {
        Self {
            kind,
            model,
            chosen_at: now_rfc3339(),
        }
    }

    /// The preference file under a home directory.
    #[must_use]
    pub fn path_under(home: &Path) -> PathBuf {
        home.join(".nika").join("session-intelligence.json")
    }

    /// Load the choice under `home` — `None` when never chosen (the
    /// first run) or unreadable (a corrupt file is « never chosen », and
    /// the wizard runs again rather than guessing).
    #[must_use]
    pub fn load(home: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path_under(home)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Persist the choice under `home` (creates `~/.nika/`).
    ///
    /// # Errors
    ///
    /// The filesystem's refusal.
    pub fn save(&self, home: &Path) -> std::io::Result<()> {
        let path = Self::path_under(home);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // A civil date from the epoch (proleptic Gregorian · UTC) — enough for
    // a stamp a human reads; the engine's clocks live elsewhere.
    let days = secs / 86_400;
    let (hour, minute, second) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days: u64) -> (u64, u64, u64) {
    // Howard Hinnant's algorithm, unsigned form.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// One seat this machine holds, as the census saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SeatSeen {
    /// The seat id.
    pub id: String,
    /// The product binary is on PATH (the one an infer-grade seat spawns).
    pub product_present: bool,
    /// The seat's sign-in evidence.
    pub configured: bool,
}

/// The deterministic census — presence only, never a dial, never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IntelligenceCensus {
    /// The harness seats this machine holds.
    pub seats: Vec<SeatSeen>,
    /// The API providers whose key is PRESENT in the environment (names
    /// only · the value is never read here).
    pub api_keys: Vec<String>,
    /// The local engines configured on this machine.
    pub locals: Vec<String>,
}

impl IntelligenceCensus {
    /// A census that saw nothing (a machine with no seat, no key, no local engine).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            seats: Vec::new(),
            api_keys: Vec::new(),
            locals: Vec::new(),
        }
    }

    /// Take the census from the ONE probe every door shares.
    #[must_use]
    pub fn take() -> Self {
        let probe = nika_cli_host::probe::collect(false);
        Self::from_probe(&probe)
    }

    /// The census from an already-collected probe.
    #[must_use]
    pub fn from_probe(probe: &nika_cli_host::probe::Probe) -> Self {
        let seats = probe
            .census
            .seats
            .iter()
            .map(|s| SeatSeen {
                id: s.id.clone(),
                product_present: s.product_present,
                // The census's own sign-in judgment (the seat's auth probe ·
                // the same fact the runtime's plan reads) — never the HTTP
                // provider rows, which carry no seat.
                configured: s.signed_in,
            })
            .collect();
        let api_keys = probe
            .providers
            .iter()
            .filter(|p| {
                p.requires_key
                    && p.key_present
                    && p.readiness.access == nika_types::access::AccessClass::Api
            })
            .map(|p| p.id.clone())
            .collect();
        let locals = probe
            .providers
            .iter()
            .filter(|p| {
                p.readiness.access == nika_types::access::AccessClass::Local
                    && p.readiness.configured
            })
            .map(|p| p.id.clone())
            .collect();
        Self {
            seats,
            api_keys,
            locals,
        }
    }

    /// The first screen — human words, the atelier order (an app you
    /// already have · an API · local · none), never a class name.
    #[must_use]
    pub fn first_screen(&self) -> String {
        let seats: Vec<&str> = self
            .seats
            .iter()
            .filter(|s| s.product_present)
            .map(|s| s.id.as_str())
            .collect();
        let apps = if seats.is_empty() {
            "none found on this machine".to_owned()
        } else {
            seats.join(" · ")
        };
        let keys = if self.api_keys.is_empty() {
            "no key in the environment".to_owned()
        } else {
            self.api_keys.join(" · ")
        };
        let locals = if self.locals.is_empty() {
            "none reachable".to_owned()
        } else {
            self.locals.join(" · ")
        };
        format!(
            "Nika\nChoose which AI answers your questions here. You can change this later (`/intelligence`); the choice is kept at ~/.nika/session-intelligence.json.\n\n  1  Use an AI app I already have (a coding assistant you are signed into)\n     {apps}\n  2  Use an API (metered · your own key)\n     {keys}\n  3  Run locally (private · on this machine)\n     {locals}\n  4  No AI in this conversation\n     Nika still answers from its own catalog: your workflows · checks · examples · builtins\n"
        )
    }

    /// Turn a first-screen answer (`1`..`4`, optionally with a name:
    /// `1 codex` · `2 mistral` · `3 ollama`) into a choice — refused with
    /// its fix when this machine cannot serve it.
    ///
    /// # Errors
    ///
    /// The refusal text, with the fix, when the pick is unknown or not
    /// served on this machine.
    pub fn choose(&self, answer: &str) -> Result<UserIntelligencePreference, String> {
        let mut words = answer.split_whitespace();
        let pick = words.next().unwrap_or("");
        let name = words.next().map(str::to_owned);
        match pick {
            "1" => {
                let seat = match name {
                    Some(n) => n,
                    None => self
                        .seats
                        .iter()
                        .find(|s| s.product_present)
                        .map(|s| s.id.clone())
                        .ok_or_else(|| {
                            "no AI app found on this machine — install one (Codex · Claude Code · Gemini CLI · Kimi Code · Qwen Code) or pick 2, 3 or 4".to_owned()
                        })?,
                };
                Ok(UserIntelligencePreference::new(
                    IntelligenceKind::Harness { seat },
                    None,
                ))
            }
            "2" => {
                let provider = match name {
                    Some(n) => n,
                    None => self.api_keys.first().cloned().ok_or_else(|| {
                        "no API key in the environment — `export <PROVIDER>_API_KEY=…` (`nika doctor` names the variable) or pick 1, 3 or 4".to_owned()
                    })?,
                };
                Ok(UserIntelligencePreference::new(
                    IntelligenceKind::Api { provider },
                    None,
                ))
            }
            "3" => {
                let provider = match name {
                    Some(n) => n,
                    None => self.locals.first().cloned().ok_or_else(|| {
                        "no local engine reachable — start one (ollama · lmstudio · llamacpp · localai · vllm) or pick 1, 2 or 4".to_owned()
                    })?,
                };
                Ok(UserIntelligencePreference::new(
                    IntelligenceKind::Local { provider },
                    None,
                ))
            }
            "4" => Ok(UserIntelligencePreference::new(
                IntelligenceKind::None,
                None,
            )),
            other => Err(format!("`{other}` is not a choice — answer 1, 2, 3 or 4")),
        }
    }
}

/// Where the project context goes when the human reasons over it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataLocus {
    /// Through an AI app's account (their servers).
    Remote {
        /// The product.
        product: String,
    },
    /// Through a metered API (the provider's servers).
    Metered {
        /// The provider.
        provider: String,
    },
    /// Stays on this machine.
    Local,
    /// Nothing leaves: no model reasons.
    None,
}

impl DataLocus {
    /// The plain-language consequence the human reads before the first turn.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Remote { product } => format!(
                "{product} · uses your existing account · project context you ask Nika to reason over may be sent through {product}"
            ),
            Self::Metered { provider } => format!(
                "{provider} API · metered · project context you ask Nika to reason over is sent to {provider}"
            ),
            Self::Local => "local · private · project context stays on this machine".to_owned(),
            Self::None => {
                "no conversational AI · nothing leaves this machine · the facts stay".to_owned()
            }
        }
    }
}

/// The intelligence the session runs with — the choice judged against
/// the census, with its locus and its refusal when this machine cannot
/// serve it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedSessionIntelligence {
    /// The human's choice.
    pub kind: IntelligenceKind,
    /// The model the reasoner names, when a path needs one.
    pub model: Option<String>,
    /// Where the context goes.
    pub locus: DataLocus,
    /// This machine can serve the choice now.
    pub ready: bool,
    /// Why not, with the fix, when it cannot.
    pub why: Option<String>,
}

impl ResolvedSessionIntelligence {
    /// Judge the choice against the census — an explicit choice this
    /// machine cannot serve is refused with its fix, never replaced.
    #[must_use]
    pub fn resolve(pref: &UserIntelligencePreference, census: &IntelligenceCensus) -> Self {
        let (locus, ready, why) = match &pref.kind {
            IntelligenceKind::Harness { seat } => {
                let seen = census.seats.iter().find(|s| &s.id == seat);
                match seen {
                    Some(s) if s.product_present && s.configured => (
                        DataLocus::Remote {
                            product: seat.clone(),
                        },
                        true,
                        None,
                    ),
                    Some(s) if s.product_present => (
                        DataLocus::Remote {
                            product: seat.clone(),
                        },
                        false,
                        Some(format!(
                            "`{seat}` is installed but not signed in — sign in to {seat} itself, or `/intelligence` to choose another path"
                        )),
                    ),
                    _ => (
                        DataLocus::Remote {
                            product: seat.clone(),
                        },
                        false,
                        Some(format!(
                            "`{seat}` is not installed on this machine — install it, or `/intelligence` to choose another path (`nika doctor` lists the seats)"
                        )),
                    ),
                }
            }
            IntelligenceKind::Api { provider } => {
                if census.api_keys.iter().any(|k| k == provider) {
                    (
                        DataLocus::Metered {
                            provider: provider.clone(),
                        },
                        true,
                        None,
                    )
                } else {
                    (
                        DataLocus::Metered {
                            provider: provider.clone(),
                        },
                        false,
                        Some(format!(
                            "no key for `{provider}` in the environment — `export <VAR>=…` (`nika doctor` names the variable), or `/intelligence` to choose another path"
                        )),
                    )
                }
            }
            IntelligenceKind::Local { provider } => {
                if census.locals.iter().any(|l| l == provider) {
                    (DataLocus::Local, true, None)
                } else {
                    (
                        DataLocus::Local,
                        false,
                        Some(format!(
                            "`{provider}` is not reachable on this machine — start it, or `/intelligence` to choose another path"
                        )),
                    )
                }
            }
            IntelligenceKind::None => (DataLocus::None, true, None),
        };
        Self {
            kind: pref.kind.clone(),
            model: pref.model.clone(),
            locus,
            ready,
            why,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn census() -> IntelligenceCensus {
        IntelligenceCensus {
            seats: vec![
                SeatSeen {
                    id: "codex".to_owned(),
                    product_present: true,
                    configured: true,
                },
                SeatSeen {
                    id: "claude-code".to_owned(),
                    product_present: false,
                    configured: false,
                },
            ],
            api_keys: vec!["mistral".to_owned()],
            locals: vec![],
        }
    }

    /// The choice round-trips through the user file under a home.
    #[test]
    fn the_preference_round_trips_under_home() {
        let home = tempfile::tempdir().expect("home");
        assert!(
            UserIntelligencePreference::load(home.path()).is_none(),
            "first run"
        );
        let pref = UserIntelligencePreference::new(
            IntelligenceKind::Harness {
                seat: "codex".to_owned(),
            },
            None,
        );
        pref.save(home.path()).expect("saved");
        let back = UserIntelligencePreference::load(home.path()).expect("loaded");
        assert_eq!(back.kind, pref.kind);
        assert!(
            back.chosen_at.ends_with('Z') && back.chosen_at.len() == 20,
            "{}",
            back.chosen_at
        );
        std::fs::write(
            UserIntelligencePreference::path_under(home.path()),
            "{ not json",
        )
        .expect("corrupt");
        assert!(
            UserIntelligencePreference::load(home.path()).is_none(),
            "a corrupt file is « never chosen »"
        );
    }

    /// An explicit choice this machine cannot serve is refused with its
    /// fix — never replaced by another path.
    #[test]
    fn an_unserved_choice_is_refused_never_replaced() {
        let c = census();
        let absent = UserIntelligencePreference::new(
            IntelligenceKind::Harness {
                seat: "claude-code".to_owned(),
            },
            None,
        );
        let r = ResolvedSessionIntelligence::resolve(&absent, &c);
        assert!(!r.ready);
        assert!(
            r.why
                .as_deref()
                .is_some_and(|w| w.contains("not installed")),
            "{r:?}"
        );
        assert_eq!(r.kind, absent.kind, "the choice stands");
        let no_key = UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "openai".to_owned(),
            },
            None,
        );
        let r = ResolvedSessionIntelligence::resolve(&no_key, &c);
        assert!(
            !r.ready
                && r.why
                    .as_deref()
                    .is_some_and(|w| w.contains("no key for `openai`")),
            "{r:?}"
        );
        let served = UserIntelligencePreference::new(
            IntelligenceKind::Harness {
                seat: "codex".to_owned(),
            },
            None,
        );
        let r = ResolvedSessionIntelligence::resolve(&served, &c);
        assert!(r.ready);
        assert!(
            r.locus.line().contains("may be sent through codex"),
            "{}",
            r.locus.line()
        );
    }

    /// The first screen speaks human words in the atelier order and never
    /// a class name; a pick resolves to the choice or its fix.
    #[test]
    fn the_first_screen_and_the_picks() {
        let c = census();
        let screen = c.first_screen();
        let one = screen.find("1  Use an AI app").expect("1");
        let two = screen.find("2  Use an API").expect("2");
        let three = screen.find("3  Run locally").expect("3");
        let four = screen.find("4  No AI in this conversation").expect("4");
        assert!(one < two && two < three && three < four);
        for banned in ["AccessClass", "ACP", "harness", "billing"] {
            assert!(!screen.contains(banned), "{banned} on the first screen");
        }
        assert!(
            screen.contains("codex") && !screen.contains("claude-code"),
            "only installed apps are offered"
        );
        assert_eq!(
            c.choose("1").expect("codex").kind,
            IntelligenceKind::Harness {
                seat: "codex".to_owned()
            }
        );
        assert_eq!(
            c.choose("2").expect("mistral").kind,
            IntelligenceKind::Api {
                provider: "mistral".to_owned()
            }
        );
        assert!(
            c.choose("3")
                .expect_err("no local")
                .contains("no local engine")
        );
        assert_eq!(c.choose("4").expect("none").kind, IntelligenceKind::None);
        assert!(c.choose("9").is_err());
    }
}
