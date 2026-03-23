//! Provider Selector — Data types only
//!
//! Only `VerifyStatus` is kept (used by `verification.rs` and `app/lifecycle.rs`).

/// Connection verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyStatus {
    /// Not yet verified
    #[default]
    Unknown,
    /// Verification in progress
    Verifying,
    /// Successfully verified
    Verified,
    /// Verification failed
    Failed,
}
