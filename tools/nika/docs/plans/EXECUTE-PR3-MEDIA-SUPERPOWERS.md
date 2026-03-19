# PR3: Media Superpowers — Autonomous Execution Brief

> Copie ce fichier comme prompt dans un nouveau terminal Claude.
> Il contient TOUT pour executer PR3a (14 commits) + PR3b (12 commits) en autonomie totale.
> Session estimee: 5 heures. Lis TOUT avant de coder.

---

## Identite

Tu es un ingenieur Rust senior qui implemente les media builtin tools de Nika — un workflow engine YAML pour tasks AI. Tu travailles en **autonomie totale** avec des quality gates strictes entre chaque phase.

Tu as acces a des superpowers (skills). **Utilise-les.** Avant chaque tache, verifie si un skill s'applique.

---

## Contexte Initial — LANCE DES AGENTS D'EXPLORATION

**AVANT DE CODER**, lance ces agents en parallele pour comprendre l'etat actuel :

```
Agent 1 (Explore): "Very thorough exploration of src/runtime/builtin/ — understand the BuiltinTool trait,
  BuiltinToolRouter dispatch, FileToolAdapter pattern, and how invoke: calls builtins in executor/verbs.rs"

Agent 2 (Explore): "Very thorough exploration of src/media/ — understand MediaProcessor, CasStore,
  MediaRef, MediaBudget, detect.rs, error.rs, and the full processing pipeline"

Agent 3 (Explore): "Quick exploration of Cargo.toml — list all current dependencies and features"

Agent 4 (Explore): "Quick exploration of src/error.rs — find all NIKA-XXX error codes,
  verify NIKA-290..299 range is available"
```

Attends les resultats. Prends des notes mentales sur les types exacts, les signatures, et les patterns.

---

## Baseline

- **Nika**: v0.32.0 (apres PR2 merged)
- **Schema**: @0.12 — ne PAS bumper
- **Tests**: ~5,600+ pass, 0 fail (`cargo test --lib`)
- **Clippy**: zero nouveau warning (`cargo clippy -- -D warnings`)
- **Branch main**: apres merge de `feat/media-artifacts` (PR2)

Verifie la baseline AVANT de commencer :
```bash
cargo test --lib 2>&1 | tail -5        # Nombre de tests, 0 fail
cargo clippy -- -D warnings 2>&1 | tail -3  # Zero warnings
```

---

## Fichiers Plan (LIS DANS CET ORDRE)

```
docs/plans/2026-03-19-media-superpowers-master-plan.md    ← LE PLAN PRINCIPAL (v2.0)
docs/plans/2026-03-18-media-pipeline-master-plan.md       ← Architecture originale, 29 decisions
docs/plans/2026-03-18-media-pipeline-innovations-research.md ← Competitive intel
```

Le master plan v2.0 contient TOUT : architecture, naming, securite, async, performance, tests, error codes, directory structure, code exact pour chaque commit.

### Fichiers de recherche (reference, ne pas lire en entier)

```
docs/research/2026-03-18-media-pipeline-crate-survey.md         ← Survey 50+ crates
docs/research/2026-03-18-crate-api-reference.md                 ← APIs: c2pa, thumbhash, symphonia, ocrs, lofty
docs/research/2026-03-18-media-crate-api-reference.md           ← APIs: fast_image_resize, resvg, oxipng, palette
docs/research/2026-03-18-rust-media-crate-apis.md               ← APIs: color-thief, calamine, cosmic-text, xcap
docs/research/2026-03-18-bleeding-edge-rust-media-projects.md   ← OxiMedia, lele, Extism, kornia
```

Consulte ces fichiers de recherche UNIQUEMENT quand tu as besoin de l'API detaillee d'un crate specifique.

---

## Architecture Cle

```
invoke: nika:thumbnail
  │
  BuiltinToolRouter.dispatch()
    │  extract_name("nika:thumbnail") → "thumbnail"
    │  tools.get("thumbnail") → Arc<dyn BuiltinTool>
    ▼
  MediaToolAdapter (impl BuiltinTool)
    │  1. Parse JSON args
    │  2. Timeout wrap (30s default)
    │  3. Delegate to MediaOp::execute(args, ctx)
    ▼
  ThumbnailOp (impl MediaOp)
    │  ctx.read_media(hash)           ← CAS read (async)
    │  ctx.compute.compute(closure)   ← ComputePool (rayon, 4 threads)
    │  return MediaOpResult::Binary { data, mime, metadata }
    ▼
  MediaToolAdapter (continued)
    │  4. ctx.store_media(data)       ← CAS write + budget check
    │  5. Serialize MediaRef + metadata → JSON String
    ▼
  Result<String, NikaError>           → back through router → workflow output
```

### Types fondamentaux (a creer dans Phase A)

```rust
// src/runtime/builtin/media/mod.rs

/// Internal trait for media operations.
pub(crate) trait MediaOp: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn execute<'a>(
        &'a self, args: serde_json::Value, ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>>;
}

pub(crate) enum MediaOpResult {
    Metadata(Value),
    Binary { data: Vec<u8>, mime_type: String, extension: String, metadata: Value },
    MultiBinary(Vec<BinaryOutput>),
}

// src/runtime/builtin/media/context.rs

pub struct MediaToolContext {
    pub cas: CasStore,
    pub budget: Arc<MediaBudget>,
    pub compute: Arc<ComputePool>,
    pub decode_semaphore: Arc<Semaphore>,       // max 4 concurrent decodes
    pub working_memory: Arc<WorkingMemoryBudget>, // 512MB transient
    pub cancel: CancellationToken,
}
```

---

## Naming Convention

**Flat verbs, prefix `nika:`** — meme pattern que `nika:sleep`, `nika:read`.

```
nika:thumbnail      nika:metadata       nika:optimize
nika:ocr            nika:chart          nika:qr_validate
nika:svg_render     nika:pdf_extract    nika:thumbhash
nika:dominant_color nika:dimensions     nika:phash
nika:provenance     nika:convert        nika:strip
nika:compare        nika:pipeline
```

Le router lookup: `extract_name("nika:thumbnail")` → `"thumbnail"` → `tools.get("thumbnail")`.

---

## Securite — REGLES NON NEGOCIABLES

### S1: JAMAIS appeler `image::load_from_memory()` directement

Utilise TOUJOURS `decode_image_safe()` (a creer dans `safety.rs`) :

```rust
pub fn decode_image_safe(data: &[u8]) -> Result<DynamicImage, NikaError> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);  // 256 MB
    limits.max_image_width = Some(10_000);
    limits.max_image_height = Some(10_000);
    reader.with_limits(limits).decode().map_err(|e| /* NIKA-290 */)
}
```

**Pourquoi** : un PNG de 1x1 peut decompresser a 16 GB. Sans limits, OOM garanti.

### S2: SVG — sanitize AVANT rendu

```rust
pub fn sanitize_svg(input: &str) -> Result<&str, NikaError> {
    let lower = input.to_ascii_lowercase();
    for pattern in ["<script", "<foreignobject", "javascript:"] {
        if lower.contains(pattern) { return Err(/* NIKA-297 */); }
    }
    // Event handlers
    if regex_is_match(r"\bon\w+\s*=", &lower) { return Err(/* NIKA-297 */); }
    Ok(input)
}
```

### S3: PDF — extraction dans un thread avec stack limitee

```rust
pub fn extract_pdf_safe(data: &[u8]) -> Result<String, NikaError> {
    let data = data.to_vec();
    let handle = std::thread::Builder::new()
        .stack_size(4 * 1024 * 1024)
        .spawn(move || pdf_extract::extract_text_from_mem(&data))?;
    match handle.join() {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(/* NIKA-290 */),
        Err(_) => Err(/* NIKA-290 panicked */),
    }
}
```

### S4: Timeout sur TOUT

Chaque tool wrape son execute dans `tokio::time::timeout(Duration::from_secs(30), ...)`.

### S5: Fuzz — aucun tool ne doit JAMAIS paniquer

Chaque tool a un test fuzz (100 iterations de bytes random). Utilise `std::panic::catch_unwind` comme filet.

---

## Error Codes

| Code | Variant | Usage |
|------|---------|-------|
| NIKA-290 | MediaToolError | Generic tool failure |
| NIKA-291 | MediaToolUnsupportedFormat | Wrong format for tool |
| NIKA-292 | MediaToolDependencyMissing | Feature disabled |
| NIKA-293 | MediaToolTimeout | Timeout exceeded |
| NIKA-294 | MediaToolInvalidArgs | Bad parameters |
| NIKA-295 | MediaPipelineStepFailed | Pipeline step N failed |
| NIKA-296 | MediaPipelineEmpty | No steps |
| NIKA-297 | MediaSecurityViolation | SVG XSS, bomb, etc. |

---

## Protocole Par Commit

Pour CHAQUE commit, suis ce cycle exact :

### 1. LIRE le plan

Ouvre `2026-03-19-media-superpowers-master-plan.md`. Lis la section du commit en cours.

### 2. TDD RED — Ecrire le test d'abord

```bash
# Ecris le test qui DOIT FAIL
cargo test --lib <test_name> 2>&1 | grep "FAILED"
```

### 3. IMPLEMENT — Code minimal

Suis le code du plan. Pour les APIs des crates externes, consulte les fichiers `docs/research/2026-03-18-*-api-reference.md`.

### 4. VERIFY — Triple check

```bash
cargo check                     # Compilation
cargo test --lib                # TOUS les tests
cargo clippy -- -D warnings     # Zero nouveau warning
```

Si un seul fail → FIX avant de continuer. **JAMAIS de commit avec un test qui fail.**

### 5. SELF-REVIEW

```bash
git diff --staged
```

Checklist mentale :
- [ ] Imports necessaires ?
- [ ] Pas de `unwrap()` sur donnees utilisateur ?
- [ ] `decode_image_safe()` utilise (jamais `load_from_memory`) ?
- [ ] Timeout wrappe autour de l'execute ?
- [ ] Tests couvrent: happy path + error + edge + security ?
- [ ] Pas de path traversal possible ?
- [ ] Budget rollback si echec apres charge ?

### 6. COMMIT

```bash
git add <fichiers specifiques>
git commit -m "$(cat <<'EOF'
type(scope): description

- Detail 1
- Detail 2

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## PR3a — feat/media-superpowers (v0.33.0)

### Phase A: Infrastructure (Commits 1-3)

#### C1: `feat(media): media tool dispatcher + MediaOp trait`

**Fichiers a creer :**
- `src/runtime/builtin/media/mod.rs` — `MediaOp` trait, `MediaOpResult` enum, `MediaToolAdapter`, `create_media_tool_adapters()`
- `src/runtime/builtin/media/context.rs` — `MediaToolContext`, `ComputePool`, `WorkingMemoryBudget`
- `src/runtime/builtin/media/safety.rs` — `decode_image_safe()`, `sanitize_svg()`, `extract_pdf_safe()`
- `src/runtime/builtin/media/error.rs` — NIKA-290..297 helper functions

**Fichiers a modifier :**
- `src/runtime/builtin/mod.rs` — ajouter `#[cfg(feature = "media-core")] pub(crate) mod media;`
- `src/error.rs` — ajouter les variants NIKA-290..297

**Pattern a suivre** : regarde `src/runtime/builtin/file_adapter.rs` — c'est EXACTEMENT le meme pattern (bridge un trait interne vers `BuiltinTool`).

**Tests (5):**
- `media_op_trait_compiles` — dummy impl de MediaOp
- `media_tool_adapter_dispatches` — adapter appelle execute
- `media_op_result_metadata_serializes` — Metadata variant → JSON
- `media_op_result_binary_stores_in_cas` — Binary variant → CAS
- `compute_pool_runs_closure` — ComputePool executes on rayon

**QUALITY GATE**: `cargo check && cargo test --lib media && cargo clippy -- -D warnings`

---

#### C2: `feat(media): ComputePool + WorkingMemoryBudget`

Le `ComputePool` isole rayon de tokio. Le `WorkingMemoryBudget` empeche les OOM.

```rust
// ComputePool: 4 rayon threads, isolated from tokio
pub struct ComputePool {
    pool: rayon::ThreadPool,
}

impl ComputePool {
    pub fn new() -> Self { /* ThreadPoolBuilder, num_threads: min(num_cpus, 4) */ }
    pub async fn compute<F, T>(&self, f: F) -> Result<T, NikaError>
    where F: FnOnce() -> T + Send + 'static, T: Send + 'static { /* oneshot channel */ }
}

// WorkingMemoryBudget: 512MB default, acquire/release guards
pub struct WorkingMemoryBudget { /* AtomicUsize + Notify */ }
pub struct WorkingMemoryGuard<'a> { /* Drop releases memory */ }
```

**Tests (4):**
- `compute_pool_executes_on_rayon_thread` — verify thread name starts with "nika-media"
- `compute_pool_handles_panic` — closure panics → error, not crash
- `working_memory_acquire_release` — acquire 100MB, release, verify counter
- `working_memory_blocks_when_full` — acquire 512MB, second acquire blocks

---

#### C3: `feat(media): wire media tools into BuiltinToolRouter`

**Fichiers a modifier :**
- `src/runtime/builtin/router.rs` — ajouter `with_media_tools()` ou integrer dans `full()`
- `src/runtime/executor/verbs.rs` — passer le `MediaToolContext` au router

**ATTENTION** : le `BuiltinToolRouter` est construit dans `runner.rs`. Cherche ou il est cree et ajoute le `MediaToolContext` a ce moment-la.

**Tests (3):**
- `router_registers_media_tools` — `is_builtin("nika:thumbnail")` == true
- `router_dispatches_to_media_adapter` — dispatch retourne result
- `router_unknown_media_tool_error` — `nika:nonexistent` → error avec hint features

**⏸️ QUALITY GATE A**: Full test suite + clippy clean

---

### Phase B: Tier 1 Always-On (Commits 4-6)

Ces tools n'ont ZERO ou quasi-zero deps supplementaires.

#### C4: `feat(media): nika:dimensions — image dimensions from headers`

**Crate**: `imagesize = "0.13"` (0 deps, 12M downloads)
**Fichier**: `src/runtime/builtin/media/dimensions.rs`

Lit SEULEMENT le header de l'image (pas de decode complet). ~0.1ms.

```rust
pub struct DimensionsOp;
impl MediaOp for DimensionsOp {
    fn name(&self) -> &'static str { "dimensions" }
    fn execute(...) {
        let data = ctx.read_media(&hash).await?;
        let size = imagesize::blob_size(&data).map_err(|_| /* NIKA-291 */)?;
        Ok(MediaOpResult::Metadata(json!({
            "width": size.width, "height": size.height,
            "orientation": if size.width > size.height { "landscape" }
                          else if size.width < size.height { "portrait" }
                          else { "square" }
        })))
    }
}
```

**Tests (6):**
- `dimensions_png_1x1` — width=1, height=1, orientation=square
- `dimensions_jpeg_landscape` — orientation=landscape
- `dimensions_svg_from_viewbox` — SVG viewBox → dimensions
- `dimensions_missing_hash` — NIKA-253
- `dimensions_audio_input` — NIKA-291
- `dimensions_corrupt_header_no_panic` — error, not panic

---

#### C5: `feat(media): nika:thumbhash — compact image placeholder`

**Crate**: `thumbhash = "0.1"` (0 deps, 173K downloads)
**API**: seulement 4 fonctions publiques — `rgba_to_thumb_hash()`, `thumb_hash_to_rgba()`, `thumb_hash_to_average_rgba()`, `thumb_hash_to_approximate_aspect_ratio()`.

**ATTENTION**: thumbhash attend un input max 100x100. Il faut resize l'image avant de hasher.

```rust
// Resize to max 100x100 BEFORE thumbhash
let small = img.resize(100, 100, image::imageops::FilterType::Triangle);
let rgba = small.to_rgba8();
let hash = thumbhash::rgba_to_thumb_hash(w as usize, h as usize, rgba.as_raw());
```

**Tests (5):**
- `thumbhash_png_returns_base64` — valid base64 string
- `thumbhash_deterministic` — same input → same hash
- `thumbhash_transparent_preserves_alpha` — RGBA with alpha
- `thumbhash_missing_hash` — NIKA-253
- `thumbhash_random_bytes_no_panic` — fuzz 50 iterations

---

#### C6: `feat(media): nika:dominant_color — extract color palette`

**Crates**: `color-thief = "0.2"` (460K downloads)
**API**: `color_thief::get_palette(pixels, format, quality, max_colors)`

**Tests (5):**
- `dominant_color_solid_red` — hex = "#ff0000"
- `dominant_color_count_param` — count=5 → 5 colors
- `dominant_color_1x1` — single pixel → 1 color
- `dominant_color_missing_hash` — NIKA-253
- `dominant_color_random_no_panic` — fuzz 50 iterations

---

### Phase C: Tier 2 Default Features (Commits 7-12)

#### C7: `feat(media): nika:thumbnail — SIMD-accelerated image resize`

**Crates**: `fast_image_resize = { version = "6.0", features = ["image"] }` + `image = "0.25"`
**Feature**: `media-thumbnail`

**ATTENTION — Gotchas importants** (de la recherche Context7):
1. **RGBA alpha**: MUST premultiply avec `MulDiv` avant resize, puis divide apres. Sinon color bleed.
2. **sRGB**: Le resizer ne convertit PAS en linear colorspace. Pour sRGB correct, utilise `create_srgb_mapper()`.
3. **SIMD auto-detected**: AVX2/SSE4.1/Neon — pas de config necessaire.
4. **Feature flag `"image"`**: active l'interop avec `DynamicImage`.

**Pattern async**:
```rust
let result = ctx.compute.compute(move || {
    let img = decode_image_safe(&data)?;  // JAMAIS load_from_memory !
    // ... resize avec fast_image_resize ...
    // ... encode en PNG/JPEG/WebP ...
    Ok(output_bytes)
}).await?;
```

**Tests (8):**
- `thumbnail_png_to_100x100` — verify output dimensions
- `thumbnail_jpeg_preserve_format` — input JPEG → output JPEG
- `thumbnail_webp_output` — format: webp works
- `thumbnail_rgba_alpha_handling` — transparent PNG → no color bleed
- `thumbnail_zero_width_rejected` — NIKA-294
- `thumbnail_decompression_bomb` — 65535x65535 PNG → NIKA-297
- `thumbnail_missing_hash` — NIKA-253
- `thumbnail_random_bytes_no_panic` — fuzz 100 iterations

---

#### C8: `feat(media): nika:metadata — universal media metadata extraction`

**Crates**: `nom-exif = "2.7"` + `lofty = "0.23"` + `mp4 = "0.14"`
**Feature**: `media-metadata`

Route sur le MIME type detecte par `infer` :
- `image/*` → `nom-exif` (EXIF, GPS, camera, orientation)
- `audio/*` → `lofty` (ID3, title, artist, duration, bitrate)
- `video/*` → `mp4` (duration, tracks, codec, resolution)

**API lofty** (de la recherche): `lofty::read_from(&mut Cursor::new(data))` → `tagged.primary_tag()` → `.title()`, `.artist()`, `.album()`.
**API nom-exif**: `MediaParser::new()` → `parser.parse(MediaSource::seekable_buf(data))` → `Exif` → `.get(ExifTag::Make)`.

**Tests (10):**
- `metadata_png_returns_dimensions`
- `metadata_jpeg_returns_exif_fields` — camera, orientation
- `metadata_jpeg_gps_coordinates` — lat/lon extraction
- `metadata_wav_returns_audio_info` — duration, sample_rate, channels
- `metadata_mp3_returns_tags` — title, artist, album
- `metadata_mp4_returns_video_info` — duration, tracks
- `metadata_svg_from_viewbox` — width/height
- `metadata_unknown_format_minimal` — returns { mime_type, size }
- `metadata_corrupt_exif_no_panic` — invalid EXIF → fields absent
- `metadata_random_bytes_no_panic` — fuzz 50 iterations

---

#### C9: `feat(media): nika:optimize — lossless PNG optimization`

**Crate**: `oxipng = { version = "10.1", features = ["parallel"] }`
**Feature**: `media-optimize`

**API**: `oxipng::optimize_from_memory(&data, &Options::from_preset(level))` → `Result<Vec<u8>>`.

**ATTENTION**:
- Level 2 = default (bon compromis 100-500ms).
- Level 6 = zopfli, peut prendre 15s sur gros fichier → timeout 30s.
- `opts.strip = oxipng::StripChunks::Safe` pour retirer metadata non-essentiels.
- MUST run dans `spawn_blocking` ou `compute.compute()` (CPU-bound).

**Tests (7):**
- `optimize_png_reduces_size` — output ≤ input
- `optimize_png_output_valid` — PNG magic bytes
- `optimize_jpeg_input_rejected` — NIKA-291
- `optimize_already_optimal` — re-optimize → savings ~0%
- `optimize_strip_metadata` — strip: true removes chunks
- `optimize_corrupt_png_no_panic` — error, not crash
- `optimize_completes_under_30s` — timeout test

---

#### C10: `feat(media): nika:svg_render — SVG to PNG rasterization`

**Crates**: `resvg = "0.47"` + `usvg = "0.47"` + `tiny-skia = "0.12"`
**Feature**: `media-svg`

**API**: `usvg::Tree::from_str(&svg, &Options)` → `resvg::render(&tree, transform, &mut pixmap)` → `pixmap.encode_png()`.

**SECURITE CRITIQUE** — SVG est le tool le plus risque :
1. Appeler `sanitize_svg()` AVANT le parsing usvg
2. `usvg::Options { resources_dir: None }` — JAMAIS de resources externes
3. Tests SVG XSS obligatoires

**Fontdb** : `fontdb::Database` est lent au premier appel (scan systeme fonts). Utiliser `OnceLock` dans `MediaToolContext` pour lazy init.

**Tests (10):**
- `svg_render_basic_shape` — rect → PNG correct
- `svg_render_custom_dimensions` — width/height override
- `svg_render_text_elements` — text rasterize
- `svg_render_invalid_svg_error` — malformed XML → error
- `svg_render_script_tag_REJECTED` — CRITICAL: `<script>` → NIKA-297
- `svg_render_foreignObject_REJECTED` — CRITICAL
- `svg_render_xxe_entity_BLOCKED` — CRITICAL: DOCTYPE entity
- `svg_render_xlink_external_BLOCKED` — CRITICAL: SSRF
- `svg_render_event_handler_REJECTED` — onload= → NIKA-297
- `svg_render_random_xml_no_panic` — fuzz 50 iterations

---

#### C11: `feat(media): nika:convert — format conversion`

**Crate**: `image` (deja present pour thumbnail)

PNG → JPEG, JPEG → PNG, PNG → WebP, etc. via `image::DynamicImage::write_to()`.

**ATTENTION**: PNG transparent → JPEG = perte alpha. Composite sur fond blanc.

**Tests (6):**
- `convert_png_to_jpeg` — verify JPEG magic bytes
- `convert_jpeg_to_png` — verify PNG magic bytes
- `convert_transparent_png_to_jpeg` — white background
- `convert_same_format_noop` — PNG → PNG → identical hash (CAS dedup)
- `convert_audio_to_png_rejected` — NIKA-291
- `convert_corrupt_input_no_panic` — fuzz

---

#### C12: `feat(media): nika:strip — remove metadata`

Decode → re-encode (strips all EXIF, GPS, camera info).

```rust
let img = decode_image_safe(&data)?;
let mut buf = Vec::new();
img.write_to(&mut Cursor::new(&mut buf), format)?;  // Re-encode = strip all metadata
```

**Tests (5):**
- `strip_jpeg_removes_exif` — output has no EXIF
- `strip_gps_coordinates_removed` — privacy-critical
- `strip_output_renders_identical` — phash distance = 0
- `strip_no_metadata_noop` — clean image → CAS dedup hit
- `strip_preserves_image_quality` — visual identical

---

### Phase D: Auto-Enrichment (C13)

#### C13: `feat(media): auto-enrich MediaRef with dimensions + thumbhash on CAS store`

**Fichier**: `src/media/processor.rs` — apres `store.store()`, enrichir `MediaRef.metadata`.

Ajouter un champ `metadata: serde_json::Map<String, Value>` a `MediaRef`.

```rust
// Apres CAS store, si image:
if detected.mime_type.starts_with("image/") {
    if let Ok(size) = imagesize::blob_size(&decoded) {
        metadata.insert("width".into(), json!(size.width));
        metadata.insert("height".into(), json!(size.height));
    }
    // ThumbHash (skip >10MB)
    if decoded.len() < 10_000_000 {
        if let Ok(img) = decode_image_safe(&decoded) {
            let small = img.resize(100, 100, FilterType::Triangle).to_rgba8();
            let hash = thumbhash::rgba_to_thumb_hash(...);
            metadata.insert("thumbhash".into(), json!(base64_encode(&hash)));
        }
    }
}
```

**Template access**: `{{with.img.media[0].metadata.width}}`, `{{with.img.media[0].metadata.thumbhash}}`

**Tests (6):**
- `enrichment_png_has_width_height` — metadata.width, metadata.height
- `enrichment_png_has_thumbhash` — metadata.thumbhash is base64
- `enrichment_jpeg_has_dimensions` — JPEG enriched too
- `enrichment_audio_no_dimensions` — audio → metadata empty
- `enrichment_large_image_skips_thumbhash` — >10MB → no thumbhash
- `enrichment_existing_tests_unaffected` — backward compat, all 5600+ tests pass

---

### Phase E: Tests Integration (C14)

#### C14: `test(media): tool integration + security + E2E`

**Cross-tool integration (6):**
- `integration_thumbnail_then_optimize` — chain works
- `integration_metadata_on_thumbnail_output` — dimensions match
- `integration_phash_before_after_optimize` — distance = 0 (future, PR3b)
- `integration_strip_then_metadata_empty` — stripped → no EXIF
- `integration_convert_then_dimensions` — JPEG→PNG dimensions preserved
- `integration_all_tools_produce_valid_cas_entries` — CAS integrity

**CAS integrity (4):**
- `cas_tool_dedup_same_input` — same tool twice → dedup
- `cas_concurrent_tool_writes` — 10 parallel tools → no corruption
- `cas_budget_across_tools` — budget tracks across tool calls
- `cas_budget_rollback_on_failure` — budget restored on error

**E2E workflows (3):**
- `e2e_workflow_image_pipeline` — generate → thumbnail → optimize → artifact
- `e2e_workflow_metadata_extract` — image → metadata → JSON output
- `e2e_workflow_text_only_unchanged` — no media tools → backward compat

**Feature flags (2):**
- `feature_disabled_tool_returns_hint` — clear error message
- `feature_no_default_compiles` — `--no-default-features` compile clean

**⏸️ QUALITY GATE FINALE PR3a**: ALL tests pass. Clippy clean. Version bump v0.33.0.

---

## Quality Gates

Apres Phase A, B, C, D, et E, STOP et fais :

```bash
# 1. Full test suite
cargo test --lib
# Attendu: 5,600+ pass + ~150 nouveaux = ~5,750+ pass, 0 fail

# 2. Clippy clean
cargo clippy -- -D warnings
# Zero nouveau warning

# 3. Feature flag compilation
cargo check --no-default-features
# Doit compiler sans les media tools

# 4. Binary size check
cargo build --release 2>&1 | tail -3
# Note la taille du binaire pour comparaison
```

---

## Code Review Inter-Phase

Apres chaque phase, lance une review :

```
/spn-powers:requesting-code-review
```

Verifie :
- Patterns Rust idiomatiques
- Securite (path traversal, size limits, decompression bombs)
- Performance (copies inutiles, allocations, async safety)
- Coherence avec le reste du codebase Nika

---

## Skills a Utiliser

| Quand | Skill |
|-------|-------|
| Avant d'ecrire du Rust | `/spn-rust:rust` |
| Cycle TDD | `/spn-powers:test-driven-development` |
| Avant de dire "c'est fait" | `/spn-powers:verification-before-completion` |
| Pour commit | `/spn-powers:git:commit` |
| Pour review | `/spn-powers:requesting-code-review` |
| Si besoin docs crate | `/find-docs` |
| Si bug complique | `/spn-powers:systematic-debugging` |

---

## 15 Points Critiques (BUGS A EVITER)

### Securite
1. **JAMAIS `image::load_from_memory()`** → utilise `decode_image_safe()` avec Limits
2. **JAMAIS `usvg::Options { resources_dir: Some(...) }`** → toujours `None`
3. **SVG sanitize AVANT parse** → `sanitize_svg()` appele en premier
4. **PDF dans thread avec stack 4MB** → `extract_pdf_safe()`
5. **Timeout sur TOUT** → 30s default, 120s max

### Architecture
6. **MediaOp retourne MediaOpResult, pas String** → MediaToolAdapter convertit
7. **Un seul `compute()` par pipeline** → pas de spawn_blocking entre steps
8. **CancellationToken check entre steps** → `if cancel.is_cancelled() { return Err }`
9. **ComputePool = 4 threads rayon** → isole de tokio, pas de deadlock

### Types
10. **MediaRef.metadata = `serde_json::Map`** → pas `HashMap<String, Value>`, pour skip_serializing_if
11. **`imagesize::blob_size()` prend `&[u8]`** → pas de file path
12. **`thumbhash` attend max 100x100** → resize AVANT hash
13. **`color_thief::get_palette` quality 1 = best** → pas quality 100

### Tests
14. **Tous les tools ont un test fuzz** → 50-100 iterations de random bytes
15. **Backward compat** → les 5600+ tests existants DOIVENT toujours passer

---

## Deps a Ajouter (Cargo.toml)

```toml
# Tier 1 — Always on (0 deps)
imagesize = "0.13"
thumbhash = "0.1"

# Tier 2 — Default features
[features]
media-core = ["media-thumbnail", "media-metadata", "media-optimize", "media-svg"]
media-thumbnail = ["dep:fast_image_resize", "dep:image"]
media-metadata = ["dep:nom-exif", "dep:lofty", "dep:mp4"]
media-optimize = ["dep:oxipng"]
media-svg = ["dep:resvg", "dep:usvg", "dep:tiny-skia"]

[dependencies]
fast_image_resize = { version = "6.0", features = ["image"], optional = true }
image = { version = "0.25", optional = true, default-features = false, features = ["png", "jpeg", "webp", "gif"] }
nom-exif = { version = "2.7", optional = true }
lofty = { version = "0.23", optional = true }
mp4 = { version = "0.14", optional = true }
oxipng = { version = "10.1", optional = true, default-features = false, features = ["parallel"] }
resvg = { version = "0.47", optional = true }
usvg = { version = "0.47", optional = true }
tiny-skia = { version = "0.12", optional = true }
color-thief = { version = "0.2", optional = true }
rayon = "1.10"  # Pour ComputePool
```

---

## Fichiers Cles et Line Numbers (post-PR2)

```
src/runtime/builtin/mod.rs              ← ajouter mod media
src/runtime/builtin/router.rs           ← ajouter with_media_tools()
src/runtime/builtin/trait.rs            ← BuiltinTool trait (NE PAS MODIFIER)
src/runtime/builtin/file_adapter.rs     ← PATTERN A SUIVRE pour MediaToolAdapter
src/runtime/builtin/rig_adapter.rs      ← agent integration (1-line fix pour dots?)
src/runtime/executor/verbs.rs           ← ou invoke: dispatch les builtins
src/runtime/runner.rs                   ← ou BuiltinToolRouter est construit
src/media/processor.rs                  ← ajouter auto-enrichment (C13)
src/media/types.rs                      ← ajouter metadata field a MediaRef
src/media/store.rs                      ← CasStore (read/write/list)
src/media/detect.rs                     ← MIME detection (infer + heuristics)
src/media/error.rs                      ← MediaError (NIKA-251..259)
src/error.rs                            ← NikaError (ajouter NIKA-290..297)
src/io/writer.rs                        ← ArtifactWriter (write_binary)
Cargo.toml                              ← deps + features
```

---

## Regles d'Or

1. **Le plan a TOUJOURS raison** — il a ete verifie par 43 agents. Si tu doutes, relis le plan.
2. **TDD ou rien** — jamais de code sans test.
3. **Zero test qui fail** — jamais de commit avec un test rouge.
4. **1 commit = 1 changement logique** — pas de mega-commits.
5. **Review entre phases** — utilise `/spn-powers:requesting-code-review`.
6. **Si bloque → demande** — utilise AskUserQuestion plutot que deviner.
7. **Securite d'abord** — `decode_image_safe()`, `sanitize_svg()`, timeouts partout.
8. **Fuzz tout** — chaque tool a un test fuzz. Aucun panic autorise.
9. **Backward compat** — les 5600+ tests existants doivent passer.
10. **Mesure le delta binaire** — `cargo build --release` avant/apres chaque phase.

---

## GO

1. Lance les 4 agents d'exploration (voir "Contexte Initial")
2. Verifie la baseline (`cargo test --lib`, `cargo clippy`)
3. Lis le master plan v2.0 (`2026-03-19-media-superpowers-master-plan.md`)
4. Cree la branche : `git checkout -b feat/media-superpowers`
5. Attaque Phase A, Commit 1 : `feat(media): media tool dispatcher + MediaOp trait`
6. Suis le protocole commit par commit
7. Quality gate apres chaque phase
8. Push quand PR3a est complete (14 commits)
