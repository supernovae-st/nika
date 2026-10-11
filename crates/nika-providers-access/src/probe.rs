// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The facts a provider probe states, as values: the readiness ladder, the execution locus, a
//! key's auth rung and an opt-in model listing. Probing itself (key presence, local pings and a
//! listing over the http seam) stays in `nika-providers`, which re-exports every item here at its
//! historical `nika_providers::probe` path.

/// One `--ping` observation · a local port either answered a TCP connect
/// within the cap or it did not (no request is ever sent on the socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingState {
    /// The port accepted the connection (round-trip in ms).
    Reachable(u64),
    /// Nothing listening (or slower than the 300ms cap).
    Unreachable,
}

/// Where an EFFECTIVE endpoint executes the inference — the P0-20 fact:
/// « local » is a PROTOCOL (no key · an OpenAI-compat wire), never a
/// topology. Derived from the URL a run would ACTUALLY hit (the operator
/// override when present, else the profile seed) — never from
/// `requires_key`, so a LAN-pointed ollama can no longer render as
/// « zero network ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLocus {
    /// A loopback literal (`localhost` · 127/8 · `::1`) — zero network.
    Loopback,
    /// RFC1918 / link-local / a local hostname (`gpu.lan` · a bare
    /// name) — the operator's own network, off-box.
    Lan,
    /// An off-box endpoint that is NOT the provider's declared default
    /// (an override pointing at a public address).
    Remote,
    /// The provider's declared vendor endpoint (the catalog default).
    Cloud,
    /// Unparseable — never guessed.
    Unknown,
}

impl ExecutionLocus {
    /// The render token every surface shares (`lan` · `remote` …).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Lan => "lan",
            Self::Remote => "remote",
            Self::Cloud => "cloud",
            Self::Unknown => "unknown",
        }
    }

    /// Classify the EFFECTIVE endpoint against the provider's declared
    /// default. `None` (no override) classifies the default itself — a
    /// local seed reads `Loopback`, a vendor seed reads `Cloud`.
    #[must_use]
    pub fn classify(effective: Option<&str>, default: &str) -> Self {
        let url = effective.unwrap_or(default);
        let Some(host) = endpoint_host(url) else {
            return Self::Unknown;
        };
        if nika_types::net::is_exact_loopback_literal(host) {
            return Self::Loopback;
        }
        if is_lan_host(host) {
            return Self::Lan;
        }
        // The vendor's own declared endpoint is the cloud; any OTHER
        // off-box address is an operator-pointed remote.
        match endpoint_host(default) {
            Some(declared) if declared.eq_ignore_ascii_case(host) => Self::Cloud,
            _ => Self::Remote,
        }
    }
}

/// The HOST of a base URL (no port · v6 keeps its brackets) — the same
/// scheme-strip / authority / userinfo grammar as nika-providers' `ping_addr`,
/// minus the port. `None` = unparseable (classifies [`ExecutionLocus::Unknown`]).
fn endpoint_host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if authority.is_empty() {
        return None;
    }
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        return authority.get(..=end);
    }
    authority.split(':').next().filter(|h| !h.is_empty())
}

/// RFC1918 / link-local / ULA literal, or a hostname that can only name
/// the operator's own network (a bare name · `.lan` · `.local` ·
/// `.internal`). Public DNS names are NOT Lan — the loopback call is
/// [`nika_types::net::is_exact_loopback_literal`]'s, upstream of this.
fn is_lan_host(host: &str) -> bool {
    // seam-bypass-ok: pure parse + range math on a held string, zero OS reach
    use std::net::IpAddr;

    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
            IpAddr::V6(v6) => {
                let s = v6.segments();
                // fc00::/7 (unique local) · fe80::/10 (link-local).
                (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
            }
        };
    }
    let name = host.trim_end_matches('.');
    !name.contains('.')
        || [".lan", ".local", ".internal"]
            .iter()
            .any(|tld| name.ends_with(tld))
}

/// The access-class vocabulary rides with the ladder (probe consumers
/// read both from here · L0 re-export, strictly downward).
pub use nika_types::access::AccessClass;

/// Readiness as a LADDER, never a boolean — the P0-5 audit: « modèle
/// reconnu » was rendered as « prêt ». Every rung is its own fact, so no
/// surface ever has to conflate them again; the rungs a probe did not
/// measure stay `None`, never a hopeful default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderReadiness {
    /// The id resolves in the canonical registry (a NAME fact).
    pub recognized: bool,
    /// Key present when the provider requires one (keyless: always) —
    /// CONFIGURED, never « reachable », never « the model is pulled ».
    pub configured: bool,
    /// Filled ONLY by an explicit opt-in probe (`doctor --ping`) —
    /// `None` without one: no implicit network, ever.
    pub reachable: Option<bool>,
    /// Whether an opt-in compatible listing advertises any model. This does
    /// not qualify a selected model or prove an inference will succeed.
    pub model_available: Option<bool>,
    /// The separate protocol/model observation; absent when no probe was requested.
    pub model_listing: Option<ModelListing>,
    /// The vendored pricing snapshot carries list rates for this provider.
    pub priced: bool,
    /// Where the EFFECTIVE endpoint executes (override-aware · P0-20).
    pub execution_locus: ExecutionLocus,
    /// HOW this provider is reached (D-2026-08-04-N1) — the profile's
    /// access class (`local` · `api` · `mock`). A `harness` row arrives
    /// as a probe row of its OWN (the adapter id · `serves` set · R-5c),
    /// still never from a profile; `oauth` lands with its adapter.
    pub access: nika_types::access::AccessClass,
}

impl ProviderReadiness {
    /// Construct (INV-019) — every rung named, in ladder order.
    #[must_use]
    pub fn new(
        recognized: bool,
        configured: bool,
        reachable: Option<bool>,
        model_available: Option<bool>,
        priced: bool,
        execution_locus: ExecutionLocus,
        access: nika_types::access::AccessClass,
    ) -> Self {
        Self {
            recognized,
            configured,
            reachable,
            model_available,
            model_listing: None,
            priced,
            execution_locus,
            access,
        }
    }
}

/// A provider's environment facts — key PRESENCE only, never the value.
/// A harness adapter rides the SAME row (R-5c · one channel, one
/// derivation): the adapter id in `id`, the providers it fronts in
/// `serves`, `readiness.access == AccessClass::Harness`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a probe row is a record of independent facts (key · ACP speaker · structured output · product binary) — a state machine would conflate what doctor, the plan and the dry-run must read apart (W3-F1)"
)]
pub struct ProviderProbe {
    pub id: String,
    pub requires_key: bool,
    pub key_present: bool,
    /// The conventional env var to name in a fix line (e.g.
    /// `ANTHROPIC_API_KEY`).
    pub fix_var: String,
    /// Whether structured output rides the provider's NATIVE
    /// `json_schema` mode (`Profile::supports_response_format` — a
    /// PROVIDER fact).
    pub structured_native: bool,
    /// The readiness ladder (P0-5) — presence/classification facts only;
    /// `reachable` / `model_available` stay `None` until an opt-in probe.
    pub readiness: ProviderReadiness,
    /// The EFFECTIVE base URL a run would hit (override-aware) — config,
    /// not a credential; renderers redact userinfo before printing.
    pub endpoint: String,
    /// The provider ids a HARNESS-class row serves (R-5c) — `["openai"]`
    /// for `codex`. EMPTY on a provider row: a profile serves
    /// exactly its own id, and the resolver reads that from `id`.
    pub serves: Vec<String>,
    /// The PRODUCT binary's presence on PATH — a harness seat's own CLI,
    /// the one an infer-grade seat spawns (`codex` · `claude`) — distinct
    /// from `key_present`, the ACP speaker an agent-grade seat talks to
    /// (W3-F1: the two were one fact, and the dry-run blessed a seat whose
    /// product was gone). A provider row mirrors `key_present`.
    pub product_present: bool,
}

impl ProviderProbe {
    /// Construct (INV-019) — a provider row (`serves` stays empty; a
    /// harness row names them via [`with_serves`](Self::with_serves)).
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        requires_key: bool,
        key_present: bool,
        fix_var: impl Into<String>,
        structured_native: bool,
        readiness: ProviderReadiness,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            requires_key,
            key_present,
            fix_var: fix_var.into(),
            structured_native,
            readiness,
            endpoint: endpoint.into(),
            serves: Vec::new(),
            product_present: key_present,
        }
    }

    /// Name the product binary's presence (a harness row · W3-F1).
    #[must_use]
    pub fn with_product_present(mut self, present: bool) -> Self {
        self.product_present = present;
        self
    }

    /// Name the provider ids a harness-class row serves (R-5c).
    #[must_use]
    pub fn with_serves(mut self, serves: Vec<String>) -> Self {
        self.serves = serves;
        self
    }
}

/// What doctor (and any other surface) must distinguish for a cloud key
/// (B19 / B26 / B27 / I11 / issue 1273): present is not ready, 401 is
/// not 402, `sk-invalid` is not configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyAuth {
    /// No env var is set.
    Absent,
    /// A value is set; format was not judged / HTTP was not dialed.
    Present,
    /// Set, but the value fails the catalog prefix or a minimum length
    /// (`sk-invalid` is this class — never « ready »).
    Implausible,
    /// Live probe: HTTP 401.
    Unauthorized,
    /// Live probe: HTTP 402 (quota / billing).
    PaymentRequired,
    /// Live probe: the endpoint accepted the key (2xx).
    Ok,
}

impl KeyAuth {
    /// The doctor/JSON token (`present` · `401` · `402` · …).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
            Self::Implausible => "implausible",
            Self::Unauthorized => "401",
            Self::PaymentRequired => "402",
            Self::Ok => "ok",
        }
    }

    /// Whether doctor may render this as `configured` / `ready`.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ok | Self::Present)
    }
}

/// Classify a held key value against the catalog prefixes. The value is
/// the caller's to drop — this function never prints it.
#[must_use]
pub fn classify_key_value(prefixes: &[&str], value: &str) -> KeyAuth {
    if value.is_empty() {
        return KeyAuth::Absent;
    }
    let prefix_ok = prefixes.is_empty() || prefixes.iter().any(|p| value.starts_with(p));
    // OpenAI `sk-` / Anthropic `sk-ant-` live keys are far longer than
    // 16; `sk-invalid` (10) is the gauntlet's present-but-fake specimen.
    if !prefix_ok || value.len() < 16 {
        return KeyAuth::Implausible;
    }
    KeyAuth::Present
}

/// Classify a live auth probe's HTTP status. 401 and 402 are first-class
/// rungs; anything else stays `Present` (the value existed, the probe
/// did not prove Ok).
#[must_use]
pub fn classify_http_status(status: u16) -> KeyAuth {
    match status {
        200..=299 => KeyAuth::Ok,
        401 => KeyAuth::Unauthorized,
        402 => KeyAuth::PaymentRequired,
        _ => KeyAuth::Present,
    }
}

/// A model-list response, not evidence that any advertised model can infer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ModelListing {
    /// None when no HTTP response could be judged.
    pub protocol_compatible: Option<bool>,
    /// Observed status, absent on transport failure.
    pub http_status: Option<u16>,
    /// Validated advertised identifiers; never downloaded or invoked.
    pub models: Vec<String>,
    /// Bounded diagnostic without endpoint credentials or response contents.
    pub failure: Option<String>,
}
impl ModelListing {
    /// An unobserved capability with a fixed, non-secret reason.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            protocol_compatible: None,
            http_status: None,
            models: Vec::new(),
            failure: Some(reason.into()),
        }
    }
    /// Whether a compatible listing advertises any model. Incompatible/unobserved
    /// data cannot prove that no model exists.
    #[must_use]
    pub fn available(&self) -> Option<bool> {
        (self.protocol_compatible == Some(true)).then_some(!self.models.is_empty())
    }
}
