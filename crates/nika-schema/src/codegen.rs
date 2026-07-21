// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON-Schema codegen primitives — DESCENDED to [`nika_catalog::codegen`]
//! (C2 flag-day · the 15k prod-LOC wall: the derivation reads
//! `nika_catalog::all_builtins`, so the catalog is its natural home). This
//! shim keeps the `nika_schema::codegen` path (and the crate-root
//! re-export) byte-stable for consumers — zero surface change.

pub use nika_catalog::codegen::nika_builtin_tool_enum_schema;
