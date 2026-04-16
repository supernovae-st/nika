// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Message role — shared between provider and checkpoint types.

use serde::{Deserialize, Serialize};

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Role {
    /// System instructions.
    System,
    /// User input.
    User,
    /// Assistant response.
    Assistant,
    /// Tool result.
    Tool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_serde_roundtrip() {
        let role = Role::Assistant;
        let json = serde_json::to_string(&role).expect("serialize");
        assert_eq!(json, "\"assistant\"");
        let back: Role = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, role);
    }

    #[test]
    fn role_all_variants_serde() {
        for (role, expected) in [
            (Role::System, "\"system\""),
            (Role::User, "\"user\""),
            (Role::Assistant, "\"assistant\""),
            (Role::Tool, "\"tool\""),
        ] {
            let json = serde_json::to_string(&role).expect("serialize");
            assert_eq!(json, expected);
            let back: Role = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, role);
        }
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn role_is_send_sync() {
        _assert_send_sync::<Role>();
    }
}
