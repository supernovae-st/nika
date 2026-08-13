// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Read-only capability view trait for the Connectome's forward-compat.
//!
//! [`ModelCapabilitiesView`] abstracts over [`ModelCapabilities`] so that
//! the Connectome can provide alternative capability sources (overlay from
//! `pck`, runtime discovery, etc.) without coupling to the concrete struct.

use super::model::{ModelCapabilities, TokenLimitParam};
use super::{JsonMode, Modality, ParamFlag, TokenizerFamily};

/// Read-only view of a model's capabilities.
///
/// Blanket-implemented for [`ModelCapabilities`]. The Connectome
/// subsystem will implement this for overlay capability sources
/// (package-provided caps, runtime-discovered caps) so the runtime
/// can program against capabilities without knowing the concrete
/// backing store.
///
/// ```
/// use nika_catalog::types::{ModelCapabilities, TokenLimitParam};
/// use nika_catalog::types::capabilities_view::ModelCapabilitiesView;
///
/// let caps = ModelCapabilities::default();
/// assert_eq!(caps.token_limit_param(), TokenLimitParam::MaxTokens);
/// assert!(caps.supports_temperature());
/// ```
pub trait ModelCapabilitiesView {
    /// Which JSON field to use for token limits.
    fn token_limit_param(&self) -> TokenLimitParam;
    /// Model accepts the `temperature` parameter.
    fn supports_temperature(&self) -> bool;
    /// Model accepts `stop` / `stop_sequences` parameter.
    fn supports_stop_sequences(&self) -> bool;
    /// Model exposes a reasoning / extended-thinking mode.
    fn reasoning(&self) -> bool;
    /// Whether this model accepts a `system` role message.
    fn supports_system_messages(&self) -> bool;
    /// Maximum context window size in tokens.
    fn context_window_tokens(&self) -> Option<u32>;
    /// Maximum output tokens the model can produce.
    fn max_output_tokens(&self) -> Option<u32>;
    /// Structured JSON output capability level.
    fn json_mode(&self) -> Option<JsonMode>;
    /// Modalities this model accepts as input.
    fn input_modalities(&self) -> &[Modality];
    /// Modalities this model produces as output.
    fn output_modalities(&self) -> &[Modality];
    /// Known tokenizer family for context-window estimation.
    fn tokenizer(&self) -> Option<TokenizerFamily>;
    /// API-level parameter capability flags.
    fn supported_parameters(&self) -> &[ParamFlag];
}

impl ModelCapabilitiesView for ModelCapabilities {
    #[inline]
    fn token_limit_param(&self) -> TokenLimitParam {
        self.token_limit_param
    }
    #[inline]
    fn supports_temperature(&self) -> bool {
        self.supports_temperature
    }
    #[inline]
    fn supports_stop_sequences(&self) -> bool {
        self.supports_stop_sequences
    }
    #[inline]
    fn reasoning(&self) -> bool {
        self.reasoning
    }
    #[inline]
    fn supports_system_messages(&self) -> bool {
        self.supports_system_messages
    }
    #[inline]
    fn context_window_tokens(&self) -> Option<u32> {
        self.context_window_tokens
    }
    #[inline]
    fn max_output_tokens(&self) -> Option<u32> {
        self.max_output_tokens
    }
    #[inline]
    fn json_mode(&self) -> Option<JsonMode> {
        self.json_mode
    }
    #[inline]
    fn input_modalities(&self) -> &[Modality] {
        self.input_modalities
    }
    #[inline]
    fn output_modalities(&self) -> &[Modality] {
        self.output_modalities
    }
    #[inline]
    fn tokenizer(&self) -> Option<TokenizerFamily> {
        self.tokenizer
    }
    #[inline]
    fn supported_parameters(&self) -> &[ParamFlag] {
        self.supported_parameters
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Modality;

    #[test]
    fn blanket_impl_returns_matching_fields() {
        let caps = ModelCapabilities::default();
        assert_eq!(caps.token_limit_param(), TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature());
        assert!(caps.supports_stop_sequences());
        assert!(!caps.reasoning());
        assert!(caps.supports_system_messages());
        assert_eq!(caps.context_window_tokens(), None);
        assert_eq!(caps.max_output_tokens(), None);
        assert_eq!(caps.json_mode(), None);
        assert!(caps.input_modalities().contains(&Modality::Text));
        assert!(caps.output_modalities().contains(&Modality::Text));
        assert_eq!(caps.tokenizer(), None);
        assert!(caps.supported_parameters().is_empty());
    }

    #[test]
    fn trait_is_object_safe() {
        // Compile-time proof: can create &dyn ModelCapabilitiesView.
        fn accepts_dyn(v: &dyn ModelCapabilitiesView) -> bool {
            v.supports_temperature()
        }
        let caps = ModelCapabilities::default();
        assert!(accepts_dyn(&caps));
    }
}
