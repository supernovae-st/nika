// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ACCESS CENSUS — the ONE enumeration of every inference path this
//! machine offers (R4 root-cause: welcome, doctor and the refusal chain
//! each read a DIFFERENT source of access truth, so a signed-in harness
//! seat could run inference while the first screen said « installed · no
//! inference path »). One collector, one type; every surface READS it,
//! none recomputes its own detection.
//!
//! Sovereignty invariants (the [`crate::probe`] precedent): key checks
//! are PRESENT-NOT-PRINTED, seat detection is PATH + auth-surface
//! presence only (never a credential read, never a spawn, never the
//! network) — a census is cheap enough for the greeting.

use nika_types::access::{AccessClass, HarnessRuntime};

use crate::probe::ProviderProbe;
use crate::{ProviderRegistry, ProvidersConfig};

/// One way this machine can serve inference — the census row every
/// surface reads (doctor's machine lane · the adoption ladder · the
/// refusal tail).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessPath {
    /// The path's class (`api` · `local` · `harness` · …).
    pub class: AccessClass,
    /// The path id — a provider id (`anthropic`) or, for the harness
    /// class, the seat's PIN token (`claude-code` — never the retired
    /// ACP wrapper id).
    pub id: String,
    /// The path can serve as configured TODAY (key present · seat
    /// signed in with its adapter). Keyless local seeds are always
    /// « configured » — presence is not detection, so they never win
    /// [`AccessCensus::best`].
    pub configured: bool,
    /// WHERE the credential lives — the env var NAME for a keyed
    /// `api`/`oauth` row (`ANTHROPIC_API_KEY`); `None` for keyless and
    /// seat rows (a seat's custody is the harness's own login, never an
    /// env var nika reads). Custody is DERIVED from the row, never a
    /// value probe.
    pub custody: Option<String>,
    /// The printed fix when the path is not usable (never run). A seat
    /// row names its `--access` pin and the gesture (install · sign
    /// in), never the ACP wrapper id — the wrapper is not a pin
    /// (NIKA-1802).
    pub fix_line: Option<String>,
}

/// One harness seat's facts (feature `access-harness`) — the triple the
/// first screen needs: the APP is here · the ACP ADAPTER a session
/// spawns is here · the sign-in witness the admission gate reads. A
/// signed-in app without its adapter is « detected, not ready » — the
/// two halves are never conflated (the gauntlet's P1/P4 lesson).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SeatFact {
    /// The pin token (`claude-code` · `codex` · …).
    pub id: String,
    /// The provider ids this seat fronts.
    pub serves: Vec<String>,
    /// The product CLI is on PATH (`claude`).
    pub product_present: bool,
    /// The ACP speaker is on PATH (`claude-agent-acp` — the binary a
    /// session spawns; for native-ACP seats this IS the product bin).
    pub adapter_present: bool,
    /// The admission auth witness (the registry's auth surface — a
    /// home-file presence, or ACP-on-PATH for the command-auth class
    /// whose session is the sign-in witness).
    pub signed_in: bool,
}

impl SeatFact {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        serves: Vec<String>,
        product_present: bool,
        adapter_present: bool,
        signed_in: bool,
    ) -> Self {
        Self {
            id: id.into(),
            serves,
            product_present,
            adapter_present,
            signed_in,
        }
    }

    /// A run can START on this seat: signed in AND the adapter a
    /// session spawns is on PATH (the first screen's « ready »).
    /// `product_present` is deliberately NOT a leg: the session spawns
    /// the ADAPTER, never the product bin — for a native-ACP seat the
    /// two are one binary (`adapter_present` covers it), and for the
    /// wrapper class the adapter plus the sign-in witness (the home
    /// file the product once wrote) is enough to start. The product's
    /// absence only changes the FIX line (`as_path`), never readiness.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.signed_in && self.adapter_present
    }

    /// The seat as a census path — `configured` mirrors the resolver's
    /// candidate (`signed_in`: the sign-in witness), the fix line names
    /// the PIN and the gesture, never the ACP wrapper id.
    #[must_use]
    pub fn as_path(&self) -> AccessPath {
        let rt = HarnessRuntime::lookup(&self.id);
        let display = rt.map_or(self.id.as_str(), |r| r.display);
        let fix_line = if self.ready() {
            None
        } else if !self.product_present && !self.adapter_present {
            Some(format!(
                "install {display} itself · then `--access {}`",
                self.id
            ))
        } else if !self.adapter_present {
            Some(format!(
                "install its ACP speaker (`nika doctor` has the line) · then `--access {}`",
                self.id
            ))
        } else {
            Some(format!(
                "sign in to {display} itself · then `--access {}`",
                self.id
            ))
        };
        AccessPath {
            class: AccessClass::Harness,
            id: self.id.clone(),
            configured: self.signed_in,
            custody: None,
            fix_line,
        }
    }
}

/// The whole machine, one read: every access path (provider rows + the
/// harness seats joined in — the join the probe rows alone never did),
/// the seats a run can start on, and the strongest configured path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessCensus {
    /// Every path, provider rows then seats (census order is stable).
    pub paths: Vec<AccessPath>,
    /// The harness seats (empty when built without `access-harness`).
    pub seats: Vec<SeatFact>,
    /// Seat pin tokens a run can start on, in the ratified G-3 order.
    pub seats_ready: Vec<String>,
    /// The strongest CONFIGURED non-seed path: a ready seat outranks a
    /// metered key (the sovereign order — the operator's own plan
    /// first); a keyless local seed is a CATALOG fact, never a win.
    pub best: Option<AccessPath>,
}

impl AccessCensus {
    /// The pure fold — provider probe rows + seat facts in, census out.
    /// Zero I/O: tests drive this half.
    #[must_use]
    pub fn from_parts(probes: &[ProviderProbe], seats: Vec<SeatFact>) -> Self {
        let mut paths: Vec<AccessPath> = probes.iter().map(path_from_probe).collect();
        paths.extend(seats.iter().map(SeatFact::as_path));
        let seats_ready = seats_ready(&seats);
        let best = seats_ready
            .first()
            .and_then(|id| {
                paths
                    .iter()
                    .find(|p| p.class == AccessClass::Harness && &p.id == id)
            })
            .or_else(|| {
                paths.iter().find(|p| {
                    p.configured && matches!(p.class, AccessClass::Api | AccessClass::Oauth)
                })
            })
            .cloned();
        Self {
            paths,
            seats,
            seats_ready,
            best,
        }
    }

    /// The collector (the I/O lives here, [`crate::probe`]'s precedent):
    /// the SAME env composition a run uses, presence-only.
    #[must_use]
    pub fn collect(config: ProvidersConfig) -> Self {
        let registry = ProviderRegistry::without_http(config);
        let probes = crate::probe::collect_provider_probes(&registry);
        Self::from_parts(&probes, collect_seats())
    }

    /// The seat-escape line an auth-class refusal teaches when THIS
    /// machine has a signed-in seat (R4) — census-derived, so it is
    /// printed only when it is TRUE, and it names the LIVE pin token.
    #[must_use]
    pub fn seat_escape(&self) -> Option<String> {
        self.seats_ready
            .first()
            .map(|seat| format!("or use a signed-in seat: `--access {seat}`"))
    }
}

/// The seats half of the collector — PATH + auth-surface presence only
/// (the admission probe's own cheap pass, never a handshake spawn).
#[must_use]
pub fn collect_seats() -> Vec<SeatFact> {
    #[cfg(feature = "access-harness")]
    {
        match nika_harness::registry() {
            Ok(rows) => nika_harness::presence_facts(rows)
                .into_iter()
                .map(|fact| {
                    SeatFact::new(
                        fact.id,
                        fact.serves,
                        fact.product_present,
                        fact.acp_present,
                        fact.configured,
                    )
                })
                .collect(),
            // A broken registry offers nothing (fail-closed — the
            // `harness_provider_rows` precedent).
            Err(_) => Vec::new(),
        }
    }
    #[cfg(not(feature = "access-harness"))]
    {
        Vec::new()
    }
}

/// A provider probe row as a census path. Custody is the conventional
/// env var NAME the row already carries (the fix surface's `fix_var`) —
/// derived, never a value read.
fn path_from_probe(p: &ProviderProbe) -> AccessPath {
    AccessPath {
        class: p.readiness.access,
        id: p.id.clone(),
        configured: p.readiness.configured,
        custody: p.requires_key.then(|| p.fix_var.clone()),
        fix_line: (!p.readiness.configured && p.requires_key)
            .then(|| format!("export {}=…", p.fix_var)),
    }
}

/// The seats a run can start on, ordered by the ratified runtime order
/// (G-3) so two reads never disagree; an id outside the vocabulary
/// ranks last, codepoint-stable.
fn seats_ready(seats: &[SeatFact]) -> Vec<String> {
    let mut ready: Vec<&SeatFact> = seats.iter().filter(|s| s.ready()).collect();
    ready.sort_by_key(|s| {
        HarnessRuntime::ALL
            .iter()
            .position(|rt| rt.id == s.id)
            .unwrap_or(usize::MAX)
    });
    ready.into_iter().map(|s| s.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{ExecutionLocus, ProviderReadiness};

    fn api_row(id: &str, key_present: bool, fix_var: &str) -> ProviderProbe {
        ProviderProbe::new(
            id,
            true,
            key_present,
            fix_var,
            true,
            ProviderReadiness::new(
                true,
                key_present,
                None,
                None,
                true,
                ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.example.test",
        )
    }

    fn local_row(id: &str) -> ProviderProbe {
        ProviderProbe::new(
            id,
            false,
            false,
            "",
            false,
            ProviderReadiness::new(
                true,
                true,
                None,
                None,
                false,
                ExecutionLocus::Loopback,
                AccessClass::Local,
            ),
            "http://127.0.0.1:11434",
        )
    }

    fn seat(id: &str, product: bool, adapter: bool, signed_in: bool) -> SeatFact {
        SeatFact::new(
            id,
            vec!["anthropic".to_owned()],
            product,
            adapter,
            signed_in,
        )
    }

    #[test]
    fn a_signed_in_seat_with_its_adapter_is_a_ready_path() {
        let census = AccessCensus::from_parts(
            &[local_row("ollama")],
            vec![seat("claude-code", true, true, true)],
        );
        assert_eq!(census.seats_ready, ["claude-code"]);
        let best = census.best.as_ref().expect("a ready seat wins best");
        assert_eq!(best.id, "claude-code");
        assert_eq!(best.class, AccessClass::Harness);
        assert_eq!(
            census.seat_escape().as_deref(),
            Some("or use a signed-in seat: `--access claude-code`")
        );
    }

    /// Mutation pin for [`SeatFact::ready`]: inverting either leg
    /// (`signed_in` · `adapter_present`) must flip the census.
    #[test]
    fn a_seat_missing_either_half_is_never_ready() {
        for fact in [
            seat("claude-code", true, false, true), // signed in, no adapter
            seat("claude-code", true, true, false), // adapter, no sign-in
            seat("claude-code", false, false, false),
        ] {
            let census = AccessCensus::from_parts(&[], vec![fact]);
            assert!(census.seats_ready.is_empty(), "{:?}", census.seats);
            assert_eq!(census.seat_escape(), None, "no truth, no tail");
        }
    }

    #[test]
    fn custody_names_the_env_var_never_a_value() {
        let census = AccessCensus::from_parts(
            &[
                api_row("anthropic", false, "ANTHROPIC_API_KEY"),
                local_row("ollama"),
            ],
            vec![],
        );
        let anthropic = census
            .paths
            .iter()
            .find(|p| p.id == "anthropic")
            .expect("the row");
        assert!(!anthropic.configured);
        assert_eq!(anthropic.custody.as_deref(), Some("ANTHROPIC_API_KEY"));
        assert_eq!(
            anthropic.fix_line.as_deref(),
            Some("export ANTHROPIC_API_KEY=…")
        );
        // A keyless local seed carries no custody and never wins `best`.
        let ollama = census.paths.iter().find(|p| p.id == "ollama").expect("row");
        assert_eq!(ollama.custody, None);
        assert_eq!(ollama.fix_line, None);
        assert_eq!(census.best, None, "a keyless seed is not detection");
    }

    /// The sovereign order inside `best`: a ready seat outranks the
    /// metered key even when the key is configured.
    #[test]
    fn best_prefers_the_operators_own_plan_over_the_metered_key() {
        let census = AccessCensus::from_parts(
            &[api_row("anthropic", true, "ANTHROPIC_API_KEY")],
            vec![seat("claude-code", true, true, true)],
        );
        assert_eq!(
            census.best.as_ref().map(|p| p.id.as_str()),
            Some("claude-code")
        );
        // Without the seat the configured key is the best path.
        let keyed =
            AccessCensus::from_parts(&[api_row("anthropic", true, "ANTHROPIC_API_KEY")], vec![]);
        assert_eq!(
            keyed.best.as_ref().map(|p| p.id.as_str()),
            Some("anthropic")
        );
    }

    #[test]
    fn the_seat_fix_line_names_the_pin_never_the_wrapper() {
        let missing = seat("claude-code", false, false, false).as_path();
        let fix = missing.fix_line.expect("a fix");
        assert!(fix.contains("--access claude-code"), "{fix}");
        assert!(
            !fix.contains("claude-agent-acp"),
            "the wrapper id is not a pin (NIKA-1802): {fix}"
        );
        let no_adapter = seat("claude-code", true, false, true).as_path();
        let fix = no_adapter.fix_line.expect("a fix");
        assert!(fix.contains("--access claude-code"), "{fix}");
        assert!(!fix.contains("claude-agent-acp"), "{fix}");
        let unsigned = seat("codex", true, true, false).as_path();
        let fix = unsigned.fix_line.expect("a fix");
        assert!(
            fix.contains("sign in") && fix.contains("--access codex"),
            "{fix}"
        );
        // A ready seat teaches nothing.
        assert_eq!(seat("codex", true, true, true).as_path().fix_line, None);
    }

    /// `seats_ready` follows the ratified G-3 order, never the
    /// enumeration order (mutation pin for the sort).
    #[test]
    fn seats_ready_ride_the_ratified_order() {
        let census = AccessCensus::from_parts(
            &[],
            vec![
                seat("claude-code", true, true, true),
                seat("gemini-cli", true, true, true),
                seat("codex", true, true, true),
            ],
        );
        assert_eq!(census.seats_ready, ["gemini-cli", "codex", "claude-code"]);
    }

    #[test]
    fn the_collector_never_binds_a_value() {
        // The I/O half observes presence only: paths carry ids, custody
        // NAMES and booleans — no field can hold a secret by
        // construction.
        let census = AccessCensus::collect(ProvidersConfig::new());
        assert!(
            census.paths.iter().all(|p| !p.id.is_empty()),
            "rows are ids, not values"
        );
    }
}
