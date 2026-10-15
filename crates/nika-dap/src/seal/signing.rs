// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Whether this binary's code signature can keep a macOS keychain grant.
//!
//! A keychain item's grant (« Always Allow ») is bound to the designated
//! requirement of the binary that asked. A Developer ID release (an Apple
//! anchor, a Team ID) keeps the same requirement from one version to the next,
//! so its grant sticks. An ad-hoc linker signature (every `cargo build`, every
//! dev build) is a new identity on each build: the grant never sticks and the
//! keychain password prompt returns on every run. Such a binary never opens
//! the keychain; its custody is the explicit file pair or the 0600 files.
//! It still learns whether the keychain holds a run-key, and when the item
//! last changed, from its attributes alone with the keychain's user interface
//! off ([`keychain_item`]), so it can say whether a signed release signs with
//! another key.

use std::sync::OnceLock;

/// The requirement a binary meets when an Apple-issued certificate whose leaf
/// names a Team ID signed it: the signatures whose keychain grant sticks.
#[cfg(target_os = "macos")]
const KEEPS_A_GRANT: &str = "anchor apple generic and certificate leaf[subject.OU] exists";

/// Whether this running binary's signature keeps a keychain grant, asked of
/// the running code once per process.
pub(crate) fn grant_can_stick() -> bool {
    static STICKS: OnceLock<bool> = OnceLock::new();
    *STICKS.get_or_init(probe)
}

/// The running code against [`KEEPS_A_GRANT`]: ad-hoc, unsigned or
/// unreadable code keeps no grant, never a guess the other way.
#[cfg(target_os = "macos")]
fn probe() -> bool {
    use security_framework::os::macos::code_signing::{Flags, SecCode, SecRequirement};
    let Ok(requirement) = KEEPS_A_GRANT.parse::<SecRequirement>() else {
        return false;
    };
    SecCode::for_self(Flags::NONE)
        .and_then(|code| code.check_validity(Flags::BASIC_VALIDATE_ONLY, &requirement))
        .is_ok()
}

/// Off macOS no keychain backend binds its grant to the binary that asked:
/// there is nothing to keep, and nothing changes.
#[cfg(not(target_os = "macos"))]
fn probe() -> bool {
    true
}

/// `errSecItemNotFound`: a search that matched nothing.
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25_300;

/// What the keychain says of one generic password, from its attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "off macOS no keychain item is ever read")
)]
pub(crate) enum KeychainItem {
    /// No such item.
    Absent,
    /// The item, with its modification date as the keychain describes it
    /// (`None` when it gives none).
    Present(Option<String>),
}

/// The key of `kSecAttrModificationDate` in an item's attributes.
#[cfg(target_os = "macos")]
const MODIFIED: &str = "mdat";

/// The generic password `service`/`account`, read from the item's
/// attributes only, never its data (what `security find-generic-password`
/// prints without `-g`), with the keychain's user interface off for the
/// search: no password is ever asked, whatever this binary's signature, and a
/// locked keychain answers « unknown » instead of asking to be unlocked.
/// `None` when the search itself failed, or off macOS (unknown, never a
/// guess).
#[cfg(target_os = "macos")]
pub(crate) fn keychain_item(service: &str, account: &str) -> Option<KeychainItem> {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
    use security_framework::os::macos::keychain::SecKeychain;
    // Re-enabled when dropped, at the end of this search.
    let _quiet = SecKeychain::disable_user_interaction().ok()?;
    let found = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .account(account)
        .load_attributes(true)
        .load_data(false)
        .limit(Limit::Max(1))
        .search();
    match found {
        Ok(items) => Some(items.first().map_or(KeychainItem::Absent, |item| {
            KeychainItem::Present(
                item.simplify_dict()
                    .and_then(|attributes| attributes.get(MODIFIED).cloned()),
            )
        })),
        Err(error) if error.code() == ITEM_NOT_FOUND => Some(KeychainItem::Absent),
        Err(_) => None,
    }
}

/// Off macOS what the keychain holds is not known from here.
#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_item(_service: &str, _account: &str) -> Option<KeychainItem> {
    None
}

#[cfg(test)]
mod tests {
    /// A cargo test binary is ad-hoc linker-signed: on macOS the running
    /// code itself says it keeps no grant (the very check a dev build makes).
    #[cfg(target_os = "macos")]
    #[test]
    fn this_ad_hoc_test_binary_keeps_no_keychain_grant() {
        assert!(!super::grant_can_stick());
    }

    /// Off macOS nothing binds a grant to the binary: the answer is yes.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos_the_grant_question_never_takes_the_keychain_away() {
        assert!(super::grant_can_stick());
    }
}
