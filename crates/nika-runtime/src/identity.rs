// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Compile-bound identity shared by every engine adapter.

/// The engine, source and remote-protocol identity compiled into this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[non_exhaustive]
pub struct EngineIdentity {
    engine_version: &'static str,
    build_sha: &'static str,
    spec_sha: &'static str,
    api_version: &'static str,
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

    /// Human-facing version with a build stamp when one is available.
    #[must_use]
    pub const fn version_long(&self) -> &'static str {
        env!("NIKA_VERSION_LONG")
    }
}

static ENGINE_IDENTITY: EngineIdentity = EngineIdentity {
    engine_version: env!("CARGO_PKG_VERSION"),
    build_sha: env!("NIKA_BUILD_SHA"),
    spec_sha: env!("NIKA_SPEC_SHA"),
    api_version: "v1",
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
    fn serialized_identity_names_the_four_independent_axes() {
        let identity = engine_identity();
        let value = serde_json::to_value(identity).unwrap_or_default();
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(4));
        for key in ["engine_version", "build_sha", "spec_sha", "api_version"] {
            assert!(value.get(key).is_some(), "missing {key}: {value:#}");
        }
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
