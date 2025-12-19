//! Authentication module for Nika Registry

use anyhow::Result;

/// Login to registry
pub fn login() -> Result<()> {
    eprintln!("🔐 nika auth login");
    eprintln!();
    eprintln!("Registry authentication is not yet implemented.");
    eprintln!("This feature is planned for a future release.");
    eprintln!();
    eprintln!("What will happen when implemented:");
    eprintln!("  1. Open browser for OAuth flow");
    eprintln!("  2. Store credentials in ~/.nika/auth.json");
    eprintln!("  3. Enable publishing to registry.nika.dev");
    Ok(())
}

/// Logout from registry
pub fn logout() -> Result<()> {
    eprintln!("🔐 nika auth logout");
    eprintln!();
    eprintln!("Not logged in.");
    Ok(())
}

/// Show auth status
pub fn status() -> Result<()> {
    eprintln!("🔐 nika auth status");
    eprintln!();
    eprintln!("Not logged in.");
    eprintln!("Run 'nika auth login' to authenticate with the registry.");
    Ok(())
}
