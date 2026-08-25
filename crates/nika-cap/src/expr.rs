// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The EXPRESSION boundary — what a jq program may observe or affect.
//!
//! D-2026-08-11-N26 makes expressions input-only; N27 preserves jq's clock
//! spellings by rebinding them to the run-start value supplied by an
//! effect-owning caller. This module is the typed policy shared by the jq
//! builtin, output bindings, and static checker. It deliberately has no jaq
//! dependency: consumers install upstream symbols according to this table.

use nika_types::timestamp::{NS_PER_SEC, Timestamp};

/// The pure value an effect-owning caller binds to jq's accepted `now` form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct JqClock {
    run_started_at: Timestamp,
}

impl JqClock {
    /// Bind a run-start timestamp supplied by the runtime/composition layer.
    #[must_use]
    pub const fn at(run_started_at: Timestamp) -> Self {
        Self { run_started_at }
    }

    /// Convert a caller-supplied system instant without reading the host here.
    #[must_use]
    pub fn from_system_time(time: std::time::SystemTime) -> Self {
        let unix_ns = match time.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX),
            Err(before) => -i64::try_from(before.duration().as_nanos()).unwrap_or(i64::MAX),
        };
        Self::at(Timestamp::from_unix_ns(unix_ns))
    }

    /// Seconds since the Unix epoch in jq's numeric clock representation.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // jq's `now` contract is an f64
    pub fn unix_seconds(self) -> f64 {
        self.run_started_at.unix_ns as f64 / NS_PER_SEC as f64
    }
}

/// Where a jq symbol is defined upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum JqSymbolKind {
    /// A Rust native returned by `funs()`.
    Native,
    /// A jq definition returned by `defs()`.
    Definition,
}

/// The capability class a jq symbol belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JqCapability {
    /// Deterministic run-start time supplied as a value by the caller.
    RunStartClock,
    /// Ambient process environment.
    HostEnvironment,
    /// Diagnostic output to the host.
    HostDiagnostics,
    /// Halt/process-exit control.
    ProcessControl,
}

/// How an effect-bearing upstream jq symbol is admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JqDisposition {
    /// Remove the upstream native and replace its spelling with a pure def.
    Rebind,
    /// Do not install the symbol in an expression evaluator.
    Withhold,
}

/// One effect-bearing jq symbol and its canonical admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct JqCapabilityRule {
    /// The exact upstream symbol name.
    pub name: &'static str,
    /// Whether the upstream surface is a native or a definition.
    pub kind: JqSymbolKind,
    /// The effect class controlled by this row.
    pub capability: JqCapability,
    /// Whether callers rebind or withhold it.
    pub disposition: JqDisposition,
    /// What the upstream implementation would observe or affect.
    pub effect: &'static str,
    /// The safe alternative named in an author-facing refusal.
    pub instead: &'static str,
}

impl JqCapabilityRule {
    /// Construct a policy row (FCI-002 constructor for this evolvable DTO).
    #[must_use]
    pub const fn new(
        name: &'static str,
        kind: JqSymbolKind,
        capability: JqCapability,
        disposition: JqDisposition,
        effect: &'static str,
        instead: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            capability,
            disposition,
            effect,
            instead,
        }
    }
}

const ENV_NATIVE_NAME: &str = "env";
const ENV_NATIVE_EFFECT: &str = "the ambient process environment";
const ENV_NATIVE_POLICY_INSTEAD: &str =
    "pass the value through `inputs:`, `const:`, or a governed `secrets:` reference";
const ENV_NATIVE_LEGACY_INSTEAD: &str = "pass the value in — `inputs:` (the caller), \
`const:` (the author) or `secrets:` (a governed store reference); a CHILD process \
receives its environment through `permits.env` on an `exec:` task";

/// Compatibility view of a withheld jq native.
///
/// New policy consumers should use [`JqCapabilityRule`]. This historical DTO
/// remains public so v0.114 embedders keep their source paths while its one
/// row is projected from the same constants as [`JQ_CAPABILITY_POLICY`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WithheldNative {
    /// The exact upstream native name.
    pub name: &'static str,
    /// What the upstream implementation would read.
    pub reads: &'static str,
    /// The safe alternative named in an author-facing refusal.
    pub instead: &'static str,
}

impl WithheldNative {
    /// Construct a compatibility row.
    #[must_use]
    pub const fn new(name: &'static str, reads: &'static str, instead: &'static str) -> Self {
        Self {
            name,
            reads,
            instead,
        }
    }
}

/// The global variable installed beside [`JQ_CLOCK_DEFS`].
pub const JQ_RUN_START_VAR: &str = "$nika_run_start";

/// Pure replacements for jq's accepted clock/timezone spellings.
///
/// `now` reads the injected run-start value. `localtime` and
/// `strflocaltime` are deterministic UTC projections because the workflow
/// language declares a clock but no host-timezone authority. The upstream
/// natives are removed first, so these definitions cannot reach the host.
pub const JQ_CLOCK_DEFS: &str = r"
def now: $nika_run_start;
def localtime: gmtime;
def strflocaltime($format): strftime($format);
";

/// Canonical policy for every upstream jq symbol that is not input-only.
///
/// Symbols absent from this table are ordinary input-dependent compute and
/// remain installed. A new effectful upstream symbol must add one typed row;
/// the three consumers test their installed surfaces against this policy.
pub const JQ_CAPABILITY_POLICY: &[JqCapabilityRule] = &[
    JqCapabilityRule::new(
        ENV_NATIVE_NAME,
        JqSymbolKind::Native,
        JqCapability::HostEnvironment,
        JqDisposition::Withhold,
        ENV_NATIVE_EFFECT,
        ENV_NATIVE_POLICY_INSTEAD,
    ),
    JqCapabilityRule::new(
        "now",
        JqSymbolKind::Native,
        JqCapability::RunStartClock,
        JqDisposition::Rebind,
        "the ambient host clock",
        "use the run-start value bound by the engine",
    ),
    JqCapabilityRule::new(
        "localtime",
        JqSymbolKind::Native,
        JqCapability::RunStartClock,
        JqDisposition::Rebind,
        "the ambient host timezone",
        "use the deterministic UTC projection bound by the engine",
    ),
    JqCapabilityRule::new(
        "strflocaltime",
        JqSymbolKind::Native,
        JqCapability::RunStartClock,
        JqDisposition::Rebind,
        "the ambient host timezone",
        "use the deterministic UTC projection bound by the engine",
    ),
    JqCapabilityRule::new(
        "debug_empty",
        JqSymbolKind::Native,
        JqCapability::HostDiagnostics,
        JqDisposition::Withhold,
        "host diagnostic output",
        "return the diagnostic as data",
    ),
    JqCapabilityRule::new(
        "stderr_empty",
        JqSymbolKind::Native,
        JqCapability::HostDiagnostics,
        JqDisposition::Withhold,
        "host diagnostic output",
        "return the diagnostic as data",
    ),
    JqCapabilityRule::new(
        "halt",
        JqSymbolKind::Native,
        JqCapability::ProcessControl,
        JqDisposition::Withhold,
        "host process control",
        "return an error value through the evaluator's typed result",
    ),
    JqCapabilityRule::new(
        "debug",
        JqSymbolKind::Definition,
        JqCapability::HostDiagnostics,
        JqDisposition::Withhold,
        "host diagnostic output",
        "return the diagnostic as data",
    ),
    JqCapabilityRule::new(
        "stderr",
        JqSymbolKind::Definition,
        JqCapability::HostDiagnostics,
        JqDisposition::Withhold,
        "host diagnostic output",
        "return the diagnostic as data",
    ),
    JqCapabilityRule::new(
        "halt",
        JqSymbolKind::Definition,
        JqCapability::ProcessControl,
        JqDisposition::Withhold,
        "host process control",
        "return an error value through the evaluator's typed result",
    ),
    JqCapabilityRule::new(
        "halt_error",
        JqSymbolKind::Definition,
        JqCapability::ProcessControl,
        JqDisposition::Withhold,
        "host process control",
        "return an error value through the evaluator's typed result",
    ),
];

/// The v0.114 native-withholding view retained for source compatibility.
///
/// This view intentionally retains its historical `env`-only shape. The
/// typed [`JQ_CAPABILITY_POLICY`] is authoritative for the expanded v0.115
/// classes, including diagnostics, process control, and rebound clocks.
pub const WITHHELD_JQ_NATIVES: &[WithheldNative] = &[WithheldNative::new(
    ENV_NATIVE_NAME,
    ENV_NATIVE_EFFECT,
    ENV_NATIVE_LEGACY_INSTEAD,
)];

/// Return the historical compatibility row for a withheld native.
#[must_use]
pub fn withheld_jq_native(name: &str) -> Option<&'static WithheldNative> {
    WITHHELD_JQ_NATIVES.iter().find(|rule| rule.name == name)
}

/// The policy row for one upstream symbol, if it is effect-bearing.
#[must_use]
pub fn jq_capability_rule(name: &str, kind: JqSymbolKind) -> Option<&'static JqCapabilityRule> {
    JQ_CAPABILITY_POLICY
        .iter()
        .find(|rule| rule.name == name && rule.kind == kind)
}

/// Whether an upstream native may be installed unchanged.
#[must_use]
pub fn install_jq_native(name: &str) -> bool {
    jq_capability_rule(name, JqSymbolKind::Native).is_none()
}

/// Whether an upstream definition may be installed unchanged.
#[must_use]
pub fn install_jq_definition(name: &str) -> bool {
    jq_capability_rule(name, JqSymbolKind::Definition).is_none()
}

/// The v0.114 author-facing refusal for a historically withheld native.
#[must_use]
pub fn withheld_jq_reason(name: &str) -> Option<String> {
    withheld_jq_native(name).map(|rule| {
        format!(
            "`{}` is withheld — it reads {}, and an expression sees only its input; {}",
            rule.name, rule.reads, rule.instead
        )
    })
}

/// The author-facing refusal for any symbol withheld by the expanded policy.
///
/// Rebound clock symbols return `None`: their accepted spelling compiles to
/// [`JQ_CLOCK_DEFS`], so presenting them as refused would contradict runtime.
#[must_use]
pub fn withheld_jq_policy_reason(name: &str) -> Option<String> {
    JQ_CAPABILITY_POLICY
        .iter()
        .find(|rule| rule.name == name && rule.disposition == JqDisposition::Withhold)
        .map(|rule| {
            format!(
                "`{}` is withheld — it reaches {}, and an expression sees only its input; {}",
                rule.name, rule.effect, rule.instead
            )
        })
}

/// Compatibility predicate for callers that only ask about native symbols.
#[must_use]
pub fn is_withheld_jq_native(name: &str) -> bool {
    withheld_jq_native(name).is_some()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn policy_is_unique_by_surface_and_complete() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in JQ_CAPABILITY_POLICY {
            assert!(!rule.name.is_empty());
            assert!(!rule.effect.is_empty());
            assert!(!rule.instead.is_empty());
            assert!(seen.insert((rule.name, rule.kind)), "duplicate {rule:?}");
        }
        assert_eq!(seen.len(), 11);
    }

    #[test]
    fn one_policy_drives_installation_and_refusals() {
        for name in ["env", "debug_empty", "stderr_empty", "halt"] {
            assert!(!install_jq_native(name), "{name}");
        }
        for name in ["debug", "stderr", "halt", "halt_error"] {
            assert!(!install_jq_definition(name), "{name}");
            assert!(withheld_jq_policy_reason(name).is_some(), "{name}");
        }
        for name in ["now", "localtime", "strflocaltime"] {
            assert!(!install_jq_native(name), "upstream {name} must be replaced");
            assert!(
                withheld_jq_policy_reason(name).is_none(),
                "{name} stays accepted"
            );
        }
        for name in ["strftime", "gmtime", "mktime", "map", "length"] {
            assert!(install_jq_native(name));
            assert!(install_jq_definition(name));
        }
    }

    #[test]
    fn environment_reason_names_the_governed_route() {
        let reason = withheld_jq_reason("env").expect("env is withheld");
        assert_eq!(
            reason,
            "`env` is withheld — it reads the ambient process environment, and an expression \
sees only its input; pass the value in — `inputs:` (the caller), `const:` (the author) or \
`secrets:` (a governed store reference); a CHILD process receives its environment through \
`permits.env` on an `exec:` task"
        );
        assert!(withheld_jq_reason("envv").is_none());
    }

    #[test]
    fn legacy_native_view_preserves_the_v0114_public_contract() {
        let row = withheld_jq_native("env").expect("legacy env row remains public");
        assert_eq!(row.name, ENV_NATIVE_NAME);
        assert_eq!(row.reads, ENV_NATIVE_EFFECT);
        assert_eq!(row.instead, ENV_NATIVE_LEGACY_INSTEAD);
        assert_eq!(WITHHELD_JQ_NATIVES, &[row.to_owned()]);
        for name in [
            "debug_empty",
            "stderr_empty",
            "halt",
            "now",
            "localtime",
            "strflocaltime",
            "debug",
            "stderr",
            "halt_error",
        ] {
            assert!(withheld_jq_native(name).is_none(), "{name}");
            assert!(!is_withheld_jq_native(name), "{name}");
            assert!(withheld_jq_reason(name).is_none(), "{name}");
        }
        assert!(is_withheld_jq_native("env"));
    }
}
