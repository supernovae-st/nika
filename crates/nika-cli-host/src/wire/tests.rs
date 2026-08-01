use super::*;
use serde_json::json;

/// #330 · opencode: its OWN shape (`mcp.nika` · type local · argv in
/// `command`) — created project-local, idempotent on re-run, other
/// servers preserved.
#[test]
fn opencode_project_config_created_and_idempotent() {
    let dir = temp_dir("opencode");
    let path = dir.join("opencode.json");
    let first = patch_opencode(&path, false).expect("create");
    assert!(matches!(first, WireAction::Created(_)), "{first:?}");
    let body: Value = read_json(&path).expect("json");
    assert_eq!(body["mcp"]["nika"]["type"], "local");
    assert_eq!(body["mcp"]["nika"]["command"][0], "nika");
    assert_eq!(body["mcp"]["nika"]["command"][1], "mcp");
    assert_eq!(body["mcp"]["nika"]["enabled"], true);
    let second = patch_opencode(&path, false).expect("re-run");
    assert!(matches!(second, WireAction::Current(_)), "{second:?}");
}

#[test]
fn opencode_preserves_other_servers_and_repairs_nika() {
    let dir = temp_dir("opencode-merge");
    let path = dir.join("opencode.json");
    std::fs::write(
            &path,
            r#"{"mcp":{"other":{"type":"remote"},"nika":{"type":"local","command":["nika","mcp","serve"]}},"theme":"dark"}"#,
        )
        .expect("seed");
    let action = patch_opencode(&path, false).expect("repair");
    assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
    let body: Value = read_json(&path).expect("json");
    assert_eq!(
        body["mcp"]["other"]["type"], "remote",
        "unrelated server kept"
    );
    assert_eq!(body["theme"], "dark", "unrelated key kept");
    assert_eq!(body["mcp"]["nika"]["command"][1], "mcp", "argv repaired");
}

/// #330 · hermes: YAML — the Zed contract (create-if-missing ·
/// current-detect · Manual otherwise, file byte-identical).
#[test]
fn hermes_yaml_creates_detects_and_never_rewrites() {
    let dir = temp_dir("hermes");
    let path = dir.join("config.yaml");
    let first = patch_hermes(&path, false).expect("create");
    assert!(matches!(first, WireAction::Created(_)), "{first:?}");
    let second = patch_hermes(&path, false).expect("re-run");
    assert!(matches!(second, WireAction::Current(_)), "{second:?}");
    // A pre-existing file WITHOUT our entry: snippet handed back,
    // bytes untouched (comments/anchors are the operator's).
    let foreign = "# my hermes\nmodel: hermes-4\nmcp_servers:\n  other: { command: x }\n";
    std::fs::write(&path, foreign).expect("seed");
    let third = patch_hermes(&path, false).expect("manual");
    let WireAction::Manual(msg) = &third else {
        unreachable!("expected Manual, got {third:?}");
    };
    assert!(msg.contains("mcp_servers:") && msg.contains("command: nika"));
    let after = std::fs::read_to_string(&path).expect("read");
    assert_eq!(after, foreign, "the operator file stays byte-identical");
}

#[test]
fn vscode_config_uses_stdio_and_mcp_arg() {
    let dir = temp_dir("vscode");
    let path = dir.join("vscode-mcp.json");
    let action = patch_vscode(&path, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)));
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["servers"]["nika"]["type"], "stdio");
    assert_eq!(doc["servers"]["nika"]["command"], "nika");
    assert_eq!(doc["servers"]["nika"]["args"], json!(["mcp"]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cursor_like_config_preserves_other_servers_and_migrates_stale_args() {
    let dir = temp_dir("cursor");
    let path = dir.join("mcp.json");
    std::fs::write(
        &path,
        r#"{
  "mcpServers": {
    "github": { "command": "gh", "args": ["mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
    )
    .expect("fixture");

    let action = patch_cursor_like(&path, "mcpServers", "cursor", false, false).expect("wire");
    assert!(matches!(action, WireAction::Migrated(_)));
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["github"]["command"], "gh");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn codex_config_toml_created_with_mcp_servers_table() {
    let dir = temp_dir("codex-create");
    let path = dir.join("config.toml");
    let action = patch_toml_mcp(&path, "codex", false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)));
    let body = std::fs::read_to_string(&path).expect("read");
    let doc: toml_edit::DocumentMut = body.parse().expect("toml");
    assert_eq!(doc["mcp_servers"]["nika"]["command"].as_str(), Some("nika"));
    let args: Vec<&str> = doc["mcp_servers"]["nika"]["args"]
        .as_array()
        .expect("args array")
        .iter()
        .filter_map(toml_edit::Value::as_str)
        .collect();
    assert_eq!(args, ["mcp"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn codex_config_preserves_other_tables_comments_and_is_idempotent() {
    let dir = temp_dir("codex-preserve");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        r#"# my codex setup
model = "gpt-5.2"

[projects."/Users/me/repo"]
trust_level = "trusted"

[mcp_servers.github]
command = "gh"
args = ["mcp"]
"#,
    )
    .expect("fixture");

    let action = patch_toml_mcp(&path, "codex", false).expect("wire");
    assert!(matches!(action, WireAction::Updated(_)));
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains("# my codex setup"), "comment preserved");
    assert!(body.contains("trust_level = \"trusted\""));
    assert!(body.contains("[mcp_servers.github]"));

    let again = patch_toml_mcp(&path, "codex", false).expect("wire twice");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn codex_stale_serve_args_are_migrated() {
    let dir = temp_dir("codex-migrate");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[mcp_servers.nika]\ncommand = \"nika\"\nargs = [\"mcp\", \"serve\", \"--stdio\"]\n",
    )
    .expect("fixture");

    let action = patch_toml_mcp(&path, "codex", false).expect("wire");
    assert!(matches!(action, WireAction::Migrated(_)));
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains("args = [\"mcp\"]"));
    let _ = std::fs::remove_dir_all(dir);
}

/// Client-doors W1.2 · grok: the shared `[mcp_servers.nika]` TOML shape
/// at `~/.grok/config.toml` — created, comments preserved, idempotent.
#[test]
fn grok_config_toml_created_preserves_comments_and_is_idempotent() {
    let dir = temp_dir("grok");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "# my grok setup\nmodel = \"grok-4-3\"\n\n[compat.claude]\nskills = true\n",
    )
    .expect("fixture");

    let action = patch_toml_mcp(&path, "grok", false).expect("wire");
    assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains("# my grok setup"), "comment preserved");
    assert!(body.contains("[compat.claude]"), "unrelated table kept");
    let doc: toml_edit::DocumentMut = body.parse().expect("toml");
    assert_eq!(doc["mcp_servers"]["nika"]["command"].as_str(), Some("nika"));

    let again = patch_toml_mcp(&path, "grok", false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(dir);
}

/// Wave-3 · copilot: the desired matches THEIR writer byte-shape
/// (tools `["*"]` + type local) — a copilot-added entry reads Current
/// on first contact, never churns. Empirical fixture = the exact
/// bytes `copilot mcp add nika -- nika mcp` wrote (1.0.76 · live).
#[test]
fn copilot_matches_their_own_writer_and_is_idempotent() {
    let home = temp_dir("copilot");
    let path = home.join(".copilot").join("mcp-config.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dotdir");
    std::fs::write(
        &path,
        r#"{"mcpServers":{"nika":{"tools":["*"],"type":"local","command":"nika","args":["mcp"]}}}"#,
    )
    .expect("their writer's bytes");

    let action = patch_copilot(&path, false).expect("wire");
    assert!(
        matches!(action, WireAction::Current(_)),
        "their entry must read Current, got {action:?}"
    );

    std::fs::remove_file(&path).expect("reset");
    let created = patch_copilot(&path, false).expect("create");
    assert!(matches!(created, WireAction::Created(_)));
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["type"], "local");
    assert_eq!(doc["mcpServers"]["nika"]["tools"], json!(["*"]));
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
    let _ = std::fs::remove_dir_all(home);
}

/// Wave-3 · amp: the LITERAL dotted key "amp.mcpServers" at the
/// settings root (their writer's shape) · unrelated settings kept ·
/// JSONC gets the snippet, byte-identical (the Zed contract).
#[test]
fn amp_dotted_key_preserves_settings_and_respects_jsonc() {
    let home = temp_dir("amp");
    let path = home.join(".config").join("amp").join("settings.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(
        &path,
        r#"{"amp.notifications.enabled":true,"amp.mcpServers":{"other":{"command":"x"}}}"#,
    )
    .expect("seed");

    let action = patch_amp(&path, false).expect("wire");
    assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["amp.notifications.enabled"], true, "unrelated kept");
    assert_eq!(doc["amp.mcpServers"]["other"]["command"], "x");
    assert_eq!(doc["amp.mcpServers"]["nika"]["command"], "nika");

    let again = patch_amp(&path, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));

    let jsonc = "{\n  // amp settings\n  \"amp.anthropic.thinking\": true\n}\n";
    std::fs::write(&path, jsonc).expect("jsonc seed");
    let manual = patch_amp(&path, false).expect("manual");
    assert!(
        matches!(&manual, WireAction::Manual(m) if m.contains("amp.mcpServers")),
        "{manual:?}"
    );
    let after = std::fs::read_to_string(&path).expect("read");
    assert_eq!(after, jsonc, "JSONC byte-identical");
    let _ = std::fs::remove_dir_all(home);
}

/// The registry law: `wire all` expands to every concrete client —
/// a variant added to the enum can never be silently dropped. The
/// count is PINNED (anti-drift: the list moved 15→17→19→21): adding
/// a client means updating this pin DELIBERATELY. `All`/`Detected`
/// are doors, never expansion results.
#[test]
fn all_expands_to_every_concrete_target() {
    let expanded = expand_target(WireTarget::All);
    assert_eq!(
        expanded.len(),
        21,
        "the concrete-client pin moved — deliberate?"
    );
    assert_eq!(
        expanded.len(),
        WireTarget::value_variants().len() - 2,
        "every variant minus the two doors"
    );
    assert!(!expanded.contains(&WireTarget::All));
    assert!(!expanded.contains(&WireTarget::Detected));
}

/// Client-doors W3 · kimi: the user-level `~/.kimi-code/mcp.json`
/// (`mcpServers` map · their customization/mcp.md) — created, then
/// idempotent, other servers preserved.
#[test]
fn kimi_mcp_json_created_and_idempotent() {
    let home = temp_dir("kimi");
    let path = home.join(".kimi-code").join("mcp.json");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dotdir");
    std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).expect("seed");

    let action = patch_cursor_like(&path, "mcpServers", "kimi", false, false).expect("wire");
    assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
    assert_eq!(
        doc["mcpServers"]["other"]["command"], "x",
        "other server kept"
    );

    let again = patch_cursor_like(&path, "mcpServers", "kimi", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

/// Client-doors W1.2 · antigravity: the standalone `mcp_config.json`
/// under `~/.gemini/config/` (`mcpServers` root · gcli-migration doc) —
/// nested dirs created, then idempotent.
#[test]
fn antigravity_mcp_config_created_nested_and_idempotent() {
    let home = temp_dir("antigravity");
    let path = home.join(".gemini").join("config").join("mcp_config.json");

    let action = patch_cursor_like(&path, "mcpServers", "antigravity", false, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again =
        patch_cursor_like(&path, "mcpServers", "antigravity", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn current_config_is_idempotent() {
    let dir = temp_dir("idempotent");
    let path = dir.join("mcp.json");
    let _ = patch_cursor_like(&path, "mcpServers", "cursor", false, false).expect("wire");
    let action = patch_cursor_like(&path, "mcpServers", "cursor", false, false).expect("wire");
    assert!(matches!(action, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn zed_config_created_under_context_servers() {
    let dir = temp_dir("zed-create");
    let path = dir.join("settings.json");
    let action = patch_zed(&path, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)));
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["context_servers"]["nika"]["command"], "nika");
    assert_eq!(doc["context_servers"]["nika"]["args"], json!(["mcp"]));
    assert_eq!(doc["context_servers"]["nika"]["env"], json!({}));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn zed_jsonc_settings_are_never_destroyed() {
    // Zed's settings.json is JSONC — the DEFAULT file ships with comments.
    // A naive parse-rewrite would strip them; the contract is: never
    // rewrite a file we cannot round-trip losslessly — hand the user the
    // exact snippet instead.
    let dir = temp_dir("zed-jsonc");
    let path = dir.join("settings.json");
    let jsonc = "{\n  // Zed settings\n  \"theme\": \"One Dark\"\n}\n";
    std::fs::write(&path, jsonc).expect("fixture");

    let action = patch_zed(&path, false).expect("wire");
    assert!(
        matches!(
            &action,
            WireAction::Manual(message)
                if message.contains("context_servers")
                    && message.contains("\"command\": \"nika\"")
        ),
        "expected Manual with the snippet, got {action:?}"
    );
    let body = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        body, jsonc,
        "JSONC file must be byte-identical (never destroyed)"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn zed_preserves_other_context_servers_and_migrates_stale() {
    let dir = temp_dir("zed-migrate");
    let path = dir.join("settings.json");
    std::fs::write(
        &path,
        r#"{
  "theme": "One Dark",
  "context_servers": {
    "github": { "command": "gh", "args": ["mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
    )
    .expect("fixture");

    let action = patch_zed(&path, false).expect("wire");
    assert!(matches!(action, WireAction::Migrated(_)));
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["theme"], "One Dark", "unrelated settings preserved");
    assert_eq!(doc["context_servers"]["github"]["command"], "gh");
    assert_eq!(doc["context_servers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_zed(&path, false).expect("wire twice");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(dir);
}

/// #384 · gemini: `mcpServers` lives inside the SHARED settings.json —
/// every unrelated setting survives, stale argv migrates.
#[test]
fn gemini_settings_preserves_unrelated_keys_and_migrates_stale() {
    let dir = temp_dir("gemini");
    let path = dir.join("settings.json");
    std::fs::write(
        &path,
        r#"{
  "theme": "GitHub",
  "autoAccept": false,
  "mcpServers": {
    "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
    )
    .expect("fixture");

    let action = patch_cursor_like(&path, "mcpServers", "gemini", false, false).expect("wire");
    assert!(matches!(action, WireAction::Migrated(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["theme"], "GitHub", "unrelated settings preserved");
    assert_eq!(doc["autoAccept"], false, "unrelated settings preserved");
    assert_eq!(doc["mcpServers"]["context7"]["command"], "npx");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "gemini", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(dir);
}

/// #384 · lmstudio: the documented dir wins, the cache dir
/// (lmstudio-bug-tracker#1371) is honoured only when it is the ONLY
/// app-created root, and a bare home falls back to the documented
/// default.
#[test]
fn lmstudio_path_resolution_truth_table() {
    let home = temp_dir("lmstudio-home");

    // Bare home — documented default (created on write).
    assert_eq!(
        lmstudio_mcp_path_from(&home),
        home.join(".lmstudio").join("mcp.json")
    );

    // Only the cache root exists — the app lives there, follow it.
    std::fs::create_dir_all(home.join(".cache").join("lm-studio")).expect("cache root");
    assert_eq!(
        lmstudio_mcp_path_from(&home),
        home.join(".cache").join("lm-studio").join("mcp.json")
    );

    // Documented root exists — it wins even beside the cache root.
    std::fs::create_dir_all(home.join(".lmstudio")).expect("documented root");
    assert_eq!(
        lmstudio_mcp_path_from(&home),
        home.join(".lmstudio").join("mcp.json")
    );
    let _ = std::fs::remove_dir_all(home);
}

/// #384 · lmstudio: dedicated `mcp.json` (Cursor-style `mcpServers`) —
/// created then idempotent at the resolved path.
#[test]
fn lmstudio_config_created_and_idempotent() {
    let home = temp_dir("lmstudio-cfg");
    std::fs::create_dir_all(home.join(".lmstudio")).expect("root");
    let path = lmstudio_mcp_path_from(&home);

    let action = patch_cursor_like(&path, "mcpServers", "lmstudio", false, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "lmstudio", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

/// #449 wave-3b · cline: a host IDE's EXISTING extension dir wins (the
/// live hot-reloaded file); bare home falls back to the stable
/// `~/.cline/data/settings/` (the CLI's today + extension's next home).
#[test]
fn cline_path_resolution_truth_table() {
    let home = temp_dir("cline-home");

    // Bare home — the stable cross-generation path (created on write).
    assert_eq!(
        cline_settings_path_from(&home),
        home.join(".cline")
            .join("data")
            .join("settings")
            .join("cline_mcp_settings.json")
    );

    // A host IDE has the extension — its live globalStorage file wins.
    let host_root = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Cursor")
    } else {
        home.join(".config").join("Cursor")
    };
    let ext = host_root
        .join("User")
        .join("globalStorage")
        .join("saoudrizwan.claude-dev");
    std::fs::create_dir_all(&ext).expect("ext dir");
    assert_eq!(
        cline_settings_path_from(&home),
        ext.join("settings").join("cline_mcp_settings.json")
    );
    let _ = std::fs::remove_dir_all(home);
}

/// #449 wave-3b · cline: Cursor-style `mcpServers` record at the
/// resolved path — created with parents, then idempotent.
#[test]
fn cline_config_created_and_idempotent() {
    let home = temp_dir("cline-cfg");
    let path = cline_settings_path_from(&home);

    let action = patch_cursor_like(&path, "mcpServers", "cline", false, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "cline", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

/// #449 wave-3b · continue: the OWN-FILE drop-dir write
/// (`~/.continue/mcpServers/nika.json` · Claude-Desktop shape) — the
/// user's config.yaml is never touched; the reload hint rides Created/
/// Updated and stays OFF the idempotent re-run.
#[test]
fn continue_own_file_created_idempotent_and_hinted() {
    let home = temp_dir("continue");
    let path = home.join(".continue").join("mcpServers").join("nika.json");

    let action = patch_cursor_like(&path, "mcpServers", "continue", false, false)
        .map(|a| with_hint(a, "reload Continue"))
        .expect("wire");
    assert!(
        matches!(&action, WireAction::Created(label) if label.contains("reload Continue")),
        "created carries the hint: {action:?}"
    );
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "continue", false, false)
        .map(|a| with_hint(a, "reload Continue"))
        .expect("re-run");
    assert!(
        matches!(&again, WireAction::Current(label) if !label.contains("reload")),
        "current stays bare: {again:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// #449 · claude-desktop: the app-config dir differs per platform —
/// macOS `Library/Application Support/Claude/`, elsewhere
/// `.config/Claude/` — and is NEVER Claude Code's `~/.claude.json`.
#[test]
fn claude_desktop_path_is_the_app_config_dir() {
    let home = Path::new("/probe-home");
    let path = claude_desktop_config_path_from(home);
    let expected = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
    } else {
        home.join(".config").join("Claude")
    };
    assert_eq!(path, expected.join("claude_desktop_config.json"));
}

/// #449 · claude-desktop: `mcpServers` root (the modelcontextprotocol.io
/// quickstart shape) — created with parent dirs, then idempotent, other
/// servers preserved.
#[test]
fn claude_desktop_config_created_and_idempotent() {
    let home = temp_dir("claude-desktop");
    let path = claude_desktop_config_path_from(&home);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("app dir");
    std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).expect("seed");

    let action =
        patch_cursor_like(&path, "mcpServers", "claude-desktop", false, false).expect("wire");
    assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
    assert_eq!(doc["mcpServers"]["other"]["command"], "x");

    let again =
        patch_cursor_like(&path, "mcpServers", "claude-desktop", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

/// #449 · qwen: gemini-cli fork — same `mcpServers` key in a user-scope
/// `settings.json`, its own `.qwen` dotdir. Created then idempotent.
#[test]
fn qwen_settings_created_and_idempotent() {
    let home = temp_dir("qwen");
    let path = home.join(".qwen").join("settings.json");

    let action = patch_cursor_like(&path, "mcpServers", "qwen", false, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "qwen", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(home);
}

/// #384 · junie: project-scope `.junie/mcp/mcp.json` — nested dirs
/// created, `mcpServers` root, idempotent.
#[test]
fn junie_project_config_created_nested_and_idempotent() {
    let project = temp_dir("junie");
    let path = project.join(".junie").join("mcp").join("mcp.json");

    let action = patch_cursor_like(&path, "mcpServers", "junie", false, false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    let doc = read_json(&path).expect("json");
    assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
    assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

    let again = patch_cursor_like(&path, "mcpServers", "junie", false, false).expect("re-run");
    assert!(matches!(again, WireAction::Current(_)));
    let _ = std::fs::remove_dir_all(project);
}

/// H7 · audit UX 2026-07-30: `wire all` rewrote every known client
/// config with no preview and no consent. The honest door is
/// preview (`--dry-run`) → detected (`wire detected`) → one client;
/// `all` asks on a terminal and refuses in a pipe without `--yes`.
#[test]
fn dry_run_lists_the_plan_and_writes_nothing() {
    let dir = temp_dir("dry-run");
    let dir_str = dir.to_str().expect("utf8 temp dir");
    let out = run_with(
        WireTarget::Vscode,
        dir_str,
        WireOptions {
            dry_run: true,
            ..WireOptions::default()
        },
    );
    assert_eq!(out.code, crate::output::exit::OK, "{out:?}");
    assert!(
        out.text.contains("dry-run"),
        "the plan says it is one: {out:?}"
    );
    assert!(
        out.text.contains("would create") && out.text.contains(".vscode"),
        "the plan names the action and the path: {out:?}"
    );
    assert!(
        !dir.join(".vscode").join("mcp.json").exists(),
        "a preview never writes"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// H7 · dry-run on an already-wired client reports `current` and
/// leaves the bytes untouched (no mtime-noise rewrite either).
#[test]
fn dry_run_reports_current_and_keeps_bytes_identical() {
    let dir = temp_dir("dry-run-current");
    let path = dir.join("opencode.json");
    let wired = r#"{"mcp":{"nika":{"type":"local","command":["nika","mcp"],"enabled":true}}}"#;
    std::fs::write(&path, wired).expect("seed");
    let out = run_with(
        WireTarget::Opencode,
        dir.to_str().expect("utf8"),
        WireOptions {
            dry_run: true,
            ..WireOptions::default()
        },
    );
    assert_eq!(out.code, crate::output::exit::OK, "{out:?}");
    assert!(out.text.contains("current"), "{out:?}");
    let after = std::fs::read_to_string(&path).expect("read");
    assert_eq!(after, wired, "byte-identical under a preview");
    let _ = std::fs::remove_dir_all(dir);
}

/// H7 · `detected`: only the clients THIS machine shows (the probe's
/// presence truth) — an absent client's config is never created.
#[test]
fn detected_targets_cover_only_present_clients() {
    let home = temp_dir("detected-home");
    let dir = temp_dir("detected-dir");
    assert!(
        detected_targets_in(&home, &dir).is_empty(),
        "a bare machine detects nothing"
    );

    std::fs::create_dir_all(home.join(".cursor")).expect("dir");
    std::fs::write(home.join(".cursor").join("mcp.json"), "{}").expect("seed");
    std::fs::write(home.join(".claude.json"), "{}").expect("seed");
    assert_eq!(
        detected_targets_in(&home, &dir),
        vec![WireTarget::Cursor, WireTarget::Claude],
        "probe order · present only"
    );

    std::fs::create_dir_all(dir.join(".vscode")).expect("dir");
    std::fs::write(dir.join(".vscode").join("mcp.json"), "{}").expect("seed");
    let found = detected_targets_in(&home, &dir);
    assert_eq!(found.len(), 3);
    assert!(found.contains(&WireTarget::Vscode), "workspace-scope seen");
    assert!(!found.contains(&WireTarget::Windsurf), "absent stays out");
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(dir);
}

/// H7 · the consent gate is PURE (the `model pull` confirm-gate
/// precedent): dry-run or `--yes` proceeds, a terminal prompts, a
/// bare pipe is refused.
#[test]
fn all_consent_truth_table() {
    assert_eq!(all_consent(true, false, false), AllConsent::Proceed);
    assert_eq!(all_consent(false, true, false), AllConsent::Proceed);
    assert_eq!(all_consent(false, false, true), AllConsent::Prompt);
    assert_eq!(all_consent(false, false, false), AllConsent::Refuse);
}

/// H7 · a bare `wire all` in a pipe refuses HONESTLY: exit 3, and
/// the refusal names the right door (preview · detected · --yes).
#[test]
fn wire_all_without_consent_is_refused_naming_the_path() {
    let out = run_with(WireTarget::All, ".", WireOptions::default());
    assert_eq!(out.code, crate::output::exit::ENV, "{out:?}");
    assert!(out.text.contains("--dry-run"), "{out:?}");
    assert!(out.text.contains("wire detected"), "{out:?}");
    assert!(out.text.contains("--yes"), "{out:?}");
}

/// H7 · `wire all --dry-run` is the preview — no consent needed,
/// nothing written (the temp workspace stays empty). The exit code
/// PREDICTS the run's: a client whose existing config is unreadable
/// is a `✖ skipped` line and flips the code to ENV — on a machine
/// with all-healthy configs the preview exits OK.
#[test]
fn wire_all_dry_run_previews_without_consent() {
    let dir = temp_dir("all-dry-run");
    let out = run_with(
        WireTarget::All,
        dir.to_str().expect("utf8"),
        WireOptions {
            dry_run: true,
            ..WireOptions::default()
        },
    );
    assert!(
        out.code == crate::output::exit::OK
            || (out.code == crate::output::exit::ENV && out.text.contains("skipped")),
        "the exit code predicts the run's (skips included): {out:?}"
    );
    assert!(out.text.contains("dry-run"), "{out:?}");
    assert!(
        out.text.contains("opencode.json"),
        "the workspace-scope client is planned: {out:?}"
    );
    assert!(
        !dir.join("opencode.json").exists(),
        "a preview never writes"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[allow(clippy::disallowed_methods)]
fn temp_dir(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
        || {
            std::env::current_dir()
                .expect("current dir")
                .join("target")
                .join("tmp")
        },
        PathBuf::from,
    );
    let dir = base.join(format!("nika-wire-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// The atomic write (audit 2026-07-31): every config write lands via a
/// same-dir temp file + fsync + rename — a crash mid-write can no
/// longer destroy the user's whole config. A NEW file is 0600 (these
/// configs grow API keys); an existing file keeps its mode through the
/// rename; no temp litter remains.
#[test]
fn atomic_write_creates_0600_preserves_mode_and_leaves_no_litter() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("atomic");
    let path = dir.join("mcp.json");

    // Create → 0600, exact bytes.
    write_atomic(&path, "{\"a\":1}\n").expect("write");
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "a new config is owner-only, got {mode:o}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "{\"a\":1}\n",
        "the content is the post-rename body"
    );

    // Overwrite with a custom mode → the mode survives the rename.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("chmod");
    write_atomic(&path, "{\"a\":2}\n").expect("rewrite");
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode, 0o640, "an existing mode is preserved, got {mode:o}");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "{\"a\":2}\n");

    // The temp file is renamed away — no `.nika-tmp-*` litter.
    let litter: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(".nika-tmp-"))
        .collect();
    assert!(litter.is_empty(), "no temp litter: {litter:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Both wire writers route through the atomic house: a fresh TOML/JSON
/// config lands 0600, and a pre-existing file's mode survives the
/// update (the truncate-in-place era rewrote it at the umask).
#[test]
fn writers_route_through_the_atomic_house() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp_dir("atomic-routes");
    let mode_of = |path: &Path| std::fs::metadata(path).expect("meta").permissions().mode() & 0o777;

    // The TOML writer, create path.
    let toml = dir.join("config.toml");
    let action = patch_toml_mcp(&toml, "codex", false).expect("wire");
    assert!(matches!(action, WireAction::Created(_)), "{action:?}");
    assert_eq!(mode_of(&toml), 0o600, "a new TOML config is owner-only");

    // The JSON writer, create path.
    let json = dir.join("mcp.json");
    write_json(&json, &json!({"mcpServers": {}})).expect("json");
    assert_eq!(mode_of(&json), 0o600, "a new JSON config is owner-only");

    // The JSON writer over a pre-existing 0640 file preserves the mode.
    std::fs::set_permissions(&json, std::fs::Permissions::from_mode(0o640)).expect("chmod");
    write_json(&json, &json!({"mcpServers": {"nika": {"command": "nika"}}})).expect("rewrite");
    assert_eq!(mode_of(&json), 0o640, "an existing mode is preserved");
    let _ = std::fs::remove_dir_all(dir);
}
