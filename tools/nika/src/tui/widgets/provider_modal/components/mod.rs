//! UI components for provider modal
//!
//! v0.8.9: Added AnimatedHeader for cosmic WOW effects
//! v0.8.9: Added FooterStatsBar for session metrics
//! v0.8.9: Added VerificationEffect for Matrix-style provider checking

mod animated_header;
mod download_gauge;
mod footer_stats;
mod provider_card;
mod verification_effect;

pub use animated_header::*;
pub use download_gauge::*;
pub use footer_stats::*;
pub use provider_card::*;
pub use verification_effect::*;
