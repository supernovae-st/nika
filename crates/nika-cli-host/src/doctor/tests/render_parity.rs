//! The doctor byte oracle: human `render` and the machine `render_json`
//! through their unchanged public signatures. Every expected digest and
//! literal below was captured by running the immutable pre-move renderer
//! on these same inputs, never by the renderer under
//! test. One group digest frames each case as `name NUL len NUL bytes`.

use sha2::{Digest as _, Sha256};

use super::*;
use crate::probe::AdoptionState;

fn themes() -> [(&'static str, Theme); 5] {
    [
        ("plain", Theme::new(false, false, false)),
        ("color", Theme::new(true, false, false)),
        ("ascii", Theme::new(false, true, false)),
        ("color_ascii", Theme::new(true, true, false)),
        ("animate", Theme::new(true, false, true)),
    ]
}

fn row(level: Level, label: &str, detail: &str, fix: Option<&str>) -> Finding {
    Finding {
        level,
        label: label.to_owned(),
        detail: detail.to_owned(),
        fix: fix.map(str::to_owned),
    }
}

/// Empty · one per level · every level mixed with escapes, multibyte,
/// newlines, a label wider than the column and an empty detail.
fn finding_sets() -> Vec<(&'static str, Vec<Finding>)> {
    vec![
        ("empty", vec![]),
        ("ok", vec![row(Level::Ok, "binary", "v0.96.0", None)]),
        (
            "warn",
            vec![row(
                Level::Warn,
                "keys",
                "no key set",
                Some("export MISTRAL_API_KEY=…"),
            )],
        ),
        (
            "fail",
            vec![row(
                Level::Fail,
                "provider",
                "none usable",
                Some("nika doctor --ping"),
            )],
        ),
        (
            "mixed",
            vec![
                row(Level::Ok, "binary", "v0.0.0-tëst ✨", None),
                row(
                    Level::Warn,
                    "config",
                    "a \"quoted\" \\ detail",
                    Some("nika wire claude"),
                ),
                row(
                    Level::Fail,
                    "runtime",
                    "broken\nsecond line\ttab",
                    Some("nika doctor"),
                ),
                row(Level::Ok, "a-label-wider-than-the-column", "", None),
                row(Level::Warn, "café", "équipe · 日本語", Some("")),
                row(Level::Ok, "binary", "v0.96.0 again", Some("nika welcome")),
            ],
        ),
    ]
}

fn census() -> AccessCensus {
    AccessCensus::from_parts(
        &[
            cloud("mistral", "MISTRAL_API_KEY", true),
            cloud("deepseek", "DEEPSEEK_API_KEY", false),
            local("ollama"),
        ],
        vec![],
    )
}

fn receipts_probe() -> Probe {
    Probe {
        models: ModelsProbe::default(),
        version: "0.96.0".to_owned(),
        config_path: None,
        providers: vec![local("ollama")],
        census: AccessCensus::default(),
        clients: vec![
            ClientProbe {
                id: "hermes".to_owned(),
                path: "~/.hermes/config.yaml".to_owned(),
                present: true,
                current: true,
                stale: false,
            },
            ClientProbe {
                id: "cursor".to_owned(),
                path: "~/.cursor/mcp.json".to_owned(),
                present: true,
                current: true,
                stale: false,
            },
        ],
        kits: vec![KitProbe {
            client: "cursor".to_owned(),
            version: "0.106.0".to_owned(),
        }],
        clients_registry: RegistryCoverage::default(),
        image: ImageProbe::default(),
        tts: TtsProbe::default(),
        local_pings: Vec::new(),
        pricing: PricingProbe::default(),
        retention: crate::retention::RetentionConfig::default(),
        retention_notes: vec![],
        recorded_runs: 0,
        tracked_traces: None,
    }
}

fn cases() -> Vec<(String, String)> {
    let states = [
        ("installed", AdoptionState::Installed),
        ("local_detected", AdoptionState::LocalDetected),
        ("local_reachable", AdoptionState::LocalReachable),
        ("key_present", AdoptionState::KeyPresent),
        ("seat_ready", AdoptionState::SeatReady),
        ("real_ready", AdoptionState::RealReady),
    ];
    let receipts = crate::probe::capability_receipts(&receipts_probe());
    let mut out = Vec::new();
    for (f, findings) in finding_sets() {
        for verbose in [false, true] {
            for (t, theme) in themes() {
                let text = render(&findings, verbose, theme);
                let sober = crate::display::vocab::sober(theme, &text);
                out.push((format!("{f}/human/{verbose}/{t}/raw"), text));
                out.push((format!("{f}/human/{verbose}/{t}/sober"), sober));
            }
        }
        for (s, state) in states {
            for (r, receipts) in [("none", &[][..]), ("hosts", &receipts[..])] {
                for (c, census) in [("empty", AccessCensus::default()), ("paths", census())] {
                    let json = render_json(&findings, state, receipts, &census);
                    out.push((format!("{f}/json/{s}/{r}/{c}"), json));
                }
            }
        }
    }
    out
}

/// `(group, cases, sha256)` per leading two name segments, in order.
fn groups(cases: &[(String, String)]) -> Vec<(String, usize, String)> {
    let mut out: Vec<(String, usize, Sha256)> = Vec::new();
    for (name, text) in cases {
        let key = name.splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
        if out.last().is_none_or(|(k, _, _)| *k != key) {
            out.push((key, 0, Sha256::new()));
        }
        if let Some((_, n, hasher)) = out.last_mut() {
            *n += 1;
            hasher.update(name.as_bytes());
            hasher.update([0]);
            hasher.update(text.len().to_string().as_bytes());
            hasher.update([0]);
            hasher.update(text.as_bytes());
        }
    }
    out.into_iter()
        .map(|(k, n, h)| (k, n, format!("{:x}", h.finalize())))
        .collect()
}

/// `(group, cases, sha256)` captured from the pre-move code; never recomputed by the code under test.
const EXPECTED: &[(&str, usize, &str)] = &[
    (
        "empty/human",
        20,
        "ae26b716b02fb4e3303244aa5fb8714757cae19f8cb41e6590ac99aa2c3ce977",
    ),
    (
        "empty/json",
        24,
        "2c1a62c0b34128cdb51a7c63ecec1c00f00a14ffeda0385040c4c5faf16d839c",
    ),
    (
        "ok/human",
        20,
        "fcf3040f97eb9d9b6c9fd97cd2a1f07c879290a773a568e9045c13420f81a32c",
    ),
    (
        "ok/json",
        24,
        "6bed50bf43a5cbfaf7193a614c10f012221fbb0e74f0b44b8d8d67d1d663f4b5",
    ),
    (
        "warn/human",
        20,
        "336e4f11fc5cff20b67318f18c282a2b28ecc49fb2ff99d54f8a73960f0d3d4e",
    ),
    (
        "warn/json",
        24,
        "26e5fd79f4385a98b67d6fb30e5167ea38eaab4e40f57a28cabea309bce46ef5",
    ),
    (
        "fail/human",
        20,
        "85be24dee6ad33bcee83beb5bb9d27f86d72227ea856d0fe284410ed4ca2d8ff",
    ),
    (
        "fail/json",
        24,
        "7fefa06fb7fe003b5b765b8ecc24ca699257a9c02f5f90cca0986032414c2eb3",
    ),
    (
        "mixed/human",
        20,
        "422d09bf8be43e4100035d9ebf0fa6ec027c0077203b6020512f23a97c9ae0ae",
    ),
    (
        "mixed/json",
        24,
        "4e00c65d4c3368554755716b76d98bf0ec7a2d237e0511bd611a1823f6523deb",
    ),
];

#[test]
fn doctor_renders_the_pre_move_bytes() {
    let cases = cases();
    let want: Vec<(String, usize, String)> = EXPECTED
        .iter()
        .map(|(g, n, s)| ((*g).to_owned(), *n, (*s).to_owned()))
        .collect();
    assert_eq!(groups(&cases), want);
}

/// Two whole reports kept literally (pre-move bytes), so a drift names its first differing byte.
#[test]
fn the_mixed_report_keeps_its_exact_pre_move_text() {
    let mixed = &finding_sets()[4].1;
    assert_eq!(
        render(mixed, false, Theme::new(false, false, false)),
        MIXED_HUMAN
    );
    assert_eq!(
        render_json(mixed, AdoptionState::KeyPresent, &[], &census()),
        MIXED_JSON
    );
}

const MIXED_HUMAN: &str = "✖ 3 ok · 2 warn · 1 fail\n✔ binary     v0.0.0-tëst ✨\n✖ runtime    broken\nsecond line\ttab\n  fix: nika doctor\n✔ a-label-wider-than-the-column \n⚠ café       équipe · 日本語\n  fix: \n✔ binary     v0.96.0 again\n  fix: nika welcome\n· advisory   a healthy machine's notes — config defaults · nika doctor --verbose unfolds each\n";

const MIXED_JSON: &str = "{\n  \"access\": {\n    \"best\": \"mistral\",\n    \"paths\": [\n      {\n        \"class\": \"api\",\n        \"configured\": true,\n        \"custody\": \"MISTRAL_API_KEY\",\n        \"fix\": null,\n        \"id\": \"mistral\"\n      },\n      {\n        \"class\": \"api\",\n        \"configured\": false,\n        \"custody\": \"DEEPSEEK_API_KEY\",\n        \"fix\": \"export DEEPSEEK_API_KEY=…\",\n        \"id\": \"deepseek\"\n      },\n      {\n        \"class\": \"local\",\n        \"configured\": true,\n        \"custody\": null,\n        \"fix\": null,\n        \"id\": \"ollama\"\n      }\n    ],\n    \"seats_ready\": [\n      \"mistral\"\n    ]\n  },\n  \"adoption_state\": \"key_present\",\n  \"findings\": [\n    {\n      \"detail\": \"v0.0.0-tëst ✨\",\n      \"fix\": null,\n      \"label\": \"binary\",\n      \"level\": \"ok\"\n    },\n    {\n      \"detail\": \"a \\\"quoted\\\" \\\\ detail\",\n      \"fix\": \"nika wire claude\",\n      \"label\": \"config\",\n      \"level\": \"warn\"\n    },\n    {\n      \"detail\": \"broken\\nsecond line\\ttab\",\n      \"fix\": \"nika doctor\",\n      \"label\": \"runtime\",\n      \"level\": \"fail\"\n    },\n    {\n      \"detail\": \"\",\n      \"fix\": null,\n      \"label\": \"a-label-wider-than-the-column\",\n      \"level\": \"ok\"\n    },\n    {\n      \"detail\": \"équipe · 日本語\",\n      \"fix\": \"\",\n      \"label\": \"café\",\n      \"level\": \"warn\"\n    },\n    {\n      \"detail\": \"v0.96.0 again\",\n      \"fix\": \"nika welcome\",\n      \"label\": \"binary\",\n      \"level\": \"ok\"\n    }\n  ],\n  \"receipts\": [],\n  \"summary\": {\n    \"fail\": 1,\n    \"ok\": 3,\n    \"warn\": 2\n  }\n}";
