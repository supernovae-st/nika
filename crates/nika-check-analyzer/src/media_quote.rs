// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The price of a paid-media builtin call (`nika:image_generate` ·
//! `nika:tts_generate`), judged on its arguments the way the builtin reads
//! them (C-D3). One judgment serves every reader: the launch floor and the
//! check's cost rows judge the literal arguments, the dispatch guard and an
//! `agent:` loop judge the rendered ones before the request, and the ledger
//! reads why a returned call's spend is absent.
//!
//! The provider is the explicit `provider:`, else the one its `model:` names;
//! the model is the explicit `model:`, else the provider's default. xAI
//! images carry a per-image price whatever the model; the dated media table
//! prices the other paid models per image, per character or per token, and
//! any model it does not list is unpriced. An argument only the run decides
//! leaves the floor at the least the table lists over what it could be (a
//! templated `quality:` floors at the cheapest quality, a templated `n:` at
//! one image), and a templated provider or model leaves the whole price to
//! the run.

use nika_catalog::{ImageRequest, MediaPrice, MediaRate, TokenAxis};
use serde_json::Value;

/// What one paid-media builtin call costs at least, judged on its arguments.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MediaQuote<'a> {
    /// Not paid media: another builtin, a local server, the mock or the
    /// keyless `system` voice.
    Unmetered,
    /// A call the catalog prices: the provider it bills, the model when its
    /// price depends on one, and the least it costs.
    Priced {
        /// The provider the call bills.
        provider: &'a str,
        /// The model it bills (explicit, else the provider's default) ·
        /// `None` when the price does not depend on the model (xAI).
        model: Option<&'a str>,
        /// The least the call costs.
        floor: MediaFloor,
    },
    /// A paid call no catalog rate prices: real spend a cap cannot bound.
    Unpriced {
        /// The provider the call bills.
        provider: &'a str,
        /// The model it bills, when the arguments or the default name one.
        model: Option<&'a str>,
    },
    /// Only the run decides who bills the call or which model: the template
    /// that decides it.
    RunDecided(&'a str),
}

/// The least a priced media call costs.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MediaFloor {
    /// At least `usd` for the whole call: every image of its `n:`, every
    /// character of its text.
    Usd {
        /// The floor, USD.
        usd: f64,
        /// The dated rate that sets it · `None` for xAI's per-image price.
        rate: Option<&'static MediaRate>,
    },
    /// Above zero, known only once the call runs: a model priced from the
    /// usage it reports (no per-image price), or speech whose text only the
    /// run renders.
    Metered {
        /// The rate the bill will read (image output tokens, characters).
        rate: Option<&'static MediaRate>,
    },
}

impl MediaFloor {
    /// The floor in USD, when it is known before the call.
    #[must_use]
    pub fn usd(self) -> Option<f64> {
        match self {
            Self::Usd { usd, .. } => Some(usd),
            Self::Metered { .. } => None,
        }
    }

    /// The dated rate that sets or meters it (`None`: xAI's per-image price).
    #[must_use]
    pub fn rate(self) -> Option<&'static MediaRate> {
        match self {
            Self::Usd { rate, .. } | Self::Metered { rate } => rate,
        }
    }
}

/// Why an armed cost cap refuses a paid-media call before its request.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MediaCapRefusal<'a> {
    /// No catalog rate prices it.
    Unpriced {
        /// The provider it bills.
        provider: &'a str,
        /// The model it bills, when known.
        model: Option<&'a str>,
    },
    /// Its floor exceeds what the cap leaves.
    OverFloor {
        /// The provider it bills.
        provider: &'a str,
        /// The model it bills, when its price depends on one.
        model: Option<&'a str>,
        /// The least the call costs, USD.
        floor_usd: f64,
    },
    /// It is priced, so it costs more than nothing, and the cap leaves
    /// nothing.
    NothingLeft {
        /// The provider it bills.
        provider: &'a str,
        /// The model it bills, when its price depends on one.
        model: Option<&'a str>,
    },
}

impl MediaCapRefusal<'_> {
    /// The refusal in words, for `tool`, under a cap that leaves
    /// `remaining_usd`: the tool, its provider and model, and why.
    #[must_use]
    pub fn reason(&self, tool: &str, remaining_usd: f64) -> String {
        let callee = |provider: &str, model: Option<&str>| match model {
            Some(model) => format!("{tool} on `{provider}` (`{model}`)"),
            None => format!("{tool} on `{provider}`"),
        };
        match *self {
            Self::Unpriced { provider, model } => format!(
                "{} has no catalog price, so the run's cost cap cannot bound it",
                callee(provider, model)
            ),
            Self::OverFloor {
                provider,
                model,
                floor_usd,
            } => format!(
                "{} costs at least ${floor_usd:.6}, above the ${remaining_usd:.6} left under \
                 the run's cost cap",
                callee(provider, model)
            ),
            Self::NothingLeft { provider, model } => format!(
                "{} is billed from the usage it reports, and the run's cost cap has nothing \
                 left (${remaining_usd:.6})",
                callee(provider, model)
            ),
        }
    }
}

/// The price of one paid-media builtin call judged on `args` (literal for
/// the launch floor and the check, rendered for the dispatch guard and the
/// `agent:` loop). `tool` accepts the `nika:` prefix.
#[must_use]
pub fn media_quote<'a>(tool: &str, args: &'a Value) -> MediaQuote<'a> {
    let provider = args.get("provider").and_then(Value::as_str);
    let model = args.get("model").and_then(Value::as_str);
    let decider = provider.or(model);
    // A named provider makes the catalog answer for a paid-media builtin only.
    let paid_tool = nika_catalog::builtin_media_provider(tool, Some(""), None).is_some();
    if !paid_tool {
        return MediaQuote::Unmetered;
    }
    if let Some(template) = decider.filter(|d| is_template(d)) {
        return MediaQuote::RunDecided(template);
    }
    let Some(provider) = nika_catalog::builtin_media_provider(tool, provider, model) else {
        // Neither key names a provider: the builtin refuses the arguments
        // before any request.
        return MediaQuote::Unmetered;
    };
    if let Some(template) = model.filter(|m| is_template(m)) {
        // Only the run decides the model: its price too, unless the provider
        // bills alike whatever the model.
        return match nika_catalog::builtin_media_price(tool, provider, None) {
            MediaPrice::Unmetered => MediaQuote::Unmetered,
            MediaPrice::PerCall(per) => priced_per_call(provider, per, args),
            _ => MediaQuote::RunDecided(template),
        };
    }
    match nika_catalog::builtin_media_price(tool, provider, model) {
        MediaPrice::Unmetered => MediaQuote::Unmetered,
        MediaPrice::PerCall(per) => priced_per_call(provider, per, args),
        MediaPrice::Rated(rated) => MediaQuote::Priced {
            provider,
            model: Some(model.unwrap_or(rated.model)),
            floor: rated_floor(rated, args),
        },
        _ => MediaQuote::Unpriced {
            provider,
            model: model.or_else(|| nika_catalog::builtin_media_default_model(tool, provider)),
        },
    }
}

/// A provider priced per image whatever the model (xAI): its price × `n:`.
fn priced_per_call<'a>(provider: &'a str, per: f64, args: &Value) -> MediaQuote<'a> {
    MediaQuote::Priced {
        provider,
        model: None,
        floor: MediaFloor::Usd {
            usd: per * static_n(args),
            rate: None,
        },
    }
}

/// The floor of a call on a model the dated table prices: per image × `n:`
/// at the size class and quality it asks for, or per character × the text it
/// sends; metered when the table prices the model from usage only, or when
/// only the run renders the text.
fn rated_floor(rated: nika_catalog::MediaModel, args: &Value) -> MediaFloor {
    let arg = |key: &str| args.get(key).and_then(Value::as_str);
    if rated.tool == "tts_generate" {
        let rate = rated.character_rate();
        let text = arg("text").filter(|text| !is_template(text));
        let usd = text.and_then(|text| {
            let characters = u64::try_from(text.trim().chars().count()).ok()?;
            rated.speech_usd(characters)
        });
        return match usd {
            Some(usd) => MediaFloor::Usd { usd, rate },
            None => MediaFloor::Metered { rate },
        };
    }
    let image_size = match args.get("provider_options") {
        // A templated options object may name any class: leave it open.
        Some(Value::String(template)) => Some(template.as_str()),
        Some(options) => options.get("image_size").and_then(Value::as_str),
        None => None,
    };
    let request = ImageRequest::new()
        .with_size(arg("size"))
        .with_aspect_ratio(arg("aspect_ratio"))
        .with_image_size(image_size)
        .with_quality(arg("quality"));
    match rated.image_floor(&request) {
        Some(rate) => MediaFloor::Usd {
            usd: rate.usd * static_n(args),
            rate: Some(rate),
        },
        None => MediaFloor::Metered {
            rate: rated.token_rate(TokenAxis::ImageOutput),
        },
    }
}

/// The provider a paid-media builtin call bills, read from its arguments the
/// way the builtin reads them ([`nika_catalog::builtin_media_provider`]): the
/// explicit `provider:`, else the one its `model:` prefix names. `None` for
/// any other tool, when the arguments name no provider, or when the provider
/// (or the model it would be inferred from) is a template: only the run
/// decides it. The same law judges a literal at check and a rendered call at
/// dispatch.
#[must_use]
pub fn media_call_provider<'a>(tool: &str, args: &'a Value) -> Option<&'a str> {
    let provider = args.get("provider").and_then(Value::as_str);
    let model = args.get("model").and_then(Value::as_str);
    let templated = match provider {
        Some(provider) => is_template(provider),
        None => model.is_some_and(is_template),
    };
    if templated {
        return None;
    }
    nika_catalog::builtin_media_provider(tool, provider, model)
}

/// The template a paid-media builtin call's price is decided by, when only
/// the run can resolve it: a templated `provider:`, a templated `model:` the
/// provider would be inferred from, or a templated `model:` of a provider
/// whose price depends on the model. `None` otherwise and for any other
/// tool. The check lists such a call as decided at run; the run judges it
/// before its request.
#[must_use]
pub fn media_call_template<'a>(tool: &str, args: &'a Value) -> Option<&'a str> {
    match media_quote(tool, args) {
        MediaQuote::RunDecided(template) => Some(template),
        _ => None,
    }
}

/// The paid-media call an armed cap must refuse before its request (C-D3),
/// judged on arguments that are already rendered, against `remaining_usd`
/// (what the cap leaves at call time · a snapshot, never a reservation): a
/// call no catalog rate prices, a floor above what is left, or a priced call
/// when nothing is left. `None` when the call is not paid media, is
/// unmetered, or fits. The runtime's dispatch guard and the `agent:` loop's
/// tool calls speak their refusals from this one verdict.
#[must_use]
pub fn media_cap_refusal<'a>(
    tool: &str,
    args: &'a Value,
    remaining_usd: f64,
) -> Option<MediaCapRefusal<'a>> {
    match media_quote(tool, args) {
        MediaQuote::Unpriced { provider, model } => {
            Some(MediaCapRefusal::Unpriced { provider, model })
        }
        MediaQuote::Priced {
            provider,
            model,
            floor,
        } => match floor.usd() {
            Some(floor_usd) => (micro_usd(floor_usd) > micro_usd(remaining_usd)).then_some(
                MediaCapRefusal::OverFloor {
                    provider,
                    model,
                    floor_usd,
                },
            ),
            None => (micro_usd(remaining_usd) <= 0)
                .then_some(MediaCapRefusal::NothingLeft { provider, model }),
        },
        // A rendered call carries no template; a literal one is the launch
        // gates' to judge.
        MediaQuote::RunDecided(_) | MediaQuote::Unmetered => None,
    }
}

/// Why a returned paid-media call that reported no usable `cost_usd` is
/// absent from the spend (C-D3): the catalog has no price for the model it
/// billed (`missing_catalog_price`), or a priced call did not report or let
/// the builtin compute what it billed (`provider_did_not_report_usage`).
/// `None` for local, mock and keyless media and for any other tool. `value`
/// is the call's output or its arguments: both name the provider and the
/// model the way the builtin resolved them.
#[must_use]
pub fn media_spend_unpriced(tool: &str, value: &Value) -> Option<nika_types::cost::UnpricedReason> {
    match media_quote(tool, value) {
        MediaQuote::Unpriced { .. } => Some(nika_types::cost::UnpricedReason::MissingCatalogPrice),
        MediaQuote::Priced { .. } => {
            Some(nika_types::cost::UnpricedReason::ProviderDidNotReportUsage)
        }
        MediaQuote::RunDecided(_) | MediaQuote::Unmetered => None,
    }
}

pub(crate) fn is_template(s: &str) -> bool {
    s.contains("${{")
}

/// The images a call asks for: a literal `n:` (at least 1), else 1 — the
/// least a templated `n:` can render to.
pub(crate) fn static_n(args: &Value) -> f64 {
    #[allow(clippy::cast_precision_loss)] // image `n:` is capped at 10
    args.get("n")
        .and_then(Value::as_u64)
        .map_or(1.0, |n| n.max(1) as f64)
}

/// Whole micro-USD (1e-6) — the run ledger's budget grain: a floor one ULP
/// over what is left never refuses.
fn micro_usd(usd: f64) -> i64 {
    // REASON: floors and budgets are operator-scale dollars (≪ the i64 micro
    // range) and finite by construction (catalog rates · validated budgets).
    #[allow(clippy::cast_possible_truncation)]
    {
        (usd * 1_000_000.0).round() as i64
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use nika_types::cost::UnpricedReason;

    const IMAGE: &str = "nika:image_generate";
    const SPEECH: &str = "nika:tts_generate";

    fn floor_usd(tool: &str, args: &Value) -> f64 {
        match media_quote(tool, args) {
            MediaQuote::Priced { floor, .. } => floor.usd().expect("a known floor"),
            other => panic!("{args}: {other:?}"),
        }
    }

    fn approx(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{expected} ≠ {actual}");
    }

    #[test]
    fn a_templated_paid_media_provider_or_model_is_decided_at_run() {
        let args = json!({ "provider": "${{ item }}", "prompt": "a star" });
        assert_eq!(media_call_template(IMAGE, &args), Some("${{ item }}"));
        let args = json!({ "model": "${{ inputs.model }}", "text": "hi" });
        assert_eq!(
            media_call_template(SPEECH, &args),
            Some("${{ inputs.model }}")
        );
        // A literal provider whose price depends on the model: the model
        // only the run renders decides the price.
        let args =
            json!({ "provider": "elevenlabs", "model": "${{ inputs.voice }}", "text": "hi" });
        assert_eq!(
            media_call_template(SPEECH, &args),
            Some("${{ inputs.voice }}")
        );
        // xAI bills alike whatever the model; mock never bills.
        for args in [
            json!({ "provider": "xai", "model": "${{ inputs.m }}", "prompt": "a star" }),
            json!({ "provider": "mock", "model": "${{ inputs.m }}", "prompt": "a star" }),
            json!({ "provider": "openai", "prompt": "a star" }),
            json!({ "model": "gpt-image-1", "prompt": "a star" }),
            json!({ "prompt": "a star" }),
        ] {
            assert_eq!(media_call_template(IMAGE, &args), None, "{args}");
        }
        let args = json!({ "provider": "${{ item }}" });
        assert_eq!(
            media_call_template("nika:image_fx", &args),
            None,
            "not a paid-media builtin"
        );
    }

    /// The default models are priced from the dated table: `OpenAI`'s
    /// `gpt-image-2` at its listed size and quality, `quality: auto` at the
    /// cheapest it lists for the size; Gemini's default at its 1K class.
    #[test]
    fn image_floors_read_the_dated_table() {
        approx(
            floor_usd(IMAGE, &json!({ "provider": "openai", "prompt": "x" })),
            0.005,
        );
        approx(
            floor_usd(
                IMAGE,
                &json!({ "provider": "openai", "size": "1024x1024", "quality": "high", "n": 2 }),
            ),
            0.422,
        );
        approx(
            floor_usd(
                IMAGE,
                &json!({ "model": "gpt-image-2", "size": "1024x1024" }),
            ),
            0.006,
        );
        approx(
            floor_usd(IMAGE, &json!({ "provider": "gemini", "prompt": "x" })),
            0.067,
        );
        approx(
            floor_usd(
                IMAGE,
                &json!({ "model": "gemini-3-pro-image", "provider_options": { "image_size": "4K" } }),
            ),
            0.24,
        );
        approx(
            floor_usd(IMAGE, &json!({ "provider": "xai", "n": 3 })),
            0.06,
        );
        // A templated quality, size or `n:` floors at the least it can render to.
        approx(
            floor_usd(
                IMAGE,
                &json!({ "provider": "openai", "quality": "${{ inputs.q }}", "size": "${{ inputs.s }}", "n": "${{ inputs.n }}" }),
            ),
            0.005,
        );
        approx(
            floor_usd(
                IMAGE,
                &json!({ "provider": "gemini", "provider_options": "${{ inputs.options }}" }),
            ),
            0.045,
        );
    }

    /// GPT Image 2.5 is priced from the usage it reports only: metered, with
    /// its image output rate, and no floor before the call.
    #[test]
    fn a_token_priced_image_model_is_metered() {
        let args = json!({ "model": "gpt-image-2.5-flare", "prompt": "x" });
        match media_quote(IMAGE, &args) {
            MediaQuote::Priced {
                provider,
                model,
                floor,
            } => {
                assert_eq!((provider, model), ("openai", Some("gpt-image-2.5-flare")));
                assert_eq!(floor.usd(), None);
                approx(floor.rate().expect("its output rate").usd, 30.0);
            }
            other => panic!("{other:?}"),
        }
    }

    /// Speech floors per character of the text it sends (trimmed, as the
    /// builtin sends it); a text only the run renders is metered.
    #[test]
    fn speech_floors_per_character() {
        approx(
            floor_usd(
                SPEECH,
                &json!({ "provider": "openai", "model": "tts-1", "text": "  hello  " }),
            ),
            15.0 * 5.0 / 1e6,
        );
        approx(
            floor_usd(
                SPEECH,
                &json!({ "provider": "elevenlabs", "text": "hello" }),
            ),
            80.0 * 5.0 / 1e6,
        );
        let templated = json!({ "provider": "elevenlabs", "text": "${{ tasks.script.output }}" });
        assert!(matches!(
            media_quote(SPEECH, &templated),
            MediaQuote::Priced {
                floor: MediaFloor::Metered { .. },
                ..
            }
        ));
    }

    #[test]
    fn unpriced_models_are_named_with_their_default() {
        assert_eq!(
            media_quote(SPEECH, &json!({ "provider": "openai", "text": "hi" })),
            MediaQuote::Unpriced {
                provider: "openai",
                model: Some("gpt-4o-mini-tts")
            }
        );
        assert_eq!(
            media_quote(IMAGE, &json!({ "model": "gpt-image-1.5", "prompt": "x" })),
            MediaQuote::Unpriced {
                provider: "openai",
                model: Some("gpt-image-1.5")
            }
        );
        for args in [
            json!({ "provider": "mock", "prompt": "x" }),
            json!({ "provider": "local", "prompt": "x" }),
            json!({ "provider": "system", "text": "x" }),
        ] {
            assert_eq!(media_quote(IMAGE, &args), MediaQuote::Unmetered, "{args}");
        }
    }

    #[test]
    fn the_cap_refuses_unpriced_media_a_floor_above_what_is_left_and_any_price_at_zero() {
        let speech = json!({ "provider": "openai", "text": "hi" });
        assert_eq!(
            media_cap_refusal(SPEECH, &speech, 100.0),
            Some(MediaCapRefusal::Unpriced {
                provider: "openai",
                model: Some("gpt-4o-mini-tts")
            })
        );
        let unmapped = json!({ "model": "eleven_multilingual_v1", "text": "hi" });
        assert!(matches!(
            media_cap_refusal(SPEECH, &unmapped, 100.0),
            Some(MediaCapRefusal::Unpriced {
                provider: "elevenlabs",
                ..
            })
        ));
        let xai = json!({ "provider": "xai", "prompt": "a star", "n": 3 });
        assert_eq!(media_cap_refusal(IMAGE, &xai, 1.0), None, "$0.06 fits $1");
        match media_cap_refusal(IMAGE, &xai, 0.05).expect("$0.06 exceeds $0.05") {
            MediaCapRefusal::OverFloor {
                provider,
                floor_usd,
                ..
            } => {
                assert_eq!(provider, "xai");
                approx(floor_usd, 0.06);
            }
            other => panic!("{other:?}"),
        }
        let high = json!({ "provider": "openai", "size": "1024x1024", "quality": "high" });
        assert_eq!(media_cap_refusal(IMAGE, &high, 0.211), None, "exactly fits");
        assert!(media_cap_refusal(IMAGE, &high, 0.2).is_some());
        let flare = json!({ "model": "gpt-image-2.5-flare", "prompt": "a star" });
        assert_eq!(
            media_cap_refusal(IMAGE, &flare, 0.01),
            None,
            "metered, fits"
        );
        assert_eq!(
            media_cap_refusal(IMAGE, &flare, 0.0),
            Some(MediaCapRefusal::NothingLeft {
                provider: "openai",
                model: Some("gpt-image-2.5-flare")
            })
        );
        for args in [
            json!({ "provider": "mock", "prompt": "a star" }),
            json!({ "provider": "local", "prompt": "a star" }),
            json!({ "provider": "system", "text": "hi" }),
        ] {
            assert_eq!(media_cap_refusal(IMAGE, &args, 0.0), None, "{args}");
        }
    }

    #[test]
    fn refusals_read_in_words() {
        let unpriced = MediaCapRefusal::Unpriced {
            provider: "openai",
            model: Some("gpt-4o-mini-tts"),
        };
        assert_eq!(
            unpriced.reason(SPEECH, 1.0),
            "nika:tts_generate on `openai` (`gpt-4o-mini-tts`) has no catalog price, so the \
             run's cost cap cannot bound it"
        );
        let over = MediaCapRefusal::OverFloor {
            provider: "xai",
            model: None,
            floor_usd: 0.06,
        };
        assert_eq!(
            over.reason(IMAGE, 0.05),
            "nika:image_generate on `xai` costs at least $0.060000, above the $0.050000 left \
             under the run's cost cap"
        );
    }

    #[test]
    fn returned_paid_media_without_spend_names_why_it_is_unpriced() {
        assert_eq!(
            media_spend_unpriced(
                IMAGE,
                &json!({ "provider": "gemini", "model": "gemini-2.0-flash-preview-image-generation" })
            ),
            Some(UnpricedReason::MissingCatalogPrice)
        );
        assert_eq!(
            media_spend_unpriced(
                IMAGE,
                &json!({ "provider": "gemini", "model": "gemini-3.1-flash-image" })
            ),
            Some(UnpricedReason::ProviderDidNotReportUsage)
        );
        assert_eq!(
            media_spend_unpriced(IMAGE, &json!({ "provider": "xai" })),
            Some(UnpricedReason::ProviderDidNotReportUsage)
        );
        assert_eq!(
            media_spend_unpriced(SPEECH, &json!({ "provider": "mock" })),
            None
        );
        assert_eq!(
            media_spend_unpriced("nika:fetch", &json!({ "provider": "openai" })),
            None
        );
    }
}
