// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-store` — the signed-write / verified-recall memory substrate
//! (F-P8 · SMSR arXiv:2606.12703 · TMA-NM arXiv:2606.24322 · `MemLineage`
//! arXiv:2605.14421 · ADR-030 · ADR-031 · ADR-078).
//!
//! The law: every write is SIGNED with the run-signing key (the engine =
//! TCB · the LLM never touches the key), the provenance label INSIDE the
//! signature; every recall VERIFIES per entry — unsigned · bad signature ·
//! rewritten label = REJECTED, never filtered (the SMSR
//! no-provenance-free-filter theorem: unsigned admission floor = 0 %).
//! Rejections are named ([`RejectReason`]), journaled
//! (`memory_entry_rejected`), and counted in the seal's `covers["memory"]`
//! beside the admitted-set digests ([`seal_fold`]). The envelope and the
//! fold carry `"v": 1` (the envelope's INSIDE the signed preimage —
//! FCI-003/011/022 · decode dispatches on it, an unknown version
//! rejecting [`RejectReason::UnsupportedVersion`]).
//!
//! Named deviations (`docs/crate-specs/nika-store.md` §2): canonical bytes
//! = JCS (the engine's ONE canonicalization voice — the `serde_json`
//! sorted-keys/compact/raw-UTF-8 idiom `nika_proof::canonical` pins), not
//! the law text's CBOR · the label is [`Integrity`](nika_cap::Integrity)
//! (the live F-O1 two-point lattice · join = Untrusted-dominates), not the
//! law's 4-level one · the key is INJECTED (`&minisign::SecretKey` /
//! `&minisign::PublicKey`), custody stays with the L4 caller. v2 owes:
//! `HashDomain::Memory` + the Python goldens · the δ(t,m,k,n) certificate ·
//! the `state.write[store]` YAML surface · a dedicated memory keynum · the
//! L2 `nika-memory` orchestrator (v1 recall is list+filter).

mod dir;
mod entry;
mod errors;
mod sign;
#[cfg(test)]
mod tests;
mod traits;

pub use dir::{MEMORY_ROOT, entry_file_name, store_dir};
pub use entry::{StoreEntry, UnsignedEntry};
pub use errors::StoreError;
pub use sign::{
    EntryVerdict, FoldRejection, RecallVerdict, RejectReason, recall_verified, remember_signed,
    seal_fold, seal_fold_detailed,
};
pub use traits::SignedMemoryStore;
