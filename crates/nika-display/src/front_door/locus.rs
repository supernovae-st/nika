// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Passive data-location labels over host-resolved facts; no configuration or authority.

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
    /// Through an OpenAI-compatible gateway: the provider id names the
    /// wire, the host names where the bytes go.
    Gateway {
        /// The provider id the wire speaks.
        provider: String,
        /// The host the base URL override points at.
        host: String,
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
            Self::Gateway { provider, host } => format!(
                "{provider}-compatible gateway · {host} · metered · project context you ask Nika to reason over is sent to {host}, not to {provider}"
            ),
            Self::Local => "local · private · project context stays on this machine".to_owned(),
            Self::None => {
                "no conversational AI · nothing leaves this machine · the facts stay".to_owned()
            }
        }
    }
}
