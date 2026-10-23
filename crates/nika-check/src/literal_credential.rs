// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A bearer credential written as a literal into an effect's arguments (#1618).
//!
//! The SECRETS gate follows the values `secrets:` declares (the IFC flow facts); a key pasted
//! into a header (`Authorization: Bearer sk-live-…`) is declared nowhere, so that gate stayed
//! green while the file carried the credential and the call sent it. This lane reads the
//! arguments of every `exec:` and `invoke:` effect (an HTTP header is one) for the `Bearer`
//! scheme followed by a literal token, written there or in the `const:` / `with:` value a bare
//! `${{ const.<name> }}` / `${{ with.<name> }}` of the argument names: both are fixed in the
//! file, so the question is decidable. A value bound at run time (a secret, an input, an
//! upstream output, a shell variable) is not a literal, and stays the flow lanes' concern.
//! The token itself is never recorded.

use nika_schema::expression::{TemplateIsland, scan_templates};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use serde_json::Value;

use crate::analyzer::static_args::ConstStrings;

/// The shortest token read as a credential. The live key and token shapes are longer; a word
/// written after `Bearer` in prose (`token` · `authentication`) is shorter.
const MIN_TOKEN: usize = 16;

/// What an island of an argument resolves to when nothing static decides it: a character no
/// token holds, so a token never runs through a value only the run knows.
const BOUND_AT_RUN: char = '\0';

/// A bearer credential written as a literal into an effect's arguments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct LiteralCredential {
    /// The task whose effect sends it.
    pub task: String,
    /// The argument that carries it (`args.headers.Authorization` · `exec.command[2]` ·
    /// `exec.shell` · `exec.stdin` · `exec.env.<NAME>`).
    pub field: String,
    /// The `const.<name>` or `with.<name>` value holding the token, when the argument names one
    /// rather than writing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<String>,
    /// The `egress.to:` value a declared secret needs to reach this effect (the tool id ·
    /// `exec`), named by the repair.
    pub sink_id: String,
    /// The literal destination host the egress pins (a network tool's literal URL host), named
    /// by the repair; `None` when the destination is not a literal host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// Every literal bearer credential the workflow's `exec:` and `invoke:` arguments carry.
#[must_use]
pub(super) fn scan(wf: &RawWorkflow) -> Vec<LiteralCredential> {
    let consts = ConstStrings::of(wf);
    let mut found = Vec::new();
    for task in &wf.tasks {
        let task = &task.value;
        let mut fields = Vec::new();
        match &task.action {
            RawAction::Exec(exec) => {
                if let Some(shell) = exec.command.shell_str() {
                    fields.push(("exec.shell".to_owned(), shell));
                } else {
                    for (i, arg) in exec.command.text_fragments().into_iter().enumerate() {
                        fields.push((format!("exec.command[{i}]"), arg));
                    }
                }
                if let Some(stdin) = &exec.stdin {
                    fields.push(("exec.stdin".to_owned(), stdin.value.as_str()));
                }
                for (name, value) in &exec.env {
                    fields.push((format!("exec.env.{}", name.value), value.value.as_str()));
                }
            }
            RawAction::Invoke(invoke) => {
                if let Some(args) = &invoke.args {
                    json_fields(&args.value, "args".to_owned(), &mut fields);
                }
            }
            // A prompt is prose sent to a model, not an argument of an effect.
            _ => continue,
        }
        for (field, text) in fields {
            if let Some(written) = credential_in(text, task, &consts) {
                found.push(LiteralCredential {
                    task: task.id.value.clone(),
                    field,
                    through: match written {
                        Written::Here => None,
                        Written::Through(source) => Some(source),
                    },
                    sink_id: super::declass::sink_id(&task.action).to_owned(),
                    host: super::declass::literal_dest_host(&task.action),
                });
            }
        }
    }
    found
}

/// Every string of an argument value, at its path (`args.headers.Authorization` ·
/// `args.items[0]`).
fn json_fields<'a>(value: &'a Value, path: String, out: &mut Vec<(String, &'a str)>) {
    match value {
        Value::String(text) => out.push((path, text)),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                json_fields(item, format!("{path}[{i}]"), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                json_fields(item, format!("{path}.{key}"), out);
            }
        }
        _ => {}
    }
}

/// Where the literal credential an argument sends is written.
enum Written {
    /// In the argument itself.
    Here,
    /// In the `const:` or `with:` value the argument names (`const.<name>` · `with.<name>`).
    Through(String),
}

/// Where `text` writes the literal bearer credential it sends, if it sends one.
fn credential_in(text: &str, task: &RawTask, consts: &ConstStrings) -> Option<Written> {
    let Ok(islands) = scan_templates(text) else {
        // An unclosed island is its own conformance refusal; its bytes are read as written.
        return writes_bearer(text).then_some(Written::Here);
    };
    if writes_bearer(&rendered(text, &islands, |_| None)) {
        return Some(Written::Here);
    }
    let mut through = None;
    let resolved = rendered(text, &islands, |island| {
        let value = static_value(island, task, consts)?;
        through.get_or_insert_with(|| island.src.clone());
        Some(value)
    });
    writes_bearer(&resolved).then(|| through.map_or(Written::Here, Written::Through))
}

/// `text` with each island replaced by what `value` decides for it, else by
/// [`BOUND_AT_RUN`].
fn rendered(
    text: &str,
    islands: &[TemplateIsland],
    mut value: impl FnMut(&TemplateIsland) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for island in islands {
        out.push_str(text.get(at..island.start).unwrap_or_default());
        match value(island) {
            Some(literal) => out.push_str(&literal),
            None => out.push(BOUND_AT_RUN),
        }
        at = island.end;
    }
    out.push_str(text.get(at..).unwrap_or_default());
    out
}

/// The literal a bare `${{ const.<name> }}` or `${{ with.<name> }}` island names: the const's
/// string, or the task's `with:` string when it is a literal or itself a bare const.
fn static_value(island: &TemplateIsland, task: &RawTask, consts: &ConstStrings) -> Option<String> {
    let bare = format!("${{{{ {} }}}}", island.src);
    if let Some(literal) = consts.resolve(&bare) {
        return Some(literal.to_owned());
    }
    let name = island.src.strip_prefix("with.")?;
    let (_, value) = task.with.iter().find(|(key, _)| key.value == name)?;
    let text = value.value.as_str()?;
    if text.contains("${{") {
        consts.resolve(text).map(str::to_owned)
    } else {
        Some(text.to_owned())
    }
}

/// Whether `text` writes the `Bearer` scheme (any case, as a word) followed by blanks and a
/// token68 run (RFC 6750 §2.1) of at least [`MIN_TOKEN`] characters.
fn writes_bearer(text: &str) -> bool {
    const SCHEME: &str = "bearer";
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(found) = lower.get(from..).and_then(|rest| rest.find(SCHEME)) {
        let at = from + found;
        from = at + SCHEME.len();
        let word = at == 0
            || bytes
                .get(at - 1)
                .is_some_and(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        let rest = bytes.get(from..).unwrap_or_default();
        let blanks = rest
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let token = rest
            .iter()
            .skip(blanks)
            .take_while(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
            })
            .count();
        if word && blanks > 0 && token >= MIN_TOKEN {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        crate::check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    fn rows(r: &crate::CheckReport) -> Vec<&crate::UnifiedFinding> {
        r.findings
            .iter()
            .filter(|f| f.kind == "literal_credential")
            .collect()
    }

    /// The specimen of #1618, verbatim: a key pasted into an HTTP node, the whole secret
    /// lifecycle of the tools it comes from.
    const SPECIMEN: &str = r#"
nika: secret-in-file
model: mock/echo
permits:
  exec: false
  tools: ["nika:fetch"]
  net:
    http: ["api.example.com"]
tasks:
  call:
    invoke:
      tool: nika:fetch
      args:
        url: https://api.example.com/v1
        method: GET
        headers:
          Authorization: "Bearer sk-live-THISISFAKESECRETKEY"
outputs:
  ok: ${{ tasks.call.output }}
"#;

    /// The literal header is refused on the SECRETS gate: the row names the task and the
    /// argument, points to `secrets:` with the declaration and the reference that replace the
    /// literal, and never prints the token, nor does the report.
    #[test]
    fn a_literal_bearer_header_is_refused_and_points_to_secrets() {
        let r = report(SPECIMEN);
        assert!(!r.is_clean(), "{:#?}", r.findings);
        let rows = rows(&r);
        let [row] = rows.as_slice() else {
            panic!("one literal credential row: {:#?}", r.findings);
        };
        assert_eq!(
            (row.gate, row.severity),
            ("SECRETS", crate::FindingSeverity::Error)
        );
        assert_eq!(row.task.as_deref(), Some("call"));
        assert!(row.code.is_none(), "no spec code names this class: {row:?}");
        for part in [
            "`args.headers.Authorization`",
            "task `call`",
            "`secrets:`",
            "egress: [{ to: \"nika:fetch\", host: \"api.example.com\" }]",
            "Bearer ${{ secrets.api_token }}",
        ] {
            assert!(row.message.contains(part), "{part} · {}", row.message);
        }
        let json = serde_json::to_string(&r).expect("the report serializes");
        assert!(
            !json.contains("THISISFAKESECRETKEY"),
            "the token never rides: {json}"
        );
    }

    /// The specimen repaired as its row says: the declared secret, the egress the send needs
    /// and the template that references it. The template is not refused and the whole check
    /// is clean. The repair keeps the specimen's `outputs:` export out: returning the response
    /// of a call that carried a secret is `NIKA-SEC-007`'s own question, with its own repair.
    #[test]
    fn the_secret_form_the_row_names_is_clean() {
        let repaired = SPECIMEN
            .replace(
                "permits:",
                "secrets:\n  api_token: { source: env, key: API_TOKEN, egress: [{ to: \"nika:fetch\", host: \"api.example.com\" }] }\npermits:",
            )
            .replace("sk-live-THISISFAKESECRETKEY", "${{ secrets.api_token }}");
        let r = report(&repaired);
        assert!(rows(&r).is_empty(), "{:#?}", r.findings);
        let kinds: Vec<&str> = r.findings.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            ["secret_egress"],
            "only the export remains: {:#?}",
            r.findings
        );
        let r = report(&repaired.replace("outputs:\n  ok: ${{ tasks.call.output }}\n", ""));
        assert!(r.is_clean(), "the send is sanctioned: {:#?}", r.findings);
    }

    /// The second path of #1618 (wave 9): the token in `const:`, the header a template over it.
    /// A `with:` value the argument names is read the same way. The row names where the token
    /// is written.
    #[test]
    fn a_token_held_in_const_or_with_is_refused_where_it_is_written() {
        let through_const = SPECIMEN
            .replace(
                "permits:",
                "const:\n  token: \"sk-live-THISISFAKESECRETKEY\"\npermits:",
            )
            .replace(
                "Bearer sk-live-THISISFAKESECRETKEY",
                "Bearer ${{ const.token }}",
            );
        let through_with = SPECIMEN
            .replace(
                "\"Bearer sk-live-THISISFAKESECRETKEY\"",
                "\"${{ with.auth }}\"",
            )
            .replace(
                "    invoke:",
                "    with: { auth: \"Bearer sk-live-THISISFAKESECRETKEY\" }\n    invoke:",
            );
        for (yaml, source) in [
            (through_const, "`const.token`"),
            (through_with, "`with.auth`"),
        ] {
            let r = report(&yaml);
            assert!(!r.is_clean(), "{source}: {:#?}", r.findings);
            let rows = rows(&r);
            let [row] = rows.as_slice() else {
                panic!("{source}: one row: {:#?}", r.findings);
            };
            assert!(
                row.message.contains("`args.headers.Authorization`")
                    && row.message.contains(source),
                "{source} · {}",
                row.message
            );
            assert!(
                !row.message.contains("THISISFAKESECRETKEY"),
                "{}",
                row.message
            );
        }
    }

    /// An `exec:` argument carries the same header: an argv element, the shell string and an
    /// environment value are each refused at their own field, with the `exec` egress.
    #[test]
    fn a_literal_bearer_exec_argument_is_refused_at_its_field() {
        const H: &str = "Authorization: Bearer ghp_0123456789abcdefABCDEF0123456789abcd";
        for (exec, field) in [
            (
                format!("{{ command: [curl, -H, \"{H}\", \"https://api.example.com\"] }}"),
                "`exec.command[2]`",
            ),
            (
                format!("{{ shell: \"curl -H '{H}' https://api.example.com\" }}"),
                "`exec.shell`",
            ),
            (
                format!("{{ command: [printenv], env: {{ AUTH: \"{H}\" }} }}"),
                "`exec.env.AUTH`",
            ),
        ] {
            let r = report(&format!(
                "nika: w\npermits: {{ exec: [curl, printenv] }}\ntasks:\n  t:\n    exec: {exec}\n"
            ));
            let rows = rows(&r);
            let [row] = rows.as_slice() else {
                panic!("{field}: one row: {:#?}", r.findings);
            };
            assert!(row.message.contains(field), "{field} · {}", row.message);
            assert!(
                row.message.contains("egress: [{ to: \"exec\" }]"),
                "{}",
                row.message
            );
        }
    }

    /// What follows `Bearer` is a credential only when it is a literal token: prose, a shell
    /// variable, an input, an upstream output and a declared secret are not refused, nor is a
    /// short word.
    #[test]
    fn prose_and_values_bound_at_run_time_are_not_literal_credentials() {
        for value in [
            "Send a Bearer token in the Authorization header",
            "Bearer authentication",
            "Authorization: Bearer $API_TOKEN",
            "Bearer ${{ inputs.token }}",
            "Bearer ${{ with.from }}",
            "Bearer sk-${{ inputs.token }}",
            "Bearer short-token",
        ] {
            let yaml = format!(
                "nika: w\ninputs:\n  token: {{ type: string }}\npermits: {{ tools: [\"nika:log\"] }}\ntasks:\n  login:\n    invoke: {{ tool: \"nika:log\", args: {{ message: hi }} }}\n  t:\n    with: {{ from: \"${{{{ tasks.login.output }}}}\" }}\n    invoke: {{ tool: \"nika:log\", args: {{ message: \"{value}\" }} }}\n"
            );
            let r = report(&yaml);
            assert!(rows(&r).is_empty(), "{value}: {:#?}", r.findings);
        }
    }
}
