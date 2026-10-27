// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The dated media price table (C-D3): the public rates of the paid media
//! models, hand-sourced from each provider's official price page and kept
//! apart from the generated models.dev snapshot, which carries no per-image
//! or per-character rate. Every row names its provider, model, unit, USD
//! rate, source page and the date the page was read; the table states only
//! what its source states. A model with no row is unpriced, and a cost cap
//! refuses it.
//!
//! Three units: a price per image by size class and quality (the admission
//! floor, and the bill of an image response that reports no usage), a price
//! per million characters for speech (exact from the text before the call),
//! and token rates for image models (text input, image input, image output,
//! and text and thinking output where the page gives one), applied to the
//! usage the provider reports. The run's prologue records the whole table
//! (`media_pricing`), so a replay reads the prices the run was charged at.

/// The schema marker of this table, journaled with it in the prologue.
pub const MEDIA_PRICING_SCHEMA: &str = "nika/media-pricing@1";

/// The date every row of this table was read from its source.
pub const MEDIA_PRICING_AS_OF: &str = "2026-10-27";

/// `OpenAI`'s API price page: image-model token rates and speech rates.
const OPENAI_PRICING: &str = "https://developers.openai.com/api/docs/pricing";
/// `OpenAI`'s image generation guide: the per-image price table.
const OPENAI_IMAGE_GUIDE: &str =
    "https://developers.openai.com/api/docs/guides/image-generation#calculating-costs";
/// The Gemini API price page.
const GEMINI_PRICING: &str = "https://ai.google.dev/gemini-api/docs/pricing";
/// The `ElevenLabs` API price page.
const ELEVENLABS_PRICING: &str = "https://elevenlabs.io/pricing/api";

/// A token axis an image provider reports and bills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenAxis {
    /// The text of the prompt.
    TextInput,
    /// Input images (an edit's sources).
    ImageInput,
    /// Generated images.
    ImageOutput,
    /// Generated text and thinking.
    TextOutput,
}

impl TokenAxis {
    /// The axis as the prologue record and the check rows name it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TextInput => "text input",
            Self::ImageInput => "image input",
            Self::ImageOutput => "image output",
            Self::TextOutput => "text and thinking output",
        }
    }
}

/// What one media rate prices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MediaUnit {
    /// One delivered image of `size` at `quality`, as the source states it:
    /// `size` is an exact `WxH` (`1024x1536`) or a resolution class (`1K`),
    /// `quality` a level (`low`) or `any` for a model with no quality setting.
    Image {
        /// The size or resolution class.
        size: &'static str,
        /// The quality level, or `any`.
        quality: &'static str,
    },
    /// One million characters of the text a speech call sends.
    MillionCharacters,
    /// One million tokens of one reported usage axis.
    MillionTokens(TokenAxis),
}

impl MediaUnit {
    /// The unit in words: `image (1024x1536, low)` · `1M characters` ·
    /// `1M image output tokens`.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Image {
                size,
                quality: "any",
            } => format!("image ({size})"),
            Self::Image { size, quality } => format!("image ({size}, {quality})"),
            Self::MillionCharacters => "1M characters".to_owned(),
            Self::MillionTokens(axis) => format!("1M {} tokens", axis.name()),
        }
    }
}

/// One dated media rate: USD per [`MediaUnit`] of one model.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MediaRate {
    /// The builtin it prices (`image_generate` · `tts_generate`).
    pub tool: &'static str,
    /// The provider that bills it.
    pub provider: &'static str,
    /// The provider's model id.
    pub model: &'static str,
    /// What the rate prices.
    pub unit: MediaUnit,
    /// USD per unit.
    pub usd: f64,
    /// The official public page the rate was read from.
    pub source: &'static str,
    /// The date it was read (ISO `YYYY-MM-DD`).
    pub as_of: &'static str,
}

const fn image(
    provider: &'static str,
    model: &'static str,
    unit: MediaUnit,
    usd: f64,
    source: &'static str,
) -> MediaRate {
    MediaRate {
        tool: "image_generate",
        provider,
        model,
        unit,
        usd,
        source,
        as_of: MEDIA_PRICING_AS_OF,
    }
}

const fn per_image(
    provider: &'static str,
    model: &'static str,
    size: &'static str,
    quality: &'static str,
    usd: f64,
    source: &'static str,
) -> MediaRate {
    image(
        provider,
        model,
        MediaUnit::Image { size, quality },
        usd,
        source,
    )
}

const fn tokens(
    provider: &'static str,
    model: &'static str,
    axis: TokenAxis,
    usd: f64,
    source: &'static str,
) -> MediaRate {
    image(provider, model, MediaUnit::MillionTokens(axis), usd, source)
}

const fn speech(
    provider: &'static str,
    model: &'static str,
    usd: f64,
    source: &'static str,
) -> MediaRate {
    MediaRate {
        tool: "tts_generate",
        provider,
        model,
        unit: MediaUnit::MillionCharacters,
        usd,
        source,
        as_of: MEDIA_PRICING_AS_OF,
    }
}

use TokenAxis::{ImageInput, ImageOutput, TextInput, TextOutput};

/// The table, as each source stated it on [`MEDIA_PRICING_AS_OF`] (standard
/// tier, paid usage; batch, flex and priority rates are not this engine's
/// requests). Token rates are USD per million tokens; `ElevenLabs` states
/// USD per 1K characters, kept here per million (×1000).
#[rustfmt::skip]
const RATES: &[MediaRate] = &[
    // ── OpenAI images · gpt-image-2 (the default) ──────────────────────
    // Per image, output only: the guide's table at its three listed sizes.
    per_image("openai", "gpt-image-2", "1024x1024", "low", 0.006, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1024x1536", "low", 0.005, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1536x1024", "low", 0.005, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1024x1024", "medium", 0.053, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1024x1536", "medium", 0.041, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1536x1024", "medium", 0.041, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1024x1024", "high", 0.211, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1024x1536", "high", 0.165, OPENAI_IMAGE_GUIDE),
    per_image("openai", "gpt-image-2", "1536x1024", "high", 0.165, OPENAI_IMAGE_GUIDE),
    tokens("openai", "gpt-image-2", TextInput, 5.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2", ImageInput, 8.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2", ImageOutput, 30.0, OPENAI_PRICING),
    // ── OpenAI images · GPT Image 2.5 (token rates only: no per-image price) ─
    tokens("openai", "gpt-image-2.5-sunburst", TextInput, 5.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2.5-sunburst", ImageInput, 8.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2.5-sunburst", ImageOutput, 30.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2.5-flare", TextInput, 5.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2.5-flare", ImageInput, 8.0, OPENAI_PRICING),
    tokens("openai", "gpt-image-2.5-flare", ImageOutput, 30.0, OPENAI_PRICING),
    // ── Gemini images · gemini-3.1-flash-image (the default) ───────────
    per_image("gemini", "gemini-3.1-flash-image", "0.5K", "any", 0.045, GEMINI_PRICING),
    per_image("gemini", "gemini-3.1-flash-image", "1K", "any", 0.067, GEMINI_PRICING),
    per_image("gemini", "gemini-3.1-flash-image", "2K", "any", 0.101, GEMINI_PRICING),
    per_image("gemini", "gemini-3.1-flash-image", "4K", "any", 0.151, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-image", TextInput, 0.50, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-image", ImageInput, 0.50, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-image", ImageOutput, 60.0, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-image", TextOutput, 3.0, GEMINI_PRICING),
    // ── Gemini images · gemini-3-pro-image ──────────────────────────────
    per_image("gemini", "gemini-3-pro-image", "1K", "any", 0.134, GEMINI_PRICING),
    per_image("gemini", "gemini-3-pro-image", "2K", "any", 0.134, GEMINI_PRICING),
    per_image("gemini", "gemini-3-pro-image", "4K", "any", 0.24, GEMINI_PRICING),
    tokens("gemini", "gemini-3-pro-image", TextInput, 2.0, GEMINI_PRICING),
    tokens("gemini", "gemini-3-pro-image", ImageInput, 2.0, GEMINI_PRICING),
    tokens("gemini", "gemini-3-pro-image", ImageOutput, 120.0, GEMINI_PRICING),
    tokens("gemini", "gemini-3-pro-image", TextOutput, 12.0, GEMINI_PRICING),
    // ── Gemini images · gemini-2.5-flash-image (text priced as 2.5 Flash) ─
    per_image("gemini", "gemini-2.5-flash-image", "1K", "any", 0.039, GEMINI_PRICING),
    tokens("gemini", "gemini-2.5-flash-image", TextInput, 0.30, GEMINI_PRICING),
    tokens("gemini", "gemini-2.5-flash-image", ImageInput, 0.30, GEMINI_PRICING),
    tokens("gemini", "gemini-2.5-flash-image", ImageOutput, 30.0, GEMINI_PRICING),
    tokens("gemini", "gemini-2.5-flash-image", TextOutput, 2.50, GEMINI_PRICING),
    // ── Gemini images · gemini-3.1-flash-lite-image (1K only) ──────────
    per_image("gemini", "gemini-3.1-flash-lite-image", "1K", "any", 0.0336, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-lite-image", TextInput, 0.25, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-lite-image", ImageInput, 0.25, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-lite-image", ImageOutput, 30.0, GEMINI_PRICING),
    tokens("gemini", "gemini-3.1-flash-lite-image", TextOutput, 1.50, GEMINI_PRICING),
    // ── Gemini images · gemini-nano-banana-2.1 ──────────────────────────
    per_image("gemini", "gemini-nano-banana-2.1", "1K", "any", 0.0336, GEMINI_PRICING),
    per_image("gemini", "gemini-nano-banana-2.1", "2K", "any", 0.0504, GEMINI_PRICING),
    per_image("gemini", "gemini-nano-banana-2.1", "4K", "any", 0.113, GEMINI_PRICING),
    tokens("gemini", "gemini-nano-banana-2.1", TextInput, 1.50, GEMINI_PRICING),
    tokens("gemini", "gemini-nano-banana-2.1", ImageInput, 1.50, GEMINI_PRICING),
    tokens("gemini", "gemini-nano-banana-2.1", ImageOutput, 30.0, GEMINI_PRICING),
    tokens("gemini", "gemini-nano-banana-2.1", TextOutput, 7.50, GEMINI_PRICING),
    // ── OpenAI speech (per character; gpt-4o-mini-tts has none) ─────────
    speech("openai", "tts-1", 15.0, OPENAI_PRICING),
    speech("openai", "tts-1-hd", 30.0, OPENAI_PRICING),
    // ── ElevenLabs speech: each model id under its page family ─────────
    speech("elevenlabs", "eleven_v4", 80.0, ELEVENLABS_PRICING),                // v4
    speech("elevenlabs", "eleven_v4_turbo", 40.0, ELEVENLABS_PRICING),          // v4 Turbo
    speech("elevenlabs", "eleven_v3", 80.0, ELEVENLABS_PRICING),                // v3
    speech("elevenlabs", "eleven_v3_conversational", 40.0, ELEVENLABS_PRICING), // v3 Conversational
    speech("elevenlabs", "eleven_multilingual_v2", 80.0, ELEVENLABS_PRICING),   // v2 Multilingual
    speech("elevenlabs", "eleven_flash_v2_5", 40.0, ELEVENLABS_PRICING),        // Flash / Turbo
    speech("elevenlabs", "eleven_flash_v2", 40.0, ELEVENLABS_PRICING),          // Flash / Turbo
    speech("elevenlabs", "eleven_turbo_v2_5", 40.0, ELEVENLABS_PRICING),        // Flash / Turbo
    speech("elevenlabs", "eleven_turbo_v2", 40.0, ELEVENLABS_PRICING),          // Flash / Turbo
];

/// Every row of the dated media price table, in table order.
#[must_use]
pub fn media_rates() -> &'static [MediaRate] {
    RATES
}

/// The dated media table as the run's prologue records it (`media_pricing`):
/// one JSON object with the table's `schema`, its `as_of` date and its
/// `rates`, one object per row carrying `tool`, `provider`, `model`, `unit`
/// (`image` with its `size` and `quality`, `million_characters`, or
/// `million_tokens` with its `axis`), `usd`, `source` and `as_of`. A replay
/// compares the record its run pinned with this binary's and refuses a
/// different table: a cost is read against the rates it was charged at.
#[cfg(feature = "serde")]
#[must_use]
pub fn media_pricing_record() -> serde_json::Value {
    let rates: Vec<serde_json::Value> = RATES
        .iter()
        .map(|rate| {
            let mut row = serde_json::json!({
                "tool": rate.tool,
                "provider": rate.provider,
                "model": rate.model,
                "usd": rate.usd,
                "source": rate.source,
                "as_of": rate.as_of,
            });
            let unit = match rate.unit {
                MediaUnit::Image { size, quality } => serde_json::json!({
                    "unit": "image", "size": size, "quality": quality,
                }),
                MediaUnit::MillionCharacters => serde_json::json!({ "unit": "million_characters" }),
                MediaUnit::MillionTokens(axis) => serde_json::json!({
                    "unit": "million_tokens",
                    "axis": match axis {
                        TokenAxis::TextInput => "text_input",
                        TokenAxis::ImageInput => "image_input",
                        TokenAxis::ImageOutput => "image_output",
                        TokenAxis::TextOutput => "text_output",
                    },
                }),
            };
            if let (Some(row), serde_json::Value::Object(unit)) = (row.as_object_mut(), unit) {
                row.extend(unit);
            }
            row
        })
        .collect();
    serde_json::json!({
        "schema": MEDIA_PRICING_SCHEMA,
        "as_of": MEDIA_PRICING_AS_OF,
        "rates": rates,
    })
}

/// One media model the dated table prices: the handle its rates hang on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MediaModel {
    /// The builtin (`image_generate` · `tts_generate`).
    pub tool: &'static str,
    /// The provider that bills it.
    pub provider: &'static str,
    /// The provider's model id.
    pub model: &'static str,
}

/// The priced media model `model` of `provider` on `tool` (`nika:` prefix
/// optional): `None` when the table has no row for it, so it is unpriced.
#[must_use]
pub fn find_media_model(tool: &str, provider: &str, model: &str) -> Option<MediaModel> {
    let tool = tool.strip_prefix("nika:").unwrap_or(tool);
    RATES
        .iter()
        .find(|r| r.tool == tool && r.provider == provider && r.model == model)
        .map(|r| MediaModel {
            tool: r.tool,
            provider: r.provider,
            model: r.model,
        })
}

/// The price-relevant shape of one image request, as written in its
/// arguments: what [`MediaModel::image_floor`] reads to pick a size class
/// and a quality. An absent field is the provider's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageRequest<'a> {
    /// `size:` (`WxH` · `auto`).
    pub size: Option<&'a str>,
    /// `aspect_ratio:` (`3:2`).
    pub aspect_ratio: Option<&'a str>,
    /// `provider_options.image_size` (Gemini's class: `512` · `1K` · `2K` · `4K`).
    pub image_size: Option<&'a str>,
    /// `quality:` (`auto` · `low` · `medium` · `high` · `ultra`).
    pub quality: Option<&'a str>,
}

impl<'a> ImageRequest<'a> {
    /// A request with every field at the provider's default.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// With `size:`.
    #[must_use]
    pub fn with_size(mut self, size: Option<&'a str>) -> Self {
        self.size = size;
        self
    }

    /// With `aspect_ratio:`.
    #[must_use]
    pub fn with_aspect_ratio(mut self, aspect_ratio: Option<&'a str>) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    /// With `provider_options.image_size`.
    #[must_use]
    pub fn with_image_size(mut self, image_size: Option<&'a str>) -> Self {
        self.image_size = image_size;
        self
    }

    /// With `quality:`.
    #[must_use]
    pub fn with_quality(mut self, quality: Option<&'a str>) -> Self {
        self.quality = quality;
        self
    }
}

/// The usage one image call reported, axis by axis; an axis the provider did
/// not report is `None`. Gemini reports thinking apart from its output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageUsage {
    /// Every input token.
    pub input_tokens: Option<u64>,
    /// Input text tokens, when the provider splits its input.
    pub input_text_tokens: Option<u64>,
    /// Input image tokens, when the provider splits its input.
    pub input_image_tokens: Option<u64>,
    /// Every output token (thinking excluded).
    pub output_tokens: Option<u64>,
    /// Output image tokens, when the provider splits its output.
    pub output_image_tokens: Option<u64>,
    /// Output text tokens, when the provider splits its output.
    pub output_text_tokens: Option<u64>,
    /// Thinking tokens billed apart from the output.
    pub thinking_tokens: Option<u64>,
}

impl ImageUsage {
    /// No axis reported.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// With the input totals: every token, and its text and image split.
    #[must_use]
    pub fn with_input(mut self, total: Option<u64>, text: Option<u64>, image: Option<u64>) -> Self {
        self.input_tokens = total;
        self.input_text_tokens = text;
        self.input_image_tokens = image;
        self
    }

    /// With the output totals: every token, and its image and text split.
    #[must_use]
    pub fn with_output(
        mut self,
        total: Option<u64>,
        image: Option<u64>,
        text: Option<u64>,
    ) -> Self {
        self.output_tokens = total;
        self.output_image_tokens = image;
        self.output_text_tokens = text;
        self
    }

    /// With the thinking tokens billed apart.
    #[must_use]
    pub fn with_thinking(mut self, thinking: Option<u64>) -> Self {
        self.thinking_tokens = thinking;
        self
    }
}

impl MediaModel {
    /// The model's rows, in table order.
    pub fn rates(self) -> impl Iterator<Item = &'static MediaRate> {
        RATES.iter().filter(move |r| {
            r.tool == self.tool && r.provider == self.provider && r.model == self.model
        })
    }

    /// The date its rows were read.
    #[must_use]
    pub fn as_of(self) -> &'static str {
        self.rates().next().map_or(MEDIA_PRICING_AS_OF, |r| r.as_of)
    }

    /// The USD rate per million tokens of `axis`, when its source states one.
    #[must_use]
    pub fn token_rate(self, axis: TokenAxis) -> Option<&'static MediaRate> {
        self.rates()
            .find(|r| r.unit == MediaUnit::MillionTokens(axis))
    }

    /// The USD rate per million characters (speech), when its source states one.
    #[must_use]
    pub fn character_rate(self) -> Option<&'static MediaRate> {
        self.rates()
            .find(|r| r.unit == MediaUnit::MillionCharacters)
    }

    /// The cheapest per-image row `request` can be billed at: the row of its
    /// size class and quality when the source lists that pair, else the
    /// lowest the source lists over what the request leaves open (`quality:
    /// auto`, an unlisted size). `None` for a model priced by tokens only.
    #[must_use]
    pub fn image_floor(self, request: &ImageRequest<'_>) -> Option<&'static MediaRate> {
        let images: Vec<&'static MediaRate> = self
            .rates()
            .filter(|r| matches!(r.unit, MediaUnit::Image { .. }))
            .collect();
        let size = size_class(self.provider, request);
        let quality = match request.quality {
            Some("ultra") => Some("high"),
            Some("auto") | None => None,
            other => other,
        };
        let narrowed = |rows: Vec<&'static MediaRate>, keep: &dyn Fn(&MediaUnit) -> bool| {
            let kept: Vec<&'static MediaRate> =
                rows.iter().copied().filter(|r| keep(&r.unit)).collect();
            if kept.is_empty() { rows } else { kept }
        };
        let images = match size.as_deref() {
            Some(class) => narrowed(
                images,
                &|unit| matches!(unit, MediaUnit::Image { size, .. } if *size == class),
            ),
            None => images,
        };
        let images = match quality {
            Some(level) => narrowed(
                images,
                &|unit| matches!(unit, MediaUnit::Image { quality, .. } if *quality == level),
            ),
            None => images,
        };
        images.into_iter().min_by(|a, b| a.usd.total_cmp(&b.usd))
    }

    /// What `characters` of speech cost at the model's per-character rate.
    #[must_use]
    pub fn speech_usd(self, characters: u64) -> Option<f64> {
        self.character_rate()
            .map(|rate| per_million(characters, rate.usd))
    }

    /// What one image call costs from the usage it reported: every reported
    /// axis at its rate. An axis the provider did not split is billed at the
    /// highest rate it could fall under (input at the higher of the text and
    /// image input rates; output at the image output rate, or the text rate
    /// when that is higher), never below. `None` when the output was not
    /// reported (the bulk of an image bill) or the model has no token rate.
    #[must_use]
    pub fn image_usage_usd(self, usage: &ImageUsage) -> Option<f64> {
        let rate = |axis| self.token_rate(axis).map(|r| r.usd);
        let image_out = rate(ImageOutput)?;
        let text_out = rate(TextOutput);
        let out_max = text_out.map_or(image_out, |t| t.max(image_out));
        let in_max = match (rate(TextInput), rate(ImageInput)) {
            (Some(text), Some(image)) => Some(text.max(image)),
            (one, other) => one.or(other),
        };
        let output = split_cost(
            usage.output_tokens,
            [
                (usage.output_image_tokens, Some(image_out)),
                (usage.output_text_tokens, text_out.or(Some(out_max))),
            ],
            out_max,
        )?;
        let input = in_max.and_then(|in_max| {
            split_cost(
                usage.input_tokens,
                [
                    (usage.input_text_tokens, rate(TextInput)),
                    (usage.input_image_tokens, rate(ImageInput)),
                ],
                in_max,
            )
        });
        let thinking = usage
            .thinking_tokens
            .map(|t| per_million(t, text_out.unwrap_or(out_max)));
        Some(output + input.unwrap_or(0.0) + thinking.unwrap_or(0.0))
    }
}

/// USD for `tokens` (or characters) at `rate` per million.
#[allow(clippy::cast_precision_loss)] // provider counts and request texts ≪ 2^53
fn per_million(tokens: u64, rate: f64) -> f64 {
    tokens as f64 * rate / 1_000_000.0
}

/// One reported total, billed by its split: each split axis at its rate, and
/// what the split leaves of the total at `fallback`, the highest rate the
/// remainder could be billed at. `None` when nothing was reported.
fn split_cost(
    total: Option<u64>,
    parts: [(Option<u64>, Option<f64>); 2],
    fallback: f64,
) -> Option<f64> {
    let split: u64 = parts.iter().filter_map(|(tokens, _)| *tokens).sum();
    let known: f64 = parts
        .iter()
        .filter_map(|(tokens, rate)| Some(per_million((*tokens)?, rate.unwrap_or(fallback))))
        .sum();
    match total {
        Some(total) => Some(known + per_million(total.saturating_sub(split), fallback)),
        None if parts.iter().any(|(tokens, _)| tokens.is_some()) => Some(known),
        None => None,
    }
}

/// The size class an image request is billed at by `provider`, as its
/// adapter sends it: `None` when the request leaves the size to the provider
/// in a way its price page does not list (the floor is then the lowest the
/// page lists). `OpenAI`'s listed sizes are exact (`1024x1536`); an aspect
/// ratio maps to the exact size the adapter sends. Gemini bills a resolution
/// class: `provider_options.image_size`, else the class of an exact `size:`
/// by its longer edge, else `1K` (the documented default).
fn size_class(provider: &str, request: &ImageRequest<'_>) -> Option<String> {
    let exact = request.size.filter(|s| *s != "auto");
    match provider {
        "openai" => exact.map(str::to_owned).or_else(|| {
            let mapped = match request.aspect_ratio? {
                "1:1" => "1024x1024",
                "2:3" => "1024x1536",
                "3:2" => "1536x1024",
                _ => return None,
            };
            Some(mapped.to_owned())
        }),
        "gemini" => {
            if let Some(class) = request.image_size {
                return Some(match class {
                    "512" | "0.5K" => "0.5K".to_owned(),
                    other => other.to_owned(),
                });
            }
            let Some(exact) = exact else {
                return Some("1K".to_owned());
            };
            // A size the run has yet to render (or that does not parse)
            // leaves the class open: the floor is the lowest listed.
            let (w, h) = exact.split_once('x')?;
            let edge = w.parse::<u32>().ok()?.max(h.parse::<u32>().ok()?);
            Some(
                match edge {
                    0..=512 => "0.5K",
                    513..=1024 => "1K",
                    1025..=2048 => "2K",
                    _ => "4K",
                }
                .to_owned(),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {expected}, got {actual}"
        );
    }

    fn model(tool: &str, provider: &str, name: &str) -> MediaModel {
        find_media_model(tool, provider, name).expect("a priced model")
    }

    /// Every row is dated, sourced from an official page over HTTPS, and
    /// strictly positive: the table never carries a free or undated rate.
    #[test]
    fn every_row_is_dated_sourced_and_positive() {
        assert!(!media_rates().is_empty());
        for row in media_rates() {
            assert_eq!(row.as_of, MEDIA_PRICING_AS_OF, "{row:?}");
            assert!(row.source.starts_with("https://"), "{row:?}");
            assert!(row.usd.is_finite() && row.usd > 0.0, "{row:?}");
            assert!(
                matches!(row.tool, "image_generate" | "tts_generate"),
                "{row:?}"
            );
            let speech = row.unit == MediaUnit::MillionCharacters;
            assert_eq!(speech, row.tool == "tts_generate", "{row:?}");
        }
    }

    /// No row is listed twice: one model, one rate per unit.
    #[test]
    fn no_unit_is_priced_twice() {
        let rows = media_rates();
        for (i, a) in rows.iter().enumerate() {
            for b in &rows[i + 1..] {
                assert!(
                    !(a.tool == b.tool
                        && a.provider == b.provider
                        && a.model == b.model
                        && a.unit == b.unit),
                    "{a:?} twice"
                );
            }
        }
    }

    /// The models C-D3 prices are in the table, and the ones it keeps
    /// unpriced are not.
    #[test]
    fn the_priced_and_unpriced_models_are_the_decided_ones() {
        for (tool, provider, name) in [
            ("image_generate", "openai", "gpt-image-2"),
            ("image_generate", "openai", "gpt-image-2.5-sunburst"),
            ("image_generate", "openai", "gpt-image-2.5-flare"),
            ("image_generate", "gemini", "gemini-3.1-flash-image"),
            ("image_generate", "gemini", "gemini-3-pro-image"),
            ("image_generate", "gemini", "gemini-2.5-flash-image"),
            ("image_generate", "gemini", "gemini-3.1-flash-lite-image"),
            ("image_generate", "gemini", "gemini-nano-banana-2.1"),
            ("tts_generate", "openai", "tts-1"),
            ("tts_generate", "openai", "tts-1-hd"),
            ("tts_generate", "elevenlabs", "eleven_multilingual_v2"),
            ("tts_generate", "elevenlabs", "eleven_v3"),
            ("tts_generate", "elevenlabs", "eleven_flash_v2_5"),
        ] {
            assert!(
                find_media_model(tool, provider, name).is_some(),
                "{provider} {name}"
            );
        }
        for (tool, provider, name) in [
            ("tts_generate", "openai", "gpt-4o-mini-tts"),
            ("image_generate", "openai", "gpt-image-1.5"),
            ("image_generate", "openai", "gpt-image-1-mini"),
            ("tts_generate", "elevenlabs", "eleven_multilingual_v1"),
            ("tts_generate", "elevenlabs", "eleven_ttv_v3"),
            (
                "image_generate",
                "gemini",
                "gemini-2.0-flash-preview-image-generation",
            ),
            // A model priced for one builtin is never priced for another.
            ("tts_generate", "openai", "gpt-image-2"),
        ] {
            assert!(
                find_media_model(tool, provider, name).is_none(),
                "{provider} {name} stays unpriced"
            );
        }
        assert!(find_media_model("nika:tts_generate", "openai", "tts-1").is_some());
    }

    /// `OpenAI`'s per-image floor: the listed (size, quality) row; `auto`
    /// quality is the lowest listed price for its size; an unlisted size is
    /// the lowest listed price at its quality; nothing named, the lowest.
    #[test]
    fn the_gpt_image_2_floor_reads_the_listed_size_and_quality() {
        let m = model("image_generate", "openai", "gpt-image-2");
        let floor = |request: ImageRequest<'_>| m.image_floor(&request).expect("listed").usd;
        approx(
            floor(
                ImageRequest::new()
                    .with_size(Some("1024x1024"))
                    .with_quality(Some("high")),
            ),
            0.211,
        );
        approx(
            floor(
                ImageRequest::new()
                    .with_size(Some("1536x1024"))
                    .with_quality(Some("medium")),
            ),
            0.041,
        );
        approx(
            floor(
                ImageRequest::new()
                    .with_aspect_ratio(Some("2:3"))
                    .with_quality(Some("ultra")),
            ),
            0.165,
        );
        approx(
            floor(
                ImageRequest::new()
                    .with_size(Some("1024x1024"))
                    .with_quality(Some("auto")),
            ),
            0.006,
        );
        approx(
            floor(
                ImageRequest::new()
                    .with_size(Some("2048x1152"))
                    .with_quality(Some("high")),
            ),
            0.165,
        );
        approx(
            floor(ImageRequest::new().with_aspect_ratio(Some("16:9"))),
            0.005,
        );
        approx(floor(ImageRequest::new()), 0.005);
        approx(
            floor(
                ImageRequest::new()
                    .with_size(Some("auto"))
                    .with_quality(Some("medium")),
            ),
            0.041,
        );
    }

    /// Gemini bills a resolution class: the explicit option, else the class
    /// of an exact size by its longer edge, else 1K; a class the model does
    /// not list floors at the lowest price it does.
    #[test]
    fn the_gemini_floor_reads_the_resolution_class() {
        let flash = model("image_generate", "gemini", "gemini-3.1-flash-image");
        let floor =
            |m: MediaModel, request: ImageRequest<'_>| m.image_floor(&request).expect("listed").usd;
        approx(floor(flash, ImageRequest::new()), 0.067);
        approx(
            floor(flash, ImageRequest::new().with_aspect_ratio(Some("16:9"))),
            0.067,
        );
        approx(
            floor(flash, ImageRequest::new().with_image_size(Some("4K"))),
            0.151,
        );
        approx(
            floor(flash, ImageRequest::new().with_image_size(Some("512"))),
            0.045,
        );
        approx(
            floor(flash, ImageRequest::new().with_size(Some("2048x1152"))),
            0.101,
        );
        approx(
            floor(flash, ImageRequest::new().with_size(Some("512x512"))),
            0.045,
        );
        // A size only the run renders leaves the class open: the lowest.
        approx(
            floor(
                flash,
                ImageRequest::new().with_size(Some("${{ inputs.size }}")),
            ),
            0.045,
        );
        let pro = model("image_generate", "gemini", "gemini-3-pro-image");
        approx(
            floor(pro, ImageRequest::new().with_image_size(Some("2K"))),
            0.134,
        );
        approx(
            floor(pro, ImageRequest::new().with_image_size(Some("4K"))),
            0.24,
        );
        approx(
            floor(pro, ImageRequest::new().with_image_size(Some("512"))),
            0.134,
        );
        let lite = model("image_generate", "gemini", "gemini-3.1-flash-lite-image");
        approx(
            floor(lite, ImageRequest::new().with_image_size(Some("4K"))),
            0.0336,
        );
        let banana = model("image_generate", "gemini", "gemini-nano-banana-2.1");
        approx(
            floor(banana, ImageRequest::new().with_image_size(Some("2K"))),
            0.0504,
        );
        approx(
            floor(
                model("image_generate", "gemini", "gemini-2.5-flash-image"),
                ImageRequest::new(),
            ),
            0.039,
        );
    }

    /// GPT Image 2.5 publishes token rates only: no per-image floor.
    #[test]
    fn a_token_priced_model_has_no_image_floor() {
        for name in ["gpt-image-2.5-sunburst", "gpt-image-2.5-flare"] {
            let m = model("image_generate", "openai", name);
            assert_eq!(m.image_floor(&ImageRequest::new()), None, "{name}");
            approx(m.token_rate(ImageOutput).expect("rate").usd, 30.0);
        }
    }

    /// Speech is billed per character: rate × characters ÷ 1M.
    #[test]
    fn speech_is_priced_per_character() {
        approx(
            model("tts_generate", "openai", "tts-1")
                .speech_usd(1_000)
                .expect("priced"),
            0.015,
        );
        approx(
            model("tts_generate", "openai", "tts-1-hd")
                .speech_usd(1_000)
                .expect("priced"),
            0.03,
        );
        approx(
            model("tts_generate", "elevenlabs", "eleven_multilingual_v2")
                .speech_usd(1_000)
                .expect("priced"),
            0.08,
        );
        approx(
            model("tts_generate", "elevenlabs", "eleven_flash_v2_5")
                .speech_usd(1_000)
                .expect("priced"),
            0.04,
        );
        assert_eq!(
            model("image_generate", "openai", "gpt-image-2").speech_usd(10),
            None
        );
    }

    /// The image bill from reported usage: each split axis at its rate, an
    /// unsplit remainder at the highest rate it could be, thinking at the
    /// text and thinking rate; no reported output, no bill.
    #[test]
    fn image_usage_is_billed_axis_by_axis() {
        let openai = model("image_generate", "openai", "gpt-image-2");
        // 10 text in × $5 + 1000 image out × $30 = $0.00005 + $0.03.
        let split = ImageUsage::new()
            .with_input(Some(10), Some(10), Some(0))
            .with_output(Some(1_000), None, None);
        approx(openai.image_usage_usd(&split).expect("billed"), 0.030_05);
        // No input split: the input is billed at the image input rate ($8).
        let whole = ImageUsage::new()
            .with_input(Some(10), None, None)
            .with_output(Some(1_000), None, None);
        approx(openai.image_usage_usd(&whole).expect("billed"), 0.030_08);
        assert_eq!(openai.image_usage_usd(&ImageUsage::new()), None);
        assert_eq!(
            openai.image_usage_usd(&ImageUsage::new().with_input(Some(10), None, None)),
            None,
            "no output reported: no bill from usage"
        );
        let gemini = model("image_generate", "gemini", "gemini-3.1-flash-image");
        // 20 in × $0.50 + 1120 image × $60 + 30 text × $3 + 100 thinking × $3.
        let usage = ImageUsage::new()
            .with_input(Some(20), None, None)
            .with_output(Some(1_150), Some(1_120), Some(30))
            .with_thinking(Some(100));
        approx(
            gemini.image_usage_usd(&usage).expect("billed"),
            0.000_01 + 0.0672 + 0.000_09 + 0.000_3,
        );
        // Without the modality split, all output is billed at the image rate.
        let unsplit = ImageUsage::new().with_output(Some(1_150), None, None);
        approx(gemini.image_usage_usd(&unsplit).expect("billed"), 0.069);
    }

    /// The prologue record carries every row with its unit spelled out, so
    /// a replay can compare it row for row.
    #[cfg(feature = "serde")]
    #[test]
    fn the_prologue_record_carries_every_dated_row() {
        let record = media_pricing_record();
        assert_eq!(record["schema"], MEDIA_PRICING_SCHEMA);
        assert_eq!(record["as_of"], MEDIA_PRICING_AS_OF);
        let rows = record["rates"].as_array().expect("rates");
        assert_eq!(rows.len(), media_rates().len());
        assert!(rows.iter().any(|r| r["model"] == "gpt-image-2"
            && r["unit"] == "image"
            && r["size"] == "1024x1024"
            && r["quality"] == "high"
            && r["usd"] == 0.211
            && r["source"] == OPENAI_IMAGE_GUIDE));
        assert!(rows.iter().any(|r| r["model"] == "tts-1"
            && r["unit"] == "million_characters"
            && r["usd"] == 15.0));
        assert!(rows.iter().any(|r| r["model"] == "gemini-3-pro-image"
            && r["unit"] == "million_tokens"
            && r["axis"] == "image_output"
            && r["usd"] == 120.0));
        assert!(rows.iter().all(|r| r["as_of"] == MEDIA_PRICING_AS_OF));
    }

    /// Unit labels read as the check rows print them.
    #[test]
    fn units_read_in_words() {
        assert_eq!(
            MediaUnit::Image {
                size: "1K",
                quality: "any"
            }
            .label(),
            "image (1K)"
        );
        assert_eq!(
            MediaUnit::Image {
                size: "1024x1536",
                quality: "low"
            }
            .label(),
            "image (1024x1536, low)"
        );
        assert_eq!(MediaUnit::MillionCharacters.label(), "1M characters");
        assert_eq!(
            MediaUnit::MillionTokens(TextOutput).label(),
            "1M text and thinking output tokens"
        );
    }
}
