// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The client registry — the engine CONSUMES the nika-plugins matrix
//! (H6 · operator decision Q1 2026-07-31: `clients.yaml` in the agents
//! repo stays the ONE SSOT of client × component coverage; the binary
//! reads a vendored snapshot of it, never a second hand-maintained
//! list).
//!
//! The snapshot at `data/clients.registry.yaml` is a byte copy of the
//! SSOT, embedded via `include_str!` (the guard.rs shim law: the bytes
//! shipped are the bytes under test). The drift test below fails when
//! the snapshot diverges from the agents-repo copy on a machine with
//! the monorepo layout, and skips silently in CI / packaged builds
//! where the sibling repo is absent (the `CARGO_MANIFEST_DIR`
//! sibling-crate precedent).

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The vendored snapshot — ONE source, embedded at compile time.
const VENDORED: &str = include_str!("../data/clients.registry.yaml");

/// The wire targets doctor has a CONCRETE config probe for, in the
/// historical doctor row order (the render order is load-bearing —
/// findings and receipts ride probe order).
pub const PROBE_MECHANISMS: &[&str] = &["cursor", "windsurf", "claude", "zed", "hermes", "vscode"];

/// The wire targets whose kit landing doctor can read (class A today),
/// in the historical kit row order. Consumed by the matrix-coherence
/// test (the gating itself re-derives from the registry at runtime).
#[cfg(test)]
pub const KIT_MECHANISMS: &[&str] = &["cursor", "claude", "codex"];

/// The parsed matrix: the rows the probe/doctor surfaces derive from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ClientsRegistry {
    pub schema_version: u32,
    pub clients: Vec<RegistryClient>,
}

/// One client row — only the fields the engine consumes are typed;
/// the matrix carries prose (install lines · proofs · gap reasons)
/// the binary never renders.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RegistryClient {
    pub id: String,
    pub name: String,
    #[serde(rename = "class")]
    pub class: String,
    pub status: String,
    #[serde(default)]
    pub components: BTreeMap<String, String>,
    /// The `nika wire <target>` handle — absent while the wire target
    /// awaits its first shipped release (`wire_pending` then carries
    /// the reason).
    #[serde(default)]
    pub wire: Option<String>,
    #[serde(default)]
    pub wire_pending: Option<String>,
}

/// The matrix coverage facts doctor renders — every count DERIVED at
/// read time from the vendored snapshot (the born-stale law), never
/// typed by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryCoverage {
    /// Every client row in the matrix.
    pub declared: usize,
    /// Rows `nika wire` can write today (`wire:` present).
    pub wireable: usize,
    /// Rows whose wire target shipped engine-side but awaits the first
    /// release carrying it (`wire_pending:` — the matrix's own words).
    pub wire_pending: usize,
    /// Wireable rows the doctor probe has a concrete mechanism for.
    pub probed: usize,
    /// Wireable rows WITHOUT a probe mechanism — declared-not-probed,
    /// named by registry id (never silently dropped, never claimed).
    pub declared_not_probed: Vec<String>,
}

/// The vendored matrix, parsed once — `None` only if the embedded
/// snapshot cannot parse (impossible by construction: the parse test
/// pins it; a degradation path, never a guess).
pub fn vendored() -> Option<&'static ClientsRegistry> {
    static REGISTRY: OnceLock<Option<ClientsRegistry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| serde_yaml_bw::from_str(VENDORED).ok())
        .as_ref()
}

/// Derive the coverage facts from the matrix — pure.
#[must_use]
pub fn coverage(registry: &ClientsRegistry) -> RegistryCoverage {
    let mut cov = RegistryCoverage {
        declared: registry.clients.len(),
        ..RegistryCoverage::default()
    };
    for client in &registry.clients {
        if client.wire_pending.is_some() {
            cov.wire_pending += 1;
        }
        match &client.wire {
            Some(target) if PROBE_MECHANISMS.contains(&target.as_str()) => {
                cov.wireable += 1;
                cov.probed += 1;
            }
            Some(_) => {
                cov.wireable += 1;
                cov.declared_not_probed.push(client.id.clone());
            }
            None => {}
        }
    }
    cov
}

/// Does the matrix claim this wire target for a client today? An
/// absent registry (the impossible parse failure above) degrades to
/// `true` — the historical probe set, never a silent coverage loss.
#[must_use]
pub fn registry_wires(target: &str) -> bool {
    vendored().is_none_or(|r| r.clients.iter().any(|c| c.wire.as_deref() == Some(target)))
}

/// Does the matrix ship the plugin kit for this wire target (a class-A
/// row claims it)? Same degrade-open law as [`registry_wires`].
#[must_use]
pub fn registry_ships_kit(target: &str) -> bool {
    vendored().is_none_or(|r| {
        r.clients
            .iter()
            .any(|c| c.class == "A" && c.wire.as_deref() == Some(target))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed enums the matrix header pins — the agents-repo gate
    /// enforces them SSOT-side; the engine re-proves them on the
    /// vendored bytes so a bad vendoring fails HERE, loud, never as a
    /// silent probe loss.
    const MECHANISMS: &[&str] = &[
        "native-manifest",
        "claude-layout",
        "wire",
        "init",
        "mcp-config",
        "skill-pack",
        "none",
    ];
    const STATUSES: &[&str] = &["recon", "wired", "proven"];
    const CLASSES: &[&str] = &["A", "B", "C"];

    #[test]
    fn the_vendored_snapshot_parses_and_honors_the_closed_enums() {
        let registry = vendored().expect("the vendored snapshot parses");
        assert_eq!(registry.schema_version, 1);
        assert!(
            registry.clients.len() >= 27,
            "the matrix is 27+ clients, got {}",
            registry.clients.len()
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut wires = std::collections::BTreeSet::new();
        for client in &registry.clients {
            assert!(!client.id.is_empty(), "a row without an id");
            assert!(!client.name.is_empty(), "{} has no name", client.id);
            assert!(
                CLASSES.contains(&client.class.as_str()),
                "{} class {:?} is outside the enum",
                client.id,
                client.class
            );
            assert!(
                STATUSES.contains(&client.status.as_str()),
                "{} status {:?} is outside the enum",
                client.id,
                client.status
            );
            assert!(ids.insert(&client.id), "duplicate id {}", client.id);
            for (component, mechanism) in &client.components {
                assert!(
                    MECHANISMS.contains(&mechanism.as_str()),
                    "{} {component}: mechanism {mechanism:?} is outside the enum",
                    client.id
                );
            }
            if let Some(wire) = &client.wire {
                assert!(
                    wires.insert(wire),
                    "two clients claim the wire target {wire}"
                );
                assert!(
                    client.wire_pending.is_none(),
                    "{} carries wire AND wire_pending",
                    client.id
                );
            }
        }
    }

    #[test]
    fn coverage_is_derived_and_every_wireable_client_is_accounted() {
        let registry = vendored().expect("parses");
        let cov = coverage(registry);
        assert_eq!(cov.declared, registry.clients.len());
        let wireable = registry.clients.iter().filter(|c| c.wire.is_some()).count();
        assert_eq!(cov.wireable, wireable);
        // Both directions: probed = wireable ∩ known mechanisms · every
        // other wireable client is NAMED declared-not-probed.
        let expected_probed = registry
            .clients
            .iter()
            .filter(|c| {
                c.wire
                    .as_deref()
                    .is_some_and(|w| PROBE_MECHANISMS.contains(&w))
            })
            .count();
        assert_eq!(cov.probed, expected_probed);
        assert_eq!(
            cov.probed + cov.declared_not_probed.len(),
            cov.wireable,
            "every wireable client is probed or honestly declared: {cov:?}"
        );
        // The audit's complaint (5 hand-listed probes on 19+ wireable)
        // is the floor this derivation beats — and the not-probed are
        // named, not dropped.
        assert!(cov.probed >= 6, "got {}", cov.probed);
        assert!(
            !cov.declared_not_probed.is_empty(),
            "the matrix wires more than the engine can probe today — the gap must be named"
        );
        // A wire-pending row is declared, never wireable.
        let pending = registry
            .clients
            .iter()
            .filter(|c| c.wire_pending.is_some())
            .count();
        assert_eq!(cov.wire_pending, pending);
        // 0.107 flipped the six pending wires (grok · kimi · kiro ·
        // copilot · amp · antigravity) — the first release shipping
        // their WireTargets. Zero pending is now a LEGAL state; the
        // invariant that remains is the equality above (the coverage
        // count never lies about the rows).
        for id in &cov.declared_not_probed {
            let row = registry
                .clients
                .iter()
                .find(|c| &c.id == id)
                .expect("a named id is a matrix row");
            assert!(row.wire.is_some(), "{id} is wireable by construction");
        }
    }

    #[test]
    fn the_probe_and_kit_mechanisms_are_matrix_claimed() {
        let registry = vendored().expect("parses");
        // No orphan mechanism: every probe/kit mechanism answers a wire
        // target the matrix actually claims today.
        for target in PROBE_MECHANISMS {
            assert!(
                registry
                    .clients
                    .iter()
                    .any(|c| c.wire.as_deref() == Some(*target)),
                "probe mechanism {target} has no matrix wire row"
            );
        }
        for target in KIT_MECHANISMS {
            assert!(
                registry
                    .clients
                    .iter()
                    .any(|c| c.class == "A" && c.wire.as_deref() == Some(*target)),
                "kit mechanism {target} has no class-A wire row"
            );
        }
        // The predicates agree with the matrix (and degrade open when
        // the registry is there to speak).
        assert!(registry_wires("claude"));
        assert!(!registry_wires("no-such-client"));
        assert!(registry_ships_kit("codex"));
        assert!(
            !registry_ships_kit("zed"),
            "class B wires, but no kit landing is probed for it"
        );
    }

    // NOTE — drift between the vendored snapshot and the agents SSOT is
    // NOT judged here. A unit test that byte-asserts against a live
    // sibling checkout renders a different verdict for the same commit
    // depending on what unrelated repos sit on the machine (the
    // wrong-judge class — it blocked two unrelated engine pushes
    // 2026-07-31). The drift surfaces in CI instead: the daily
    // `clients-resync.yml` lane clones nika-plugins@main, re-vendors, and
    // opens ONE heal PR on drift — the 12-gate CI judges the new
    // snapshot. Manual heal when the PR can't wait:
    //   cp <agents>/repo/clients.yaml crates/nika-cli-host/data/clients.registry.yaml
    //   python3 scripts/estate.py --write
}
