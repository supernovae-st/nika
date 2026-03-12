//! Setup Module — Interactive Onboarding Wizard
//!
//! Provides guided setup for Nika, NovaNet, and Claude Code integration.
//!
//! ## Commands
//!
//! - `nika setup` — Interactive wizard (auto-detect what's needed)
//! - `nika setup nika` — Install Nika + LSP + Daemon
//! - `nika setup novanet` — Configure NovaNet + Neo4j
//! - `nika setup claude-code` — Configure Claude Code

mod wizard;

pub use wizard::{run_setup, SetupResult, SetupTarget};

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _ = std::mem::size_of::<SetupTarget>();
        let _ = std::mem::size_of::<SetupResult>();
    }
}
