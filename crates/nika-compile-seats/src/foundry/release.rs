// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Foundry knowledge releases as their producer writes them and a consumer judges them: the byte
//! contract ([`canonical`]: strict JSON within its bounds, the canonical row text and digest) and
//! the text and path grammar ([`grammar`]) every profile shares. Pure over bytes a door already
//! collected: the knowledge door (`nika_onboard::knowledge`) collects a payload on held
//! descriptors or from memory, names the trusted identity and keeps each profile's own rules.

pub mod canonical;
pub mod grammar;
