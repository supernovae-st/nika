// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one instruction the explicitly permitted seat reads before it proposes a
//! private semantic plan: the closed vocabulary of steps, effects, obligations and
//! regions, in the request's own words. Text only, no logic; the decoder in
//! [`super`] owns what is accepted.

pub(super) const INSTRUCTIONS: &str =
    include_str!("../../assets/plan_instructions.md").trim_ascii_end();

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    /// The asset renders the very bytes the literal held: the seat reads the same instruction.
    #[test]
    fn the_instruction_renders_its_exact_bytes() {
        let digest = format!("{:x}", Sha256::digest(super::INSTRUCTIONS.as_bytes()));
        assert_eq!(
            digest,
            "91292ccdf383d9576277825a23ce5d461a70b3f8a1812059994ad2611d0f7015"
        );
        assert_eq!(super::INSTRUCTIONS.len(), 7211);
    }
}
