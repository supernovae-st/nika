# Crate spec — `nika-ocr`

| | |
|---|---|
| Status | **WIP** (Phase 2 M2.2 · second L1 effect crate · B.3 real `ocrs` inference wired · pre-admission · B.4 = mutation + 12-gate close) |
| Layer | L1 — effect implementation · async · `Send + Sync` · depends only on L0 / L0.5 |
| Sub-tier | L1-effect — OCR text extraction behind the L0.5 `OcrEngine` trait. Pure-Rust backend (`ocrs`) so the crate is `unsafe_code = forbid`-clean |
| Design | Thin adapter over **ocrs 0.12.2** (pure-Rust ML OCR · `rten` runtime · MIT/Apache · zero C system dep). Sync `ocrs` inference runs inside `tokio::task::spawn_blocking` (kernel CANCEL SAFETY contract · same pattern as `nika-screen`/`xcap`). RGBA8 `Frame` → RGB `ImageSource` → `prepare_input` → `detect_words` → `recognize_text` → `Vec<TextRegion>` |
| LOC budget | ≤1,200 src |
| File cap | ≤1,500 LOC each · Function cap ≤100 lines |
| Crate version | tracks workspace (`0.80.0`) · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | ADR-003 (12-gate admission) · ADR-081 (7 L1 security guards forever · nika-ocr owns **none** mandatory · inherits the contract template from nika-screen M2.1) |
| Error range | **NIKA-1100..1199** (per ADR-081 `nika_codes` matrix · **supersedes** the stale `io/ocr.rs` doc-comment "NIKA-1020..1039" which predates ADR-081 · reconciled here) |
| Reference | [`ocrs`](https://docs.rs/ocrs/0.12.2) (MIT/Apache) · `nika-kernel::io::ocr` (L0.5 sealed `OcrEngine` trait + `TextRegion` DTO) |

---

## 1. Purpose

`nika-ocr` is the **second computer-use L1 effect crate** (M2.2 · after
`nika-screen` M2.1). It implements the L0.5 `nika_kernel::io::ocr::OcrEngine`
trait — `read(&Frame)` + `read_region(&Frame, Rect)` — extracting `TextRegion`
records (text · bbox · confidence · BCP-47 language) from a captured RGBA8
frame.

The OCR inference is delegated to **`ocrs`** (pure-Rust ML · `rten` runtime),
so `nika-ocr` itself contains **zero `unsafe`** and honours `unsafe_code =
"forbid"` — the same sovereign / no-C-system-dep posture `nika-screen` got from
`xcap`. No cloud, no Tesseract C lib, no system package. This is the ADR-081
"OS-native / local-first when available" path.

`nika-ocr` consumes the `Frame` + `Rect` DTOs from `nika-screen` (M2.1 · single
canonical geometry type per `no-legacy-no-back-compat.md` Class 1), so the
capture → OCR pipeline shares one pixel-coordinate space.

## 2. Public API

```rust
//! `nika-ocr` · OCR text-extraction L1 effect crate.

/// Pure-Rust OCR backend (driven by `ocrs`). Loads detection + recognition
/// `.rten` models from a caller-provided path (sovereignty · §4) and runs
/// inference in `spawn_blocking`.
#[non_exhaustive]
pub struct OcrBackend { /* engine: Arc<ocrs::OcrEngine> | models path */ }

impl OcrBackend {
    /// Construct from explicit local model paths (detection + recognition).
    /// Errors NIKA-1101/1102 if a model file is missing / fails to load.
    pub fn with_models(detection: &Path, recognition: &Path) -> Result<Self, OcrError>;
}

impl nika_kernel::io::ocr::OcrEngine for OcrBackend {
    async fn read(&self, frame: &Frame) -> io::Result<Vec<TextRegion>>;
    async fn read_region(&self, frame: &Frame, region: Rect) -> io::Result<Vec<TextRegion>>;
}

/// Errors · NIKA-1101..1109 · #[non_exhaustive] + code() + is_transient().
/// NIKA-1100 = retired B.2 BackendNotWired placeholder slot (reserved).
#[non_exhaustive]
pub enum OcrError { /* ModelNotFound(1101) .. TaskJoinFailed(1109) */ }
```

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait (`OcrEngine`). Depends only on
  `nika-kernel` (L0.5) + permissive externals (`ocrs`, `rten` transitively,
  `bytes`, `tokio` rt+sync, `thiserror`, `miette`).
- `tokio` layer-legal at L1 (deny.toml wrappers allowlist · add `nika-ocr`) ·
  used for `spawn_blocking` only (sync `ocrs` inference).
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.

## 4. Model distribution — sovereignty decision (Rule 1 · RESOLVED B.3)

`ocrs` needs two `.rten` model files (text detection + recognition · ~few MB).
**Canonical posture (sovereignty · telemetry-canon §0 · zero cloud) — LOCKED B.3:**

- `OcrBackend::with_models(detection, recognition)` takes **explicit local
  paths** — the crate reads local files only and NEVER auto-downloads from the
  network at runtime (verified: the only I/O is `Path::exists` + `Model::load_file`).
- The cockpit / operator provisions the models once into a local cache
  (`~/.olympus/cache/ocr/` when the daemon ships · or a configured path) ·
  daemon-domain concern per `vendor-agnostic-architecture.md` Mandate 1 (same
  pattern as nika-screen consent persistence deferred to the Olympus side).
- A `with_models_from_cache()` convenience (reads the canonical cache path) is
  a later-batch convenience, **gated** on the daemon model-provisioning story
  (LOCK-031 spirit · no engine-side cache-path infra until the daemon owns it).

> **RESOLVED B.3** · the engine crate takes explicit paths only · model
> *provisioning* (bundle vs first-run fetch-with-consent vs operator-manual)
> stays a daemon-domain decision deferred to the Olympus side. The sovereign
> default — explicit operator/daemon-provisioned local paths, zero
> auto-download — is the one implemented.

## 5. Batch plan (skeleton-option-A · per nika-screen precedent)

- **B.1** spec (this file) ✅
- **B.2** crate skeleton + `OcrError` NIKA-1100..1109 + `OcrBackend` with
  `BackendNotWired` placeholder for `read`/`read_region` + pure frame-bounds
  validation (`validate_region` reused-shape) + headless tests.
- **B.3** wire `ocrs` real inference (RGBA→RGB `ImageSource` · `prepare_input`
  · `detect_words` · `find_text_lines` · `recognize_text` · `TextLine` →
  `TextRegion` via pure `words_bbox_union`) · `with_models` real `.rten` load
  (NIKA-1101/1102/1103 error paths) · closed the `BackendNotWired` skeleton
  (NIKA-1100 retired) · pure helpers `rgba_to_rgb` + `crop_rgba` +
  `words_bbox_union` extracted + proptested · 17 lib tests · clippy/doc/deny
  green. ✅
- **B.4** mutation run (`cargo mutants -p nika-ocr`) → ≥90 % on the pure
  surface + documented Rule-2 exemption for the `run_ocr` inference residue +
  ADR-003 canonical 12-gate close + 3-agent review swarm + admission commit.

## 6. Gate-5 mutation posture (forward note)

Like `nika-screen`, the real OCR inference path is model-dependent (needs the
`.rten` models + a real frame) → covered by `#[ignore]` smoke tests, not
headless CI. Headless-reachable logic (error codes · frame-bounds validation ·
RGBA→input conversion · `TextLine`→`TextRegion` mapping) targets 100 % mutation
kill; the `ocrs` inference residue is documented-exempt per ADR-003 Rule 2.

## 7. Security (ADR-081)

`nika-ocr` owns **no** MANDATORY-at-admission guard (the 5 mandatory guards
belong to input/a11y/browser/vision-local per ADR-081 §matrix). It inherits the
7-guard contract template + the structural admission discipline. OCR is
read-only inference over an already-captured frame (the consent/LED guards are
enforced upstream at `nika-screen` capture time).
