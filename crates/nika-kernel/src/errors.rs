// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cross-domain NIKA-NNN range REGISTRY for kernel error types.
//!
//! Since the 4-way kernel split
//! (`docs/architecture/kernel-split-census-2026-06-10.md`) the
//! per-type `NikaErrorCode` impls + constants live in the sibling that
//! OWNS each type (orphan rule — `NikaErrorCode` is a foreign trait
//! from `nika-error`). This module is the one place the full range
//! allocation stays readable, and re-exports every sibling constant so
//! `nika_kernel::errors::NIKA_050` remains a valid path.
//!
//! Error code ranges:
//! - 050–099: Shell/exec (`ShellError`) — `nika-kernel-core`
//! - 100–139: File/IO — blob 100–109 (`BlobError`) · fs 110–119 (`FsError`) — `nika-kernel-core`
//! - 140–189: Http/network (`HttpError`) — `nika-kernel-core`
//! - 230–279: MCP/tools (`ToolExecError`) — `nika-kernel-runtime`
//! - 330–379: Provider (`ProviderError`) — `nika-kernel-ai` (moved from
//!   380-429 2026-05-11 to free the Shield-reserved 380-389 slot per
//!   `dx/.claude/rules/security.md` §"Nika Shield" canonical mapping.
//!   Historical `NIKA_380..389` names dropped per
//!   `no-legacy-no-back-compat.md` · zero alias · git is archive.)
//! - 380–429: Shield (`ShieldError`) — RESERVED, per `nika/SECURITY.md`
//!   6-layer defense stack. `NIKA-380` `CapabilityDenied` · `NIKA-381`
//!   `TrustViolation` · `NIKA-382` `CanaryLeaked` · `NIKA-383`
//!   `InjectionDetected` · `NIKA-384` `SpotlightRequired` · `NIKA-385`
//!   `MlModelMissing` · `NIKA-386` `RunDepthExceeded` · `NIKA-387`
//!   `RunCycleDetected` · `NIKA-388` `CanaryInThinking` · `NIKA-389`
//!   `UntrustedVisionBlocked`. Crate not yet admitted — slot LOCKED.
//! - 600–649: Memory (`MemoryError`) — impl co-located in
//!   `nika-kernel-ai` `memory.rs` · constants in
//!   `nika_error::codes::NIKA_601..604` (the kernel-local `NIKA_600..603`
//!   placeholders were DRIFT · removed per Diamond W2.2 /
//!   `no-legacy-no-back-compat.md`).
//! - 700–749: WASM plugin (`WasmPluginError`) — impl in
//!   `nika-kernel-plugin` `errors.rs` · constant `nika_error::codes::NIKA_700`
//! - 750–799: Sandbox (`SandboxError`) — impl in `nika-kernel-plugin`
//!   `errors.rs` · constant `nika_error::codes::NIKA_750`
//! - 800–819: Observability range (reserved for v0.100 `OTel` adapter; no
//!   kernel type maps to it today — `ObservabilitySink` was dropped per Q12)
//! - 1000–1099: Screen capture (`ScreenError`) — `nika-kernel-core` `io/screen`
//!   (constants in `nika_error::codes::NIKA_1000..1009` · ADR-081 computer-use)
//! - 1100–1199: OCR (`OcrError`) — `nika-kernel-core` `io/ocr`
//!   (constants in `nika_error::codes::NIKA_1101..1109`)
//! - 1200–1299: Accessibility (`A11yError`) — `nika-kernel-core` `io/a11y`
//!   (constants in `nika_error::codes::NIKA_1201..1206`)
//! - 1300–1399: Synthetic input (`InputError`) — `nika-kernel-core` `io/input`
//!   (constants in `nika_error::codes::NIKA_1301..1305` · M2.4 nika-input)
//! - 1400–1499: Browser automation (`BrowserError`) — `nika-kernel-core`
//!   `io/browser` (constants in `nika_error::codes::NIKA_1401..1406`)

pub use nika_kernel_ai::errors::*;
pub use nika_kernel_core::errors::*;
pub use nika_kernel_runtime::errors::*;
// `nika-kernel-plugin` errors carry impls only (constants live in
// `nika_error::codes`) — nothing to re-export.
