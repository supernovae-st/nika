// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Compile-bound identity shared by every engine adapter.

use serde::ser::SerializeMap as _;

/// Current additive machine-protocol generation.
pub const MACHINE_PROTOCOL_VERSION: u32 = 1;

/// Snapshot wire generation understood by this engine build.
///
/// `nika-execution` owns the encoder. The CLI adapter parity-tests this value
/// against `nika_execution::SNAPSHOT_FORMAT_VERSION` so the L3 crates can stay
/// siblings without introducing a dependency edge solely for one constant.
pub const MACHINE_SNAPSHOT_FORMAT_VERSION: u32 = 1;

const SUPPORTED_CAPABILITIES: &[&str] = &["check", "executionSnapshot", "eventStream", "trace"];

/// The engine, source and machine-protocol identity compiled into this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct EngineIdentity {
    engine_version: &'static str,
    build_sha: &'static str,
    spec_sha: &'static str,
    api_version: &'static str,
    machine_protocol_version: u32,
    snapshot_format_version: u32,
    check_report_version: u32,
    event_format_version: u32,
    trace_format_version: u32,
    supported_capabilities: &'static [&'static str],
}

impl EngineIdentity {
    /// Cargo workspace version of this engine build.
    #[must_use]
    pub const fn engine_version(&self) -> &'static str {
        self.engine_version
    }

    /// Git build stamp, or `unknown` for a gitless unpinned build.
    #[must_use]
    pub const fn build_sha(&self) -> &'static str {
        self.build_sha
    }

    /// Exact language-spec commit shared by conformance and the embedded pack.
    #[must_use]
    pub const fn spec_sha(&self) -> &'static str {
        self.spec_sha
    }

    /// Remote execution protocol generation, distinct from the language spec.
    #[must_use]
    pub const fn api_version(&self) -> &'static str {
        self.api_version
    }

    /// Additive machine-protocol generation shared by local and HTTP adapters.
    #[must_use]
    pub const fn machine_protocol_version(&self) -> u32 {
        self.machine_protocol_version
    }

    /// Immutable execution-snapshot wire generation this build can emit.
    #[must_use]
    pub const fn snapshot_format_version(&self) -> u32 {
        self.snapshot_format_version
    }

    /// Static check-report JSON generation this build emits.
    #[must_use]
    pub const fn check_report_version(&self) -> u32 {
        self.check_report_version
    }

    /// Event payload generation this build emits.
    #[must_use]
    pub const fn event_format_version(&self) -> u32 {
        self.event_format_version
    }

    /// Trace journal generation this build emits.
    #[must_use]
    pub const fn trace_format_version(&self) -> u32 {
        self.trace_format_version
    }

    /// Stable capability tokens implemented by this engine build.
    #[must_use]
    pub const fn supported_capabilities(&self) -> &'static [&'static str] {
        self.supported_capabilities
    }

    /// Human-facing version with a build stamp when one is available.
    #[must_use]
    pub const fn version_long(&self) -> &'static str {
        env!("NIKA_VERSION_LONG")
    }
}

impl serde::Serialize for EngineIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // The four snake_case keys shipped first and remain readable. The
        // camelCase vector is the SDK contract every adapter now shares.
        let mut map = serializer.serialize_map(Some(13))?;
        map.serialize_entry("engine_version", self.engine_version)?;
        map.serialize_entry("build_sha", self.build_sha)?;
        map.serialize_entry("spec_sha", self.spec_sha)?;
        map.serialize_entry("api_version", self.api_version)?;
        map.serialize_entry("engineVersion", self.engine_version)?;
        map.serialize_entry("buildSha", self.build_sha)?;
        map.serialize_entry("specSha", self.spec_sha)?;
        map.serialize_entry("machineProtocolVersion", &self.machine_protocol_version)?;
        map.serialize_entry("snapshotFormatVersion", &self.snapshot_format_version)?;
        map.serialize_entry("checkReportVersion", &self.check_report_version)?;
        map.serialize_entry("eventFormatVersion", &self.event_format_version)?;
        map.serialize_entry("traceFormatVersion", &self.trace_format_version)?;
        map.serialize_entry("supportedCapabilities", self.supported_capabilities)?;
        map.end()
    }
}

#[allow(
    clippy::cast_lossless,
    reason = "From<u16> is not const in this compile-bound static initializer"
)]
static ENGINE_IDENTITY: EngineIdentity = EngineIdentity {
    engine_version: env!("CARGO_PKG_VERSION"),
    build_sha: env!("NIKA_BUILD_SHA"),
    spec_sha: env!("NIKA_SPEC_SHA"),
    api_version: "v1",
    machine_protocol_version: MACHINE_PROTOCOL_VERSION,
    snapshot_format_version: MACHINE_SNAPSHOT_FORMAT_VERSION,
    check_report_version: nika_check::REPORT_VERSION,
    event_format_version: nika_types::EventSchemaVersion::CURRENT.version as u32,
    trace_format_version: nika_types::TraceFormatVersion::CURRENT.version as u32,
    supported_capabilities: SUPPORTED_CAPABILITIES,
};

/// Return the single identity compiled into the engine.
#[must_use]
pub const fn engine_identity() -> &'static EngineIdentity {
    &ENGINE_IDENTITY
}

#[cfg(test)]
mod tests {
    use super::engine_identity;

    #[test]
    fn serialized_identity_adds_the_sdk_vector_without_removing_legacy_keys() {
        let identity = engine_identity();
        let value = serde_json::to_value(identity).unwrap_or_default();
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(13));
        for key in ["engine_version", "build_sha", "spec_sha", "api_version"] {
            assert!(value.get(key).is_some(), "missing {key}: {value:#}");
        }
        for key in [
            "engineVersion",
            "buildSha",
            "specSha",
            "machineProtocolVersion",
            "snapshotFormatVersion",
            "checkReportVersion",
            "eventFormatVersion",
            "traceFormatVersion",
            "supportedCapabilities",
        ] {
            assert!(value.get(key).is_some(), "missing {key}: {value:#}");
        }
        assert_eq!(value["engineVersion"], value["engine_version"]);
        assert_eq!(value["buildSha"], value["build_sha"]);
        assert_eq!(value["specSha"], value["spec_sha"]);
        assert_eq!(
            value["checkReportVersion"],
            serde_json::json!(nika_check::REPORT_VERSION)
        );
        assert_eq!(
            value["eventFormatVersion"],
            serde_json::json!(nika_types::EventSchemaVersion::CURRENT.version)
        );
        assert_eq!(
            value["traceFormatVersion"],
            serde_json::json!(nika_types::TraceFormatVersion::CURRENT.version)
        );
        assert_ne!(identity.spec_sha(), identity.api_version());
    }

    #[test]
    fn compiled_spec_identity_is_the_committed_pin_and_pack_marker() {
        let pin = include_str!("../../../SPEC_PIN");
        let pack = include_str!("../../nika-pack/pack/SPEC_SHA");
        assert_eq!(
            crate::build_support::matching_spec_sha(pin, pack),
            Ok(engine_identity().spec_sha())
        );
    }
}
