// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring authority of one CLI compile: how many requests the authoring seat may be sent.
//! A repair count is not an authority (pack 94); `--authoring-max-calls` is, and its absence is
//! exactly one request. Every seat is counted in invocations; a direct API seat is also counted
//! where its bytes leave, on a single-attempt transport, so a transport retry or a
//! structured-output fallback inside one invocation is a request too. A request past the
//! authority is refused before any byte leaves; an ACP harness is counted in invocations, its own
//! internal requests unknown. Refusals are recorded beside the core's journal, never over it,
//! and no dollar amount is claimed. The core resolves the authority and counts the requests
//! ([`nika_onboard::compile::authority`]); this door reads the flags and words the refusals.

use nika_kernel::http::{HttpError, HttpPostDyn};
use nika_onboard::compile::NativeMode;
use nika_onboard::compile::authority::{Authority, Door, Refusal, Typed, least_requests};

/// The CLI's grant of more authoring requests, and what a request refused past it is told.
const DOOR: Door = Door::new(
    "--authoring-max-calls",
    "authorize more with --authoring-max-calls",
);

/// The authority for these arguments under the resolved strategy, or the refusal of a typed
/// multiplicity it cannot honor, worded with the flag that grants more (never a silent
/// reduction). Implicit defaults run within the authority, and the receipt states where they
/// stopped.
///
/// # Errors
/// Typed repairs or samples whose worst case exceeds the authority, or a typed escalate
/// (outside an edit) or sketch strategy with a single request.
pub(super) fn resolve(
    args: &super::CompileArgs,
    max_calls: Option<u32>,
    strategy: NativeMode,
) -> Result<Authority, String> {
    let mut typed = Typed::new(args.base.is_some())
        .with_samples(args.authoring_samples)
        .with_repairs(args.authoring_repairs);
    if args.authoring_strategy.is_some() {
        typed = typed.with_strategy();
    }
    Authority::resolve(max_calls, strategy, typed, DOOR).map_err(|refusal| match refusal {
        Refusal::Multiplicity {
            needed,
            authorized,
            strategy,
        } => format!(
            "the repairs or samples typed can need {needed} authoring requests under the {} strategy, and {authorized} {} authorized: authorize them with --authoring-max-calls {needed}, or ask for fewer",
            strategy.word(),
            if authorized == 1 { "is" } else { "are" },
        ),
        Refusal::Strategy { strategy, steps } => format!(
            "the {} strategy needs at least {} authoring requests ({steps}): authorize --authoring-max-calls {1} or more",
            strategy.word(), least_requests(strategy)
        ),
        Refusal::Range {
            name,
            typed,
            least,
            most,
        } => {
            let flag = format!("--authoring-{}", name.replace('_', "-"));
            match most {
                Some(most) => format!(
                    "{flag} {typed} is outside {least}..={most}: the compiler would run another count; type one it runs as typed"
                ),
                None => format!(
                    "{flag} {typed} authorizes no authoring request: authorize {least} or more, or drop --authoring-model"
                ),
            }
        }
        other => format!("the authoring authority refuses this configuration: {other:?}"),
    })
}

/// The transport every door that seats a direct API authoring model sends through (the CLI's
/// compile, Serve's native seat): the runtime's provider composition (no SSRF floor, so a local
/// seat binds; its 600 s transport ceiling above every per-call deadline) with protocol-NACK
/// retries off and no redirect followed, so one POST is one request. Wrap it in the core's
/// `authority::Wire` to count and bound those requests.
///
/// # Errors
/// [`HttpError`] when the client cannot be built, or when it would retry on its own.
// `HttpConfig` is `#[non_exhaustive]`: field assignment, not a struct literal.
#[allow(clippy::field_reassign_with_default)]
pub fn authoring_http() -> Result<impl HttpPostDyn, HttpError> {
    let mut config = nika_http::HttpConfig::default();
    config.ssrf = nika_http::SsrfMode::Disabled;
    config.timeout = std::time::Duration::from_secs(600);
    config.retry_protocol_nacks = false;
    config.max_redirects = 0;
    let http = nika_http::ReqwestHttp::with_config(config)?;
    if !http.supports_single_attempt() {
        return Err(HttpError::Other {
            reason: "the authoring transport would retry on its own; refused".to_owned(),
        });
    }
    Ok(http)
}

pub use nika_providers::authoring::{authoring_backend, authoring_host, redact_authoring_error};

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use serde_json::json;

    #[test]
    fn authoring_endpoints_report_configuration_without_secrets_or_price_claims() {
        use nika_providers::{ProviderRegistry, ProvidersConfig};
        let normal = ProviderRegistry::without_http(ProvidersConfig::new());
        let direct = authoring_backend(&normal, "deepseek/deepseek-chat");
        assert_eq!(direct["base_url_overridden"], false);
        assert_eq!(direct["endpoint_basis"], "operator_configuration");
        let custom = ProviderRegistry::without_http(ProvidersConfig::new().with_base_url(
            "deepseek", "https://test-user:test-sentinel@gateway.invalid:8443/private?credential=test-sentinel#test-sentinel",
        ));
        let described = authoring_backend(&custom, "deepseek/deepseek-chat");
        assert_eq!(described["host"], "gateway.invalid:8443");
        assert_eq!(described["base_url_overridden"], true);
        assert_eq!(described["cost_basis"], "unpriced; billing_unverified");
        assert!(!described.to_string().contains("test-sentinel"));
        assert!(!described.to_string().contains("private"));
        assert_eq!(
            authoring_host("127.0.0.1:1234").as_deref(),
            Some("127.0.0.1:1234")
        );
        assert_eq!(
            authoring_host("http://[::1]:1234/v1").as_deref(),
            Some("[::1]:1234")
        );
        assert_eq!(authoring_host("https://bad host/private"), None);
    }

    #[derive(clap::Parser)]
    struct Door {
        #[command(flatten)]
        args: super::super::CompileArgs,
        #[command(flatten)]
        authority: super::super::AuthoringAuthority,
    }

    fn resolve(argv: &[&str], strategy: NativeMode) -> Result<Authority, String> {
        let seat = ["compile", "x", "--authoring-model", "vllm/m"];
        let door = Door::try_parse_from(seat.iter().chain(argv).copied()).expect("parses");
        super::resolve(&door.args, door.authority.authoring_max_calls, strategy)
    }

    #[test]
    fn an_explicit_multiplicity_is_refused_only_when_the_authority_cannot_honor_it() {
        // No flag: one request, and the implicit defaults run within it.
        let default = resolve(&[], NativeMode::Escalate).expect("one request");
        let record = default.record(&default.envelope(), None);
        assert_eq!(default.max_calls(), 1);
        assert_eq!(record["source"], "default: one request");
        assert_eq!(record["configured"]["worst_case"], 66);
        // Typed repairs under escalate can need sixty-six: refused, with the number to authorize.
        let refused = resolve(&["--authoring-repairs", "3"], NativeMode::Escalate).unwrap_err();
        assert_eq!(
            refused,
            "the repairs or samples typed can need 66 authoring requests under the escalate strategy, and 1 is authorized: authorize them with --authoring-max-calls 66, or ask for fewer"
        );
        let typed = ["--authoring-repairs", "3", "--authoring-max-calls", "66"];
        assert!(resolve(&typed, NativeMode::Escalate).is_ok());
        // Nothing extra asked, and a typed only granted its judgment: never refused.
        let only = ["--authoring-strategy", "only", "--authoring-max-calls", "2"];
        assert!(resolve(&only, NativeMode::Only).is_ok());
        assert!(resolve(&["--authoring-repairs", "0"], NativeMode::Escalate).is_ok());
        // Samples: twenty-one under escalate for three of them.
        assert!(resolve(&["--authoring-samples", "3"], NativeMode::Escalate).is_err());
        let sampled = ["--authoring-samples", "3", "--authoring-max-calls", "21"];
        assert!(resolve(&sampled, NativeMode::Escalate).is_ok());
        // A typed escalation needs the plan and its judgment; the implicit one runs.
        let escalate = resolve(&["--authoring-strategy", "escalate"], NativeMode::Escalate);
        let refused = escalate.expect_err("two requests at least");
        assert_eq!(
            refused,
            "the escalate strategy needs at least 2 authoring requests (the plan, then its judgment): authorize --authoring-max-calls 2 or more"
        );
        let escalate = [
            "--authoring-strategy",
            "escalate",
            "--authoring-max-calls",
            "2",
        ];
        assert!(resolve(&escalate, NativeMode::Escalate).is_ok());
        // A typed sketch needs the sketch, its fills and their judgment.
        let refused = resolve(&["--authoring-strategy", "sketch"], NativeMode::Sketch);
        assert_eq!(
            refused.expect_err("three requests at least"),
            "the sketch strategy needs at least 3 authoring requests (the sketch, its fills, then their judgment): authorize --authoring-max-calls 3 or more"
        );
        let sketch = [
            "--authoring-strategy",
            "sketch",
            "--authoring-max-calls",
            "3",
        ];
        assert!(resolve(&sketch, NativeMode::Sketch).is_ok());
        // The flag needs the authoring seat and one request at least.
        assert!(Door::try_parse_from(["compile", "x", "--authoring-max-calls", "2"]).is_err());
        let zero = [
            "compile",
            "x",
            "--authoring-model",
            "vllm/m",
            "--authoring-max-calls",
            "0",
        ];
        assert!(Door::try_parse_from(zero).is_err());
    }

    #[test]
    fn a_count_the_compiler_would_run_as_another_is_refused_never_clamped() {
        // The flags refuse counts outside what the compiler runs.
        for typed in [["--authoring-repairs", "9"], ["--authoring-samples", "0"]] {
            let argv = [
                "compile",
                "x",
                "--authoring-model",
                "vllm/m",
                typed[0],
                typed[1],
            ];
            assert!(Door::try_parse_from(argv).is_err(), "{typed:?}");
        }
        // A library caller reaches the same refusals, a grant of zero included.
        let door =
            Door::try_parse_from(["compile", "x", "--authoring-model", "vllm/m"]).expect("parses");
        let zero = super::super::AuthoringAuthority::default().with_max_calls(0);
        let refused = super::resolve(&door.args, zero.authoring_max_calls, NativeMode::Escalate);
        assert_eq!(
            refused.expect_err("zero"),
            "--authoring-max-calls 0 authorizes no authoring request: authorize 1 or more, or drop --authoring-model"
        );
        let mut args = door.args;
        args.authoring_repairs = Some(9);
        let refused = super::resolve(&args, None, NativeMode::Escalate).expect_err("nine");
        assert!(
            refused.starts_with("--authoring-repairs 9 is outside 0..=5"),
            "{refused}"
        );
    }

    #[test]
    fn the_shared_authoring_transport_makes_one_attempt_per_post() {
        let http = super::authoring_http().expect("the authoring transport builds");
        assert!(http.supports_single_attempt());
    }

    #[test]
    fn a_library_caller_builds_the_command_and_its_authority_without_literals() {
        let default = super::super::AuthoringAuthority::new();
        assert_eq!(default.authoring_max_calls, None, "one request by default");
        let door = Door::try_parse_from(["compile", "x"]).expect("parses");
        let two = default.with_max_calls(2);
        let command = super::super::CompileCommand::new(door.args, two);
        assert_eq!(command.authority.authoring_max_calls, Some(2));
        assert_eq!(command.args.intent.as_deref(), Some("x"));
    }

    #[test]
    fn the_account_states_the_flag_as_its_source_and_a_harness_requests_as_unknown() {
        let typed = ["--authoring-max-calls", "2"];
        let authority = resolve(&typed, NativeMode::Escalate).expect("two requests");
        let invocations = authority.envelope();
        // A harness is counted in invocations; its own requests are not claimed.
        let record = authority.record(&invocations, None);
        let unknown = "an ACP harness makes its own requests";
        assert_eq!(
            record["http_requests"],
            json!({"sent": null, "refused": null, "unknown": unknown})
        );
        assert_eq!(record["source"], "--authoring-max-calls");
        assert_eq!(record["invocations"], json!({"sent": 0, "refused": 0}));
    }
}
