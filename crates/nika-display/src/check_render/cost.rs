// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The COST rung of the check card — the priced arm, the composed-only
//! arm, the empty arm and the task rows (split from `check_render.rs` at
//! the 1,500-line wall · wave 3.b; the bodies moved verbatim).

use super::*;

/// The composition arm of [`cost()`] (spec 14 · the 2026-07-29 finding): no
/// OWN inference task, but resolvable children are priced — printing
/// `$0.00` here told the operator a free story about a bill the child
/// explains at `≤$X`. The totals already carry the children; the uncapped
/// count names the composed half only (no own task exists to count).
fn cost_composed_only(out: &mut String, report: &CheckReport, t: Theme) {
    let calls = crate::vocab::count(report.cost.composed.len(), "composed child call");
    if report.cost.has_unbounded {
        let _ = writeln!(
            out,
            " {} {}     {}",
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            t.paint(Role::Strong, "COST"),
            t.paint(
                Role::Warn,
                &format!(
                    "bounded portion ${} · no total ceiling · {} · {}",
                    crate::vocab::usd(report.cost.bounded_total_usd),
                    calls,
                    unbounded_census(report)
                )
            )
        );
    } else {
        let mut money = format!(
            "${} – ${} worst-case output ceiling · {} · own inference $0.00",
            crate::vocab::usd(report.cost.min_path_total_usd),
            crate::vocab::usd(report.cost.bounded_total_usd),
            calls
        );
        // A paid media call (its own, or a child's under its calling task)
        // is outside that ceiling: named here and on its row (C-D3).
        if let Some(media) = media_summary(report) {
            money.push_str(" · ");
            money.push_str(&media);
        }
        let _ = writeln!(
            out,
            " {} {}     {}",
            media_mark(t, unpriced_media(report) > 0),
            t.paint(Role::Strong, "COST"),
            money
        );
    }
    cost_media_rows(out, report, t);
}

/// The COST arm for a workflow with NO own inference task — the `$0.00`
/// that used to claim the whole bill from a lane that prices
/// `infer:`/`agent:` and nothing else; the composed-children arm joins
/// here (extracted under the fn-length law).
fn cost_empty_arm(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if !report.cost.composed.is_empty() {
        return cost_composed_only(out, report, t);
    }
    // An `exec:` runs an arbitrary program, and the programs authors
    // reach for first are billed LLM CLIs; an `mcp:` call is a third
    // party's meter. Measured 2026-07-29: a lone
    // `exec: ["claude", "-p", "write a novel"]` printed
    // `✔ COST no inference tasks · $0.00` and
    // `✔ audited · est ≤$0.0000`. A paid media call is a provider's meter
    // too (C-D3): it is counted here and named on its own row; the host
    // voice's local compute is named unpriced, never free.
    let summary = format!(
        "no infer/agent tasks · {} · exec + mcp spend unpriced{}",
        media_summary(report).unwrap_or_else(|| "$0.00".to_owned()),
        voice_clause(wf)
    );
    let unpriced = unpriced_media(report) > 0;
    let _ = writeln!(
        out,
        " {} {}     {}",
        media_mark(t, unpriced),
        t.paint(Role::Strong, "COST"),
        t.paint(if unpriced { Role::Warn } else { Role::Dim }, &summary)
    );
    cost_media_rows(out, report, t);
}

/// The paid media calls this workflow makes, unpriced ones first:
/// `None` when it makes none.
fn media_summary(report: &CheckReport) -> Option<String> {
    let calls = report.cost.media.len();
    if calls == 0 {
        return None;
    }
    let counted = crate::vocab::count(calls, "paid media call");
    Some(match unpriced_media(report) {
        0 => format!("{counted} at catalog prices"),
        unpriced => format!("{counted}, {unpriced} unpriced"),
    })
}

/// How many paid media calls no catalog rate prices before the run: no
/// catalog price, or a provider or model only the run decides.
fn unpriced_media(report: &CheckReport) -> usize {
    (report.cost.media.iter())
        .filter(|call| !call.priced)
        .count()
}

/// The COST mark: a warning while an unpriced media call hides from the
/// figure beside it (spend-honesty law 1), else the ordinary check mark.
fn media_mark(t: Theme, unpriced: bool) -> String {
    if unpriced {
        t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " })
    } else {
        mark(t, true)
    }
}

/// One row per paid media call: its floor, the catalog rate that sets it and
/// the date that rate was read; METERED for a call billed from the usage it
/// reports (no floor before the call); UNPRICED with what an armed cap does
/// with it (the run's own refusal, NIKA-1709); or DECIDED AT RUN for a
/// provider or model only the run resolves (judged before its request,
/// NIKA-1704).
fn cost_media_rows(out: &mut String, report: &CheckReport, t: Theme) {
    for call in &report.cost.media {
        let callee = match &call.model {
            Some(model) => format!("{} · {} {model}", call.tool, call.provider),
            None => format!("{} · {}", call.tool, call.provider),
        };
        let dated = |rate: &str| match &call.as_of {
            Some(as_of) => format!("{rate} · prices {as_of}"),
            None => rate.to_owned(),
        };
        let rate = call.rate.as_deref().unwrap_or("catalog rate");
        let verdict = match (call.priced, call.usd_per_call) {
            (true, Some(floor)) => format!(
                "from ${} · {}",
                crate::vocab::usd(floor),
                t.paint(Role::Dim, &dated(rate))
            ),
            (true, None) => format!(
                "{} {}",
                t.paint(Role::Strong, "METERED"),
                t.paint(
                    Role::Dim,
                    &format!("— billed from what it reports · {}", dated(rate))
                ),
            ),
            (false, _) if call.run_decided => format!(
                "{} {}",
                t.paint(Role::Warn, "DECIDED AT RUN"),
                t.paint(
                    Role::Dim,
                    "— a cap judges its price before its request (NIKA-1704)"
                ),
            ),
            (false, _) => format!(
                "{} {}",
                t.paint(Role::Warn, "UNPRICED"),
                t.paint(
                    Role::Dim,
                    "— no catalog price; any cost cap refuses it (NIKA-1709)"
                ),
            ),
        };
        let _ = writeln!(
            out,
            "   {}  {}  {verdict}",
            call.task,
            t.paint(Role::Dim, &callee)
        );
    }
}

/// ` · system voice local work unpriced` when a task speaks through the host
/// synthesizer: the `$0.00` above prices `infer:`/`agent:` only, and the
/// voice's local compute is unpriced, never free.
fn voice_clause(wf: &RawWorkflow) -> String {
    crate::check_render::host_voice_unpriced(wf)
        .map(|clause| format!(" · {clause}"))
        .unwrap_or_default()
}

/// The per-task rows of [`cost()`] — a priced row at its cap, or the
/// UNBOUNDED row with its named reason (extracted under the fn-length law).
fn cost_task_rows(out: &mut String, report: &CheckReport, t: Theme) {
    let le = crate::vocab::at_most(t.ascii);
    for c in &report.cost.tasks {
        let model = c.model.as_deref().unwrap_or("?");
        match (&c.usd, &c.unbounded_reason) {
            (Some(worst), _) => {
                let _ = writeln!(
                    out,
                    "   {}  {}  {le}{} tk  ${}",
                    c.task,
                    t.paint(Role::Dim, model),
                    c.max_tokens.unwrap_or(0),
                    crate::vocab::usd(*worst),
                );
            }
            (None, reason) => {
                let why = match reason {
                    Some(UnboundedReason::NoTokenLimit) => "no max_tokens declared",
                    Some(UnboundedReason::NoPrice) => no_price_reason(model),
                    Some(UnboundedReason::UnknownIterations) => {
                        "for_each over an expression (unknown count)"
                    }
                    _ => "unbounded",
                };
                let _ = writeln!(
                    out,
                    "   {}  {}  {} {}",
                    c.task,
                    t.paint(Role::Dim, model),
                    t.paint(Role::Warn, "UNBOUNDED"),
                    t.paint(Role::Dim, &format!("— {why}")),
                );
            }
        }
    }
}

fn no_price_reason(model: &str) -> &'static str {
    let Some((provider, name)) = model
        .split_once('/')
        .and_then(|(id, name)| nika_catalog::find_provider(id).map(|p| (p, name)))
    else {
        return "no catalog price (unknown provider/model; spend unpriced)";
    };
    if provider.tags.contains(&nika_catalog::types::Tag::Local) {
        return "no catalog price (local model; spend not metered)";
    }
    if provider.id == "mock" {
        return "no catalog price (mock model; spend not metered)";
    }
    if provider
        .models
        .iter()
        .any(|m| m.id == name || m.model == name)
        || provider.default_model == name
        || provider.cheap_model == name
    {
        "no catalog price (catalogued cloud model; spend unpriced)"
    } else {
        "no catalog price (cloud provider; spend unpriced)"
    }
}

pub(super) fn cost(
    out: &mut String,
    report: &CheckReport,
    wf: &RawWorkflow,
    t: Theme,
    layers: &VerdictLayers,
) {
    if report.cost.tasks.is_empty() {
        return cost_empty_arm(out, report, wf, t);
    }
    // OUTPUT ceiling, and the word is load-bearing. `cost::ceiling` prices
    // `max_tokens`, which the spec defines as "Max OUTPUT tokens"
    // (02-verbs §infer), against `output_price_per_million`. The prompt is
    // not underweighted in that sum — it is absent, and
    // `input_per_million` has no reader anywhere in `nika-check`.
    //
    // The gap is not academic. Measured 2026-07-28 on the most common shape
    // a person writes first — fetch a document, summarise it: a 3.2 MB body
    // interpolated into one prompt is ~818k input tokens, $2.4563 at that
    // model's published input rate, against a line reading $0.0075. 328x,
    // under a green mark, with the input price sitting four lines above the
    // output price the sum reads in the same catalog block.
    //
    // Pricing it properly needs a static bound on interpolated content,
    // which is real work (the shape is the one `for_each` already uses:
    // literal is known, expression is unbounded). Until that lands the
    // verdict NARROWS instead of overreaching — a claim that covers what it
    // computes is always available, and is the only honest thing to print
    // while the other half has no bound.
    //
    // Unbounded cost is a WARNING posture (is_clean ignores it): the
    // report stays honest about the floor without failing the file.
    // The unbounded arm used to print `$min – $bounded` under the word
    // FLOOR — and `audited_line` documents why both halves are false
    // (`min_path_total_usd` bounds nothing from below · measured 126× the
    // other way). Same decision as there: claim neither bound, show the
    // only true number (the priced portion), and name the uncapped tasks.
    let media_unpriced = unpriced_media(report);
    let (cost_mark, money, bound) = if report.cost.has_unbounded {
        (
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            format!(
                "bounded portion ${}",
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            t.paint(
                Role::Warn,
                &format!("no total ceiling · {}", unbounded_census(report)),
            ),
        )
    } else {
        (
            media_mark(t, media_unpriced > 0),
            format!(
                "${} – ${}",
                crate::vocab::usd(report.cost.min_path_total_usd),
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            // W3-F4 · a seat-served model is billed to its subscription: the
            // dollar figure is its API counterfactual, said in words. A route that reasons beyond
            // `max_tokens` bounds its visible output only (live gauntlet r04 B7).
            if let Some(beyond) = reasoning_beyond(report) {
                t.paint(
                    Role::Warn,
                    &format!("visible-output ceiling · reasoning of {beyond} billed beyond it"),
                )
            } else if layers.seat_served.is_empty() {
                "worst-case output ceiling".to_owned()
            } else {
                format!(
                    "worst-case output ceiling · seat-served (unmetered): {} — the dollar figure is the API counterfactual",
                    layers.seat_served.join(", ")
                )
            },
        )
    };
    let _ = writeln!(
        out,
        " {cost_mark} {}     {} {bound} {}",
        t.paint(Role::Strong, "COST"),
        t.paint(Role::Strong, &money),
        t.paint(Role::Dim, &priced_tail(wf, media_unpriced)),
    );
    cost_task_rows(out, report, t);
    cost_media_rows(out, report, t);
}

/// The dim tail of the priced COST rung: what its ceiling does not price
/// (`media_unpriced` paid media calls among them), and the date of the price
/// table it does.
fn priced_tail(wf: &RawWorkflow, media_unpriced: usize) -> String {
    // The price table's date rides WITH the number. A ceiling is a
    // promise, and a promise computed against prices that have since
    // moved is a promise about the past — the `--json` lane has carried
    // `pricing.snapshot.as_of` since the models rung shipped, but the
    // human lane never showed it, so the one reader who cannot query the
    // payload was the one who could not tell.
    //
    // This is not hypothetical drift: vendor intro pricing expires on
    // announced dates, so a workflow audited in one month can bill more
    // in the next with the file unchanged.
    //
    // `exec` + `mcp` join `prompts` in the unpriced list for the same
    // reason the empty-cost branch above names them: this ceiling
    // sums `infer:`/`agent:` output tokens, so a workflow that mixes
    // an `infer:` with an `exec:` gets a ceiling with the exec's whole
    // bill missing — and the ✔ does not say so. An unpriced paid media
    // call is counted beside them (C-D3), and the host voice's local work
    // is named last.
    let unpriced = match media_unpriced {
        0 => "prompts, exec + mcp".to_owned(),
        n => format!(
            "prompts, exec, mcp + {}",
            crate::vocab::count(n, "paid media call")
        ),
    };
    format!(
        "· {unpriced} unpriced · prices {}{}",
        nika_catalog::pricing_snapshot().as_of,
        voice_clause(wf)
    )
}

/// The tasks whose route bills reasoning beyond `max_tokens`, named, when there are any.
fn reasoning_beyond(report: &nika_check::CheckReport) -> Option<String> {
    let named: Vec<String> = (report.cost.tasks.iter())
        .filter(|task| task.reasoning_beyond)
        .map(|task| format!("`{}`", task.task))
        .collect();
    (!named.is_empty()).then(|| named.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpriced_rows_distinguish_catalogued_cloud_local_and_unknown_models() {
        for (model, expected) in [
            ("moonshot/k2", "catalogued cloud model; spend unpriced"),
            ("moonshot/unlisted-model", "cloud provider; spend unpriced"),
            ("ollama/unlisted-model", "local model; spend not metered"),
            (
                "unlisted-provider/model",
                "unknown provider/model; spend unpriced",
            ),
        ] {
            let source = format!(
                "nika: price-wording\nmodel: {model}\ntasks:\n  say:\n    infer: {{ prompt: hi, max_tokens: 20 }}\n"
            );
            let wf = nika_schema::parse(
                &source,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .expect("fixture parses");
            let report = nika_check::check(&wf);
            assert_eq!(
                report.cost.tasks[0].unbounded_reason,
                Some(UnboundedReason::NoPrice)
            );
            let mut text = String::new();
            cost_task_rows(&mut text, &report, Theme::new(false, false, false));
            assert!(
                text.contains("UNBOUNDED") && text.contains(expected),
                "{text}"
            );
            assert!(
                !text.contains("$0") && !text.contains("local/unknown"),
                "{text}"
            );
        }
    }

    fn media_card(source: &str) -> String {
        let wf = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        let mut text = String::new();
        let layers = VerdictLayers::default();
        cost(
            &mut text,
            &report,
            &wf,
            Theme::new(false, false, false),
            &layers,
        );
        text
    }

    /// C-D3 · a workflow whose only spend is a paid speech call used to print
    /// `✔ COST no infer/agent tasks · $0.00`: the card now names the call as
    /// unpriced, as the run that would refuse it under a cap does.
    #[test]
    fn an_unpriced_media_call_is_named_never_a_free_zero() {
        let text = media_card(
            "nika: speak\npermits: { tools: [\"nika:tts_generate\"] }\ntasks:\n  speak:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: openai, text: hi, output_dir: ./out } }\n",
        );
        assert!(!text.contains("$0.00"), "{text}");
        assert!(
            text.contains("1 paid media call, 1 unpriced")
                && text.contains("speak")
                && text.contains("nika:tts_generate · openai")
                && text.contains("UNPRICED")
                && text.contains("NIKA-1709"),
            "{text}"
        );
        assert!(
            !text.contains('✔'),
            "an unpriced call never sits under a check mark: {text}"
        );
        // The audited line quotes the figure and names what it leaves out.
        let wf = nika_schema::parse(
            "nika: speak\npermits: { tools: [\"nika:tts_generate\"] }\ntasks:\n  speak:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: openai, text: hi, output_dir: ./out } }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let est = crate::check_render::est_clause(
            &nika_check::check(&wf),
            Theme::new(false, false, false),
        );
        assert!(est.ends_with("+ 1 unpriced paid media call"), "{est}");
    }

    /// The inference ceiling keeps its figure and stops hiding the media
    /// call beside it; a priced (xAI) call rides at its catalog rate.
    #[test]
    fn media_rows_join_the_inference_ceiling() {
        let text = media_card(
            "nika: mixed\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:image_generate\"] }\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 100 }\n  cover:\n    invoke: { tool: \"nika:image_generate\", args: { model: gpt-image-1-mini, prompt: a star, output_dir: ./out } }\n  banner:\n    invoke: { tool: \"nika:image_generate\", args: { provider: xai, prompt: a star, output_dir: ./out } }\n",
        );
        assert!(text.contains("1 paid media call unpriced"), "{text}");
        assert!(
            text.contains("nika:image_generate · openai") && text.contains("UNPRICED"),
            "{text}"
        );
        assert!(
            text.contains("nika:image_generate · xai  from $0.0200 · $0.02 per image"),
            "{text}"
        );
    }

    /// C-D3 · a priced call's row shows its floor, the rate that sets it and
    /// the date the rate was read; a call billed from its usage reads
    /// METERED with its rate; nothing priced puts the card under a warning.
    #[test]
    fn priced_media_rows_show_the_floor_its_unit_and_the_table_date() {
        let text = media_card(
            "nika: priced\npermits: { tools: [\"nika:image_generate\", \"nika:tts_generate\"] }\ntasks:\n  poster:\n    invoke: { tool: \"nika:image_generate\", args: { provider: openai, size: 1024x1024, quality: high, prompt: a star, output_dir: ./out } }\n  narrate:\n    invoke: { tool: \"nika:tts_generate\", args: { model: tts-1-hd, text: \"hello world\", output_dir: ./out } }\n  flare:\n    invoke: { tool: \"nika:image_generate\", args: { model: gpt-image-2.5-flare, prompt: a star, output_dir: ./out } }\n",
        );
        let date = nika_catalog::MEDIA_PRICING_AS_OF;
        assert!(
            text.contains(&format!(
                "poster  nika:image_generate · openai gpt-image-2  from $0.2110 · $0.211 per image (1024x1024, high) · prices {date}"
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "narrate  nika:tts_generate · openai tts-1-hd  from $0.0003 · $30 per 1M characters · prices {date}"
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "flare  nika:image_generate · openai gpt-image-2.5-flare  METERED — billed from what it reports · $30 per 1M image output tokens · prices {date}"
            )),
            "{text}"
        );
        assert!(
            text.contains("3 paid media calls at catalog prices"),
            "{text}"
        );
        assert!(text.contains('✔'), "nothing unpriced: {text}");
    }

    /// A provider only the run decides is named, never a free `$0.00` under
    /// a check mark: the run judges it before its request (NIKA-1704).
    #[test]
    fn a_run_decided_media_provider_is_named_never_a_free_zero() {
        let text = media_card(
            "nika: fan\ninputs:\n  who: { type: string, default: mock }\npermits: { tools: [\"nika:image_generate\"] }\ntasks:\n  draw:\n    invoke: { tool: \"nika:image_generate\", args: { provider: \"${{ inputs.who }}\", prompt: a star, output_dir: ./out } }\n",
        );
        assert!(!text.contains("$0.00") && !text.contains('✔'), "{text}");
        assert!(
            text.contains("DECIDED AT RUN") && text.contains("NIKA-1704"),
            "{text}"
        );
        // A templated model of a provider priced per model is the run's too.
        let text = media_card(
            "nika: voice\ninputs:\n  model: { type: string, default: eleven_v3 }\npermits: { tools: [\"nika:tts_generate\"] }\ntasks:\n  say:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: elevenlabs, model: \"${{ inputs.model }}\", text: hi, output_dir: ./out } }\n",
        );
        assert!(text.contains("DECIDED AT RUN"), "{text}");
    }

    /// The composed-only layout (a child call, no own inference) names the
    /// paid media beside its ceiling and drops its check mark while an
    /// unpriced call is outside the figure.
    #[test]
    fn the_composed_only_layout_names_its_paid_media() {
        let source = "nika: parent\npermits: { tools: [\"nika:tts_generate\"] }\ntasks:\n  call:\n    invoke: { workflow: ./child.nika }\n  speak:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: openai, text: hi, output_dir: ./out } }\n";
        let wf = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check_composed(&wf, "parent.nika", &mut |_| {
            Ok("nika: child\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 100 }\n".to_owned())
        });
        assert!(!report.cost.composed.is_empty(), "the child is priced");
        let mut text = String::new();
        let layers = VerdictLayers::default();
        cost(
            &mut text,
            &report,
            &wf,
            Theme::new(false, false, false),
            &layers,
        );
        assert!(text.contains("composed child call"), "{text}");
        assert!(
            text.contains("1 paid media call, 1 unpriced")
                && text.contains("nika:tts_generate · openai")
                && text.contains("UNPRICED"),
            "{text}"
        );
        assert!(!text.contains('✔'), "{text}");
    }

    /// Local and mock media spend nothing: the card is unchanged.
    #[test]
    fn unmetered_media_keeps_the_zero_line() {
        let text = media_card(
            "nika: draft\npermits: { tools: [\"nika:image_generate\"] }\ntasks:\n  draft:\n    invoke: { tool: \"nika:image_generate\", args: { provider: mock, prompt: a star, output_dir: ./out } }\n",
        );
        assert!(
            text.contains("no infer/agent tasks · $0.00 · exec + mcp spend unpriced"),
            "{text}"
        );
        assert!(!text.contains("paid media"), "{text}");
    }

    /// The keyless host voice bills no provider, so it is no paid media call
    /// (no row, no cap refusal), yet its local work is named unpriced beside
    /// the paid media a workflow makes, in both layouts.
    #[test]
    fn the_host_voice_is_named_beside_paid_media_never_counted_as_one() {
        let speech = "permits: { tools: [\"nika:tts_generate\"] }\ntasks:\n  host:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: system, text: hi, output_dir: ./out } }\n  cloud:\n    invoke: { tool: \"nika:tts_generate\", args: { provider: openai, text: hi, output_dir: ./out } }\n";
        let text = media_card(&format!("nika: duo\n{speech}"));
        let line = text.lines().next().unwrap_or_default();
        assert!(
            line.contains(
                "no infer/agent tasks · 1 paid media call, 1 unpriced · exec + mcp spend \
                 unpriced · system voice local work unpriced"
            ),
            "{text}"
        );
        assert!(
            text.contains("nika:tts_generate · openai")
                && !text.contains("nika:tts_generate · system"),
            "only the paid call has a row: {text}"
        );
        let text = media_card(&format!(
            "nika: trio\nmodel: anthropic/claude-sonnet-4-6\n{speech}  ask:\n    infer: {{ prompt: hi, max_tokens: 100 }}\n"
        ));
        let line = text.lines().next().unwrap_or_default();
        assert!(
            line.contains("· prompts, exec, mcp + 1 paid media call unpriced · prices ")
                && line.ends_with("· system voice local work unpriced"),
            "{text}"
        );
    }
}
