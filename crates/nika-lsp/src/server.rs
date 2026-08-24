// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sync `lsp-server` dispatch loop.
//!
//! A thin transport shell over the pure [`analysis`](crate::analysis)
//! brain: it owns the open-document map and translates JSON-RPC messages
//! to analysis calls and back. No async, no tokio — `lsp-server`'s
//! blocking stdio loop (rust-analyzer's) is enough for v0.1 (single-file,
//! full-reparse-on-change).
//!
//! Lifecycle · `initialize` (handled by `Connection::initialize`) →
//! `initialized` → the main loop over `Request`/`Notification` →
//! `shutdown`/`exit`. Diagnostics publish on `didOpen` and `didChange`.
//!
//! Cancellation · each turn drains everything already queued behind the
//! blocking head message into one BATCH, harvests the batch's
//! `$/cancelRequest` notifications, and answers a request cancelled
//! BEFORE it was computed with `-32800 RequestCancelled` instead of a
//! result the client already discarded (a fast-typing burst queues
//! stale requests behind their cancels). Message ORDER is untouched —
//! the batch replays in arrival order; only cancelled requests skip
//! their compute. A cancel for an already-answered request is a no-op
//! per the spec. Because the batch may already hold the post-shutdown
//! `exit`, the loop owns the shutdown dance itself (lsp-server's
//! `handle_shutdown` recv-s the live channel and would stall on an
//! `exit` sitting in the batch).
//!
//! The open documents are keyed by the URI's STRING form (not the `Uri`
//! type itself) — `Uri` carries interior mutability (its internal offset
//! cache), so it is not a valid map key, and a `BTreeMap<String, _>` is
//! deterministic (the studio's BTree-everywhere discipline).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    Cancel, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, GotoDefinition, HoverRequest, Request as RequestTrait, Shutdown,
};
use lsp_types::{
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, HoverParams, PublishDiagnosticsParams, Uri,
};

use crate::analysis::diagnostics;
use crate::analysis::document::Document;
use crate::analysis::{
    code_action, completion, definition, hover, rename, semantic_document, symbols,
};
use crate::capabilities::server_capabilities;
use crate::error::LspError;
use crate::watchdog;

/// The open-document map: URI string → its buffer + line index.
type Docs = BTreeMap<String, Document>;

/// The document-symbol request method id (its request marker type is not
/// re-exported by name, so we match the method string directly).
const DOCUMENT_SYMBOL_METHOD: &str = "textDocument/documentSymbol";

/// `nika/semanticDocument` — the vendor-prefixed custom request (the
/// rust-analyzer `lsp_ext` convention: permanent extensions live under
/// the vendor name; capability-gated via `experimental.nika`). Params:
/// `{ "textDocument": { "uri": … } }`. Result: the semantic-document
/// payload (see `analysis::semantic_document`).
const SEMANTIC_DOCUMENT_METHOD: &str = "nika/semanticDocument";

/// The `exit` notification method string.
const EXIT_METHOD: &str = "exit";

/// JSON-RPC 2.0 reserved error codes (the LSP base protocol).
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

/// LSP reserved error code: the client cancelled the request
/// (`$/cancelRequest`) before the server computed it.
const REQUEST_CANCELLED: i32 = -32800;

/// How long the post-shutdown wait for `exit` may block (mirrors
/// `lsp_server::Connection::handle_shutdown`).
const EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Run the language server over `connection` until the client shuts it
/// down. The `connection` is already past the initialize handshake.
///
/// See the module docs for the batch/cancellation model.
pub(crate) fn serve(connection: &Connection) -> Result<(), LspError> {
    let mut docs: Docs = BTreeMap::new();
    while let Ok(head) = connection.receiver.recv() {
        let mut batch = VecDeque::from([head]);
        while let Ok(msg) = connection.receiver.try_recv() {
            batch.push_back(msg);
        }
        let mut cancelled = harvest_cancellations(&batch);
        while let Some(msg) = batch.pop_front() {
            match msg {
                Message::Request(req) if req.method == Shutdown::METHOD => {
                    send(connection, Response::new_ok(req.id, ()).into())?;
                    return await_exit(connection, batch);
                }
                Message::Request(req) => {
                    let response = if cancelled.remove(&req.id) {
                        Response::new_err(
                            req.id,
                            REQUEST_CANCELLED,
                            "request cancelled by the client".to_owned(),
                        )
                    } else {
                        handle_request(req, &docs)
                    };
                    send(connection, response.into())?;
                }
                // Harvested above. A leftover id at the end of the batch
                // names a request answered in an EARLIER batch — a
                // completed request's cancel is a no-op per the spec, and
                // rebuilding the set per batch keeps it bounded.
                Message::Notification(note) if note.method == Cancel::METHOD => {}
                Message::Notification(note) => {
                    handle_notification(connection, note, &mut docs)?;
                }
                // Responses to server→client requests — v0.1 sends none, so
                // any response is unexpected and safely ignored.
                Message::Response(_) => {}
            }
        }
    }
    Ok(())
}

/// Collect the request ids named by the batch's `$/cancelRequest`
/// notifications. Malformed cancel params are ignored — never crash the
/// loop on one bad notification.
fn harvest_cancellations(batch: &VecDeque<Message>) -> BTreeSet<RequestId> {
    batch
        .iter()
        .filter_map(|msg| match msg {
            Message::Notification(n) if n.method == Cancel::METHOD => {
                serde_json::from_value::<lsp_types::CancelParams>(n.params.clone()).ok()
            }
            _ => None,
        })
        .map(|p| match p.id {
            lsp_types::NumberOrString::Number(n) => RequestId::from(n),
            lsp_types::NumberOrString::String(s) => RequestId::from(s),
        })
        .collect()
}

/// Consume the post-shutdown `exit`: first from the tail of the already
/// drained batch, then from the live receiver. Mirrors the strictness of
/// `lsp_server::Connection::handle_shutdown` (the very next message must
/// be `exit`) — which this loop cannot call: its internal `recv` would
/// never see an `exit` already drained into the batch and would stall
/// the shutdown for its full timeout.
fn await_exit(connection: &Connection, mut batch: VecDeque<Message>) -> Result<(), LspError> {
    let next = match batch.pop_front() {
        Some(msg) => msg,
        None => connection
            .receiver
            .recv_timeout(EXIT_WAIT)
            .map_err(|e| LspError::Protocol(format!("waiting for exit notification: {e}")))?,
    };
    match next {
        Message::Notification(n) if n.method == EXIT_METHOD => Ok(()),
        other => Err(LspError::Protocol(format!(
            "unexpected message during shutdown: {other:?}"
        ))),
    }
}

/// Run the full stdio lifecycle: connect, initialize, serve, join threads.
///
/// `client_process_id` is the host's `--clientProcessId` argv value, when
/// it passed one. The `initialize` params carry the same fact for hosts
/// that use only the protocol channel, so both are offered to
/// [`watchdog::declared_parent`] and the winner is watched for the rest of
/// the session (#1181 — before this, the flag was accepted and dropped and
/// the `initialize` `processId` was never read at all).
pub(crate) fn run_stdio(client_process_id: Option<u32>) -> Result<(), LspError> {
    let (connection, io_threads) = Connection::stdio();
    // argv names the parent BEFORE the handshake, so watch it from here:
    // `initialize` blocks until the client speaks, and a host that dies
    // during that window would otherwise strand the server exactly as
    // #1181 describes — with the watchdog it was given never started.
    let argv_parent = client_process_id.filter(|pid| *pid != 0);
    if let Some(pid) = argv_parent {
        watchdog::spawn(pid, watchdog::POLL_INTERVAL);
    }
    let caps = serde_json::to_value(server_capabilities()).map_err(LspError::Serde)?;
    let init_params = connection
        .initialize(caps)
        .map_err(|e| LspError::Protocol(e.to_string()))?;
    // The payload channel only becomes readable here. It covers the hosts
    // that send `processId` and no flag; argv already won if it was given.
    if argv_parent.is_none()
        && let Some(pid) = watchdog::declared_parent(None, &init_params)
    {
        watchdog::spawn(pid, watchdog::POLL_INTERVAL);
    }
    let result = serve(&connection);
    // Drop the connection BEFORE joining: it owns the sender whose channel
    // feeds the writer IO thread. While the connection lives the writer
    // never sees its channel close, so `join()` would block forever (the
    // `Connection::memory()` canary masks this — it drops its server
    // explicitly; only the real stdio path joins these threads).
    drop(connection);
    let join = io_threads.join().map_err(LspError::Transport);
    result.and(join)
}

/// Dispatch one request to the analysis brain and build its response.
fn handle_request(req: Request, docs: &Docs) -> Response {
    let id = req.id.clone();
    match req.method.as_str() {
        HoverRequest::METHOD => respond(id, req, |p: HoverParams| {
            let pos = p.text_document_position_params;
            let doc = docs.get(uri_key(&pos.text_document.uri))?;
            let offset = doc.line_index().offset(pos.position);
            hover::hover(doc.text(), offset)
        }),
        Completion::METHOD => respond(id, req, |p: CompletionParams| {
            let pos = p.text_document_position;
            let doc = docs.get(uri_key(&pos.text_document.uri))?;
            let offset = doc.line_index().offset(pos.position);
            // The document's directory roots the one disk-backed lane
            // (agent `skills:`) — a non-file scheme simply loses it.
            let doc_dir = file_uri_dir(&pos.text_document.uri);
            Some(CompletionResponse::Array(completion::completion_at(
                doc.text(),
                offset,
                doc_dir.as_deref(),
            )))
        }),
        CodeActionRequest::METHOD => respond(id, req, |p: lsp_types::CodeActionParams| {
            let doc = docs.get(uri_key(&p.text_document.uri))?;
            Some(code_action::code_actions(
                &p.text_document.uri,
                doc.text(),
                doc.line_index(),
                p.range,
            ))
        }),
        "textDocument/prepareRename" => {
            respond(id, req, |p: lsp_types::TextDocumentPositionParams| {
                let doc = docs.get(uri_key(&p.text_document.uri))?;
                let offset = doc.line_index().offset(p.position);
                rename::prepare(doc.text(), offset).map(|(range, placeholder)| {
                    lsp_types::PrepareRenameResponse::RangeWithPlaceholder { range, placeholder }
                })
            })
        }
        "textDocument/rename" => respond_fallible(id, req, |p: lsp_types::RenameParams| {
            let pos = p.text_document_position;
            let doc = docs
                .get(uri_key(&pos.text_document.uri))
                .ok_or_else(|| "document not open".to_owned())?;
            let offset = doc.line_index().offset(pos.position);
            rename::rename(&pos.text_document.uri, doc.text(), offset, &p.new_name)
        }),
        GotoDefinition::METHOD => respond(id, req, |p: GotoDefinitionParams| {
            let pos = p.text_document_position_params;
            let uri = pos.text_document.uri;
            let doc = docs.get(uri_key(&uri))?;
            let offset = doc.line_index().offset(pos.position);
            definition::definition(&uri, doc.text(), offset).map(GotoDefinitionResponse::Scalar)
        }),
        DOCUMENT_SYMBOL_METHOD => respond(id, req, |p: DocumentSymbolParams| {
            let doc = docs.get(uri_key(&p.text_document.uri))?;
            Some(DocumentSymbolResponse::Nested(symbols::document_symbols(
                doc.text(),
            )))
        }),
        SEMANTIC_DOCUMENT_METHOD => respond(id, req, |p: lsp_types::TextDocumentIdentifier| {
            let doc = docs.get(uri_key(&p.uri))?;
            Some(semantic_document::semantic_document(doc.text()))
        }),
        // Unknown request → null result (a valid JSON-RPC reply the client
        // tolerates; v0.1 advertises only the four read features).
        _ => Response::new_ok(id, serde_json::Value::Null),
    }
}

/// Extract typed params, run the handler, and wrap the optional result in a
/// JSON-RPC response. A `None` handler result becomes a JSON `null` (the LSP
/// « no result » reply). A MALFORMED payload becomes a JSON-RPC
/// `InvalidParams` error response — never an ok-null (which would tell the
/// client « understood, nothing here » and mask a real client-side bug) and
/// never a crash (one bad request must not bring the loop down).
fn respond<P, R, F>(id: RequestId, req: Request, handler: F) -> Response
where
    P: serde::de::DeserializeOwned,
    R: serde::Serialize,
    F: FnOnce(P) -> Option<R>,
{
    let method = req.method.clone();
    match req.extract::<P>(&method) {
        Ok((_id, params)) => match handler(params) {
            Some(result) => Response::new_ok(id, result),
            None => Response::new_ok(id, serde_json::Value::Null),
        },
        Err(ExtractError::JsonError { method, error }) => Response::new_err(
            id,
            INVALID_PARAMS,
            format!("invalid params for {method}: {error}"),
        ),
        // Unreachable in practice (the method was matched to pick this
        // handler) — an honest internal error still beats a silent ok-null.
        Err(ExtractError::MethodMismatch(_)) => {
            Response::new_err(id, INTERNAL_ERROR, "internal method mismatch".to_owned())
        }
    }
}

/// Dispatch one notification (document lifecycle + diagnostics publish).
fn handle_notification(
    connection: &Connection,
    note: Notification,
    docs: &mut Docs,
) -> Result<(), LspError> {
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            if let Some(p) = extract_note::<DidOpenTextDocumentParams>(note) {
                let uri = p.text_document.uri;
                docs.insert(
                    uri_key(&uri).to_owned(),
                    Document::new(&p.text_document.text),
                );
                publish(connection, docs, &uri)?;
            }
        }
        DidChangeTextDocument::METHOD => {
            if let Some(p) = extract_note::<DidChangeTextDocumentParams>(note) {
                let uri = p.text_document.uri;
                let doc = docs
                    .entry(uri_key(&uri).to_owned())
                    .or_insert_with(|| Document::new(""));
                doc.apply_changes(&p.content_changes);
                publish(connection, docs, &uri)?;
            }
        }
        DidCloseTextDocument::METHOD => {
            if let Some(p) = extract_note::<DidCloseTextDocumentParams>(note) {
                docs.remove(uri_key(&p.text_document.uri));
                // clear the client's squiggles for the closed document
                clear_diagnostics(connection, &p.text_document.uri)?;
            }
        }
        EXIT_METHOD => {
            // An `exit` reaches here ONLY without a prior `shutdown`
            // (`Connection::handle_shutdown` consumes the post-shutdown
            // exit). Per the LSP spec that is abnormal termination — end the
            // loop with an error so the process exits non-zero (the
            // `nika lsp` arm maps any serve error to exit code 1).
            return Err(LspError::Protocol(
                "received `exit` without a prior `shutdown`".to_owned(),
            ));
        }
        _ => {} // `initialized` and any other notification: nothing to do
    }
    Ok(())
}

/// Compute and publish diagnostics for one document's current text.
fn publish(connection: &Connection, docs: &Docs, uri: &Uri) -> Result<(), LspError> {
    let Some(doc) = docs.get(uri_key(uri)) else {
        return Ok(());
    };
    let diags = diagnose(doc);
    let params = PublishDiagnosticsParams::new(uri.clone(), diags, None);
    send(
        connection,
        Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into(),
    )
}

/// Publish an empty diagnostic set (clears the client's squiggles).
fn clear_diagnostics(connection: &Connection, uri: &Uri) -> Result<(), LspError> {
    let params = PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None);
    send(
        connection,
        Notification::new(PublishDiagnostics::METHOD.to_owned(), params).into(),
    )
}

/// Run the parse+check ladder over a document and map it to diagnostics.
fn diagnose(doc: &Document) -> Vec<lsp_types::Diagnostic> {
    let index = doc.line_index();
    match nika_schema::parse(
        doc.text(),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) {
        Ok(wf) => diagnostics::from_report(index, &nika_check::check(&wf), &wf),
        Err(err) => vec![diagnostics::from_parse_error(index, &err)],
    }
}

/// Send a message, mapping a transport failure to [`LspError::Transport`].
fn send(connection: &Connection, msg: Message) -> Result<(), LspError> {
    connection
        .sender
        .send(msg)
        .map_err(|e| LspError::Transport(std::io::Error::other(e.to_string())))
}

/// The document-map key for a URI (its canonical string form).
/// Like [`respond`], for handlers whose refusal carries a MESSAGE the
/// client must show (LSP rename: an invalid new name is a request error,
/// never a silent no-op edit).
fn respond_fallible<P, R, F>(id: RequestId, req: Request, handler: F) -> Response
where
    P: serde::de::DeserializeOwned,
    R: serde::Serialize,
    F: FnOnce(P) -> Result<R, String>,
{
    let method = req.method.clone();
    match req.extract::<P>(&method) {
        Ok((_id, params)) => match handler(params) {
            Ok(result) => Response::new_ok(id, result),
            Err(message) => Response::new_err(id, INVALID_PARAMS, message),
        },
        Err(ExtractError::JsonError { method, error }) => Response::new_err(
            id,
            INVALID_PARAMS,
            format!("invalid params for {method}: {error}"),
        ),
        Err(ExtractError::MethodMismatch(_)) => {
            Response::new_err(id, INTERNAL_ERROR, "internal method mismatch".to_owned())
        }
    }
}

fn uri_key(uri: &Uri) -> &str {
    uri.as_str()
}

/// Extract a notification's typed params; `None` on a payload mismatch
/// (never crash the loop on one bad notification).
fn extract_note<P: serde::de::DeserializeOwned>(note: Notification) -> Option<P> {
    let method = note.method.clone();
    note.extract::<P>(&method).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::position::LineIndex;
    use lsp_types::{
        DidOpenTextDocumentParams, PartialResultParams, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use std::str::FromStr;

    fn uri() -> Uri {
        Uri::from_str("file:///w.nika.yaml").expect("uri")
    }

    fn open(docs: &mut Docs, text: &str) {
        docs.insert(uri_key(&uri()).to_owned(), Document::new(text));
    }

    #[test]
    fn hover_request_returns_verb_doc() {
        let mut docs = BTreeMap::new();
        let yaml = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"hi\" }\n";
        open(&mut docs, yaml);
        let index = LineIndex::new(yaml);
        let pos = index.position(yaml.find("infer").expect("verb") + 1);
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri()),
                position: pos,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let req = Request::new(1.into(), HoverRequest::METHOD.to_owned(), params);
        let resp = handle_request(req, &docs);
        let result = resp.response_result.expect("ok result");
        assert!(
            result.to_string().contains("infer"),
            "hover mentions the verb: {result}"
        );
    }

    #[test]
    fn document_symbol_request_returns_nested_tree() {
        let mut docs = BTreeMap::new();
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        open(&mut docs, yaml);
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let req = Request::new(2.into(), DOCUMENT_SYMBOL_METHOD.to_owned(), params);
        let resp = handle_request(req, &docs);
        let result = resp.response_result.expect("ok result");
        assert!(result.to_string().contains('a'), "task `a` in outline");
    }

    #[test]
    fn unknown_request_returns_null_not_error() {
        let docs = BTreeMap::new();
        let req = Request::new(
            3.into(),
            "textDocument/foldingRange".to_owned(),
            serde_json::json!({}),
        );
        let resp = handle_request(req, &docs);
        assert!(matches!(resp.response_result, Ok(serde_json::Value::Null)));
    }

    #[test]
    fn malformed_request_params_yield_a_json_rpc_invalid_params_error() {
        // A hover request whose params are NOT a valid HoverParams payload
        // must come back as a JSON-RPC `InvalidParams` (-32602) ERROR
        // response — never an ok-null (which would mask the client bug). This
        // pins the exact reserved error CODE (the `-` sign on the constant).
        let docs = BTreeMap::new();
        let req = Request::new(
            4.into(),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({ "garbage": true }),
        );
        let resp = handle_request(req, &docs);
        let err = resp
            .response_result
            .expect_err("an error response, not ok-null");
        assert_eq!(err.code, -32602, "JSON-RPC InvalidParams reserved code");
        assert!(
            err.message.contains("invalid params for"),
            "the error names the bad method: {}",
            err.message
        );
    }

    #[test]
    fn uri_key_is_the_uri_string_form() {
        // `uri_key` is the document-map key — it must be the URI's canonical
        // string, not "" / a constant (which would collide every document).
        let u = uri();
        assert_eq!(uri_key(&u), "file:///w.nika.yaml", "the exact URI string");
        let other = Uri::from_str("file:///other.nika.yaml").expect("uri");
        assert_ne!(
            uri_key(&u),
            uri_key(&other),
            "distinct URIs map to distinct keys"
        );
    }

    #[test]
    fn documents_are_keyed_distinctly_by_uri() {
        // Two open documents under different URIs must not collide — proves
        // `uri_key` returns a per-URI key (a constant "" / "xyzzy" would make
        // the second insert overwrite the first).
        let mut docs: Docs = BTreeMap::new();
        let a = Uri::from_str("file:///a.nika.yaml").expect("uri");
        let b = Uri::from_str("file:///b.nika.yaml").expect("uri");
        docs.insert(uri_key(&a).to_owned(), Document::new("nika: v1\n# a\n"));
        docs.insert(uri_key(&b).to_owned(), Document::new("nika: v1\n# b\n"));
        assert_eq!(docs.len(), 2, "two distinct documents, no key collision");
        assert_eq!(
            docs.get(uri_key(&a)).map(Document::text),
            Some("nika: v1\n# a\n")
        );
        assert_eq!(
            docs.get(uri_key(&b)).map(Document::text),
            Some("nika: v1\n# b\n")
        );
    }

    #[test]
    fn diagnose_broken_workflow_yields_an_error() {
        let doc = Document::new(
            "nika: w\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n",
        );
        let diags = diagnose(&doc);
        assert!(
            diags
                .iter()
                .any(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR)),
            "broken DAG → an error diagnostic"
        );
    }

    #[test]
    fn didopen_inserts_into_the_doc_map() {
        let mut docs = BTreeMap::new();
        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri(),
                "nika".to_owned(),
                1,
                "nika: v1\n".to_owned(),
            ),
        };
        docs.insert(
            uri_key(&open_params.text_document.uri).to_owned(),
            Document::new(&open_params.text_document.text),
        );
        assert_eq!(
            docs.get(uri_key(&uri())).map(Document::text),
            Some("nika: v1\n")
        );
    }

    /// Drain whatever the server already sent to the client side of a memory
    /// connection (non-blocking), collecting published-diagnostics params.
    fn drain_published(client: &Connection) -> Vec<PublishDiagnosticsParams> {
        let mut out = Vec::new();
        while let Ok(msg) = client.receiver.try_recv() {
            if let Message::Notification(n) = msg
                && n.method == PublishDiagnostics::METHOD
                && let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(n.params)
            {
                out.push(p);
            }
        }
        out
    }

    fn change_note(text: &str) -> Notification {
        let params = DidChangeTextDocumentParams {
            text_document: lsp_types::VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 2,
            },
            content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_owned(),
            }],
        };
        Notification::new(DidChangeTextDocument::METHOD.to_owned(), params)
    }

    #[test]
    fn didchange_updates_the_doc_and_republishes_diagnostics() {
        // The didChange arm must full-replace the document text AND publish a
        // fresh diagnostic set. Deleting the arm would leave the old text and
        // publish nothing.
        let (server, client) = Connection::memory();
        let mut docs: Docs = BTreeMap::new();
        open(
            &mut docs,
            "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n",
        );
        // change to a BROKEN workflow (waits on a ghost) → an error diag.
        let broken = "nika: w\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n";
        handle_notification(&server, change_note(broken), &mut docs).expect("handled");
        assert_eq!(
            docs.get(uri_key(&uri())).map(Document::text),
            Some(broken),
            "the document text was replaced"
        );
        let published = drain_published(&client);
        let diags = published
            .iter()
            .find(|p| p.uri == uri())
            .expect("diagnostics were republished after the change");
        assert!(
            diags
                .diagnostics
                .iter()
                .any(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR)),
            "the changed (broken) text produced an error diagnostic"
        );
    }

    #[test]
    fn didclose_removes_the_doc_and_clears_diagnostics() {
        // The didClose arm must REMOVE the document from the map AND publish
        // an EMPTY diagnostic set (clearing the client's squiggles). Deleting
        // the arm would leave the doc and publish nothing.
        let (server, client) = Connection::memory();
        let mut docs: Docs = BTreeMap::new();
        open(&mut docs, "nika: v1\n");
        assert_eq!(docs.len(), 1, "one open doc before close");
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri()),
        };
        let note = Notification::new(DidCloseTextDocument::METHOD.to_owned(), params);
        handle_notification(&server, note, &mut docs).expect("handled");
        assert!(
            docs.is_empty(),
            "the closed document was removed from the map"
        );
        let published = drain_published(&client);
        let cleared = published
            .iter()
            .find(|p| p.uri == uri())
            .expect("a clear-diagnostics notification was published");
        assert!(
            cleared.diagnostics.is_empty(),
            "closing publishes an EMPTY diagnostic set (clears squiggles)"
        );
    }

    #[test]
    fn exit_without_prior_shutdown_is_a_protocol_error() {
        // An `exit` notification reaching handle_notification (i.e. WITHOUT a
        // prior shutdown, which Connection::handle_shutdown would consume) is
        // abnormal — it must return a Protocol error so the process exits
        // non-zero. Deleting the arm would swallow it into Ok(()).
        let (server, _client) = Connection::memory();
        let mut docs: Docs = BTreeMap::new();
        let note = Notification::new(EXIT_METHOD.to_owned(), serde_json::Value::Null);
        let result = handle_notification(&server, note, &mut docs);
        assert!(
            matches!(result, Err(LspError::Protocol(_))),
            "bare exit is a protocol violation, not Ok: {result:?}"
        );
    }

    #[test]
    fn harvest_cancellations_collects_ids_and_ignores_garbage() {
        // Number ids and string ids both collect; a malformed cancel and an
        // unrelated notification contribute nothing (and never crash).
        let cancel_note = |params: serde_json::Value| -> Message {
            Notification::new(Cancel::METHOD.to_owned(), params).into()
        };
        let batch = VecDeque::from([
            cancel_note(serde_json::json!({ "id": 7 })),
            cancel_note(serde_json::json!({ "id": "abc" })),
            cancel_note(serde_json::json!({ "garbage": true })),
            Notification::new("initialized".to_owned(), serde_json::json!({})).into(),
        ]);
        let set = harvest_cancellations(&batch);
        assert_eq!(set.len(), 2, "exactly the two well-formed cancel ids");
        assert!(set.contains(&RequestId::from(7)));
        assert!(set.contains(&RequestId::from("abc".to_owned())));
    }

    #[test]
    fn unrelated_notification_is_ignored_without_error() {
        // The default arm (e.g. `initialized`) does nothing and returns Ok —
        // no doc mutation, no publish.
        let (server, client) = Connection::memory();
        let mut docs: Docs = BTreeMap::new();
        let note = Notification::new("initialized".to_owned(), serde_json::json!({}));
        handle_notification(&server, note, &mut docs).expect("ignored cleanly");
        assert!(docs.is_empty(), "no document side effect");
        assert!(
            drain_published(&client).is_empty(),
            "nothing published for an unrelated notification"
        );
    }
}

/// The parent directory of a `file://` document URI — `None` for any
/// other scheme (an untitled buffer loses only the skills lane). The
/// percent-decoding covers the space class (`%20`); an exotic byte
/// sequence simply fails to resolve and degrades to `None`.
fn file_uri_dir(uri: &lsp_types::Uri) -> Option<std::path::PathBuf> {
    if uri.scheme().is_none_or(|s| s.as_str() != "file") {
        return None;
    }
    let raw = uri.path().as_str();
    // Percent-decode at the BYTE level, then re-assemble as UTF-8 — a
    // per-byte `as char` would latin-1 every multi-byte sequence
    // (`é` = `%C3%A9` → `Ã©`) and the walk would root in a ghost dir.
    let mut decoded = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = raw.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            decoded.push(v);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let mut text = String::from_utf8_lossy(&decoded).into_owned();
    // A Windows drive rides as `/C:/…` in the URI path — shed the
    // leading slash so the PathBuf is the real drive-rooted path.
    if text.len() >= 3
        && text.as_bytes()[0] == b'/'
        && text.as_bytes()[1].is_ascii_alphabetic()
        && text.as_bytes()[2] == b':'
    {
        text.remove(0);
    }
    let path = std::path::PathBuf::from(text);
    path.parent().map(std::path::Path::to_path_buf)
}

/// Gate 9 — the canary E2E: drive the full server lifecycle over an
/// in-memory connection pair and assert the live message exchange (no
/// stdio, no Keychain, runs under `--lib`).
#[cfg(test)]
mod canary {
    use super::*;
    use lsp_types::request::{
        Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Request as RequestTrait,
    };
    use lsp_types::{
        InitializeParams, PartialResultParams, Position, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentPositionParams, WorkDoneProgressParams,
    };
    use std::str::FromStr;
    use std::thread;

    fn uri() -> Uri {
        Uri::from_str("file:///canary.nika.yaml").expect("uri")
    }

    /// Send a didOpen notification for `text`.
    fn did_open(client: &Connection, text: &str) {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri(), "nika".to_owned(), 1, text.to_owned()),
        };
        client
            .sender
            .send(Notification::new(DidOpenTextDocument::METHOD.to_owned(), params).into())
            .expect("send didOpen");
    }

    /// Send a request.
    fn request<P: serde::Serialize>(client: &Connection, id: i32, method: &str, params: P) {
        client
            .sender
            .send(Request::new(id.into(), method.to_owned(), params).into())
            .expect("send request");
    }

    /// Send a `$/cancelRequest` notification for request `id`.
    fn cancel(client: &Connection, id: i32) {
        let params = lsp_types::CancelParams {
            id: lsp_types::NumberOrString::Number(id),
        };
        client
            .sender
            .send(Notification::new(Cancel::METHOD.to_owned(), params).into())
            .expect("send cancel");
    }

    /// Open an arbitrary URI with the given text.
    fn open_uri(client: &Connection, uri: &Uri, text: &str) {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "nika".to_owned(),
                1,
                text.to_owned(),
            ),
        };
        client
            .sender
            .send(Notification::new(DidOpenTextDocument::METHOD.to_owned(), params).into())
            .expect("send didOpen");
    }

    /// A `TextDocumentPositionParams` over `uri` at `pos`.
    fn at(uri: &Uri, pos: Position) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position: pos,
        }
    }

    /// Queue the four read-feature requests over the `hello` document
    /// (hover id 10 · documentSymbol 11 · definition 12).
    fn queue_hello_requests(client: &Connection, hello_uri: &Uri, verb: Position, dep: Position) {
        request(
            client,
            10,
            HoverRequest::METHOD,
            lsp_types::HoverParams {
                text_document_position_params: at(hello_uri, verb),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        );
        request(
            client,
            11,
            DocumentSymbolRequest::METHOD,
            lsp_types::DocumentSymbolParams {
                text_document: TextDocumentIdentifier::new(hello_uri.clone()),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        );
        request(
            client,
            12,
            GotoDefinition::METHOD,
            lsp_types::GotoDefinitionParams {
                text_document_position_params: at(hello_uri, dep),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        );
    }

    /// Open a `model:`-value document and queue a completion request (id 13).
    fn queue_model_completion(client: &Connection) {
        let model_doc = "nika: w\nmodel: ";
        let model_uri = Uri::from_str("file:///m.nika.yaml").expect("uri");
        open_uri(client, &model_uri, model_doc);
        let model_idx = crate::analysis::position::LineIndex::new(model_doc);
        request(
            client,
            13,
            Completion::METHOD,
            lsp_types::CompletionParams {
                text_document_position: at(&model_uri, model_idx.position(model_doc.len())),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            },
        );
    }

    /// W1 rename over the wire: prepare (14) · rename (15) · refusal (16).
    fn queue_rename_requests(client: &Connection, hello_uri: &Uri, key_pos: Position) {
        request(
            client,
            14,
            "textDocument/prepareRename",
            at(hello_uri, key_pos),
        );
        for (id, name) in [(15, "salute"), (16, "Bad-Name")] {
            request(
                client,
                id,
                "textDocument/rename",
                lsp_types::RenameParams {
                    text_document_position: at(hello_uri, key_pos),
                    new_name: name.to_owned(),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                },
            );
        }
    }

    /// Run the server over the pre-queued conversation in a scoped thread
    /// (joins before the scope returns — no detached thread), then drain
    /// the buffered replies.
    fn run_server_and_drain(
        server: Connection,
        client: &Connection,
    ) -> (Vec<PublishDiagnosticsParams>, Vec<Response>) {
        let server_ref = &server;
        thread::scope(|scope| {
            scope.spawn(move || {
                let caps = serde_json::to_value(server_capabilities()).expect("caps");
                server_ref.initialize(caps).expect("initialize");
                serve(server_ref).expect("serve");
            });
        });
        // Drop the server so its sender disconnects — pump's recv then
        // terminates once the buffered replies are drained.
        drop(server);
        pump(client)
    }

    /// Drain client→server received messages, collecting the published
    /// diagnostics and keeping responses by id for assertion.
    fn pump(client: &Connection) -> (Vec<PublishDiagnosticsParams>, Vec<Response>) {
        let mut diags = Vec::new();
        let mut responses = Vec::new();
        while let Ok(msg) = client
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
        {
            match msg {
                Message::Notification(n) if n.method == PublishDiagnostics::METHOD => {
                    if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(n.params) {
                        diags.push(p);
                    }
                }
                Message::Response(r) => responses.push(r),
                _ => {}
            }
            // Stop once we have the shutdown response (id 99).
            if responses.iter().any(|r| r.id == 99.into()) {
                break;
            }
        }
        (diags, responses)
    }

    #[test]
    fn full_lifecycle_initialize_open_query_shutdown() {
        let (server, client) = Connection::memory();

        // Pre-queue the ENTIRE client→server conversation before the server
        // runs. The memory connection's channels are unbounded, so the
        // server drains them in order: initialize → initialized → didOpens
        // + requests → shutdown → exit. This avoids spawning a thread
        // (std::thread::spawn is workspace-disallowed; this crate is sync,
        // not tokio) — a scoped thread runs the server, the main thread
        // reads the buffered responses after it returns.

        // Client handshake.
        request(&client, 1, "initialize", InitializeParams::default());
        client
            .sender
            .send(Notification::new("initialized".to_owned(), serde_json::json!({})).into())
            .expect("initialized");

        // 1) A broken workflow → a NIKA-* error diagnostic published.
        let broken = "nika: w\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n";
        did_open(&client, broken);

        // 2) A clean workflow with a verb, an `after:` entry and a
        //    template ref riding a `with:` binding (the W2 doors).
        let hello = "nika: hello\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n  use_it:\n    after: { greet: success }\n    with:\n      msg: \"${{ tasks.greet.output }}\"\n    exec: { command: [\"echo\", \"${{ with.msg }}\"] }\n";
        let hello_uri = Uri::from_str("file:///hello.nika.yaml").expect("uri");
        open_uri(&client, &hello_uri, hello);
        let idx = crate::analysis::position::LineIndex::new(hello);
        let verb_pos = idx.position(hello.find("infer").expect("verb") + 1);
        let dep_pos = idx.position(hello.rfind("greet").expect("dep ref") + 1);
        queue_hello_requests(&client, &hello_uri, verb_pos, dep_pos);
        queue_model_completion(&client);
        let key_pos = idx.position(hello.find("\n  greet:").expect("key") + 3);
        queue_rename_requests(&client, &hello_uri, key_pos);

        // shutdown / exit (queued last — the server returns after exit)
        request(&client, 99, "shutdown", serde_json::Value::Null);
        client
            .sender
            .send(Notification::new("exit".to_owned(), serde_json::Value::Null).into())
            .expect("exit");

        let (diags, responses) = run_server_and_drain(server, &client);

        // --- assertions ---
        assert_broken_diagnostics(&diags);
        assert_query_replies(&responses);
    }

    /// A NIKA-* error diagnostic was published for the broken workflow.
    fn assert_broken_diagnostics(diags: &[PublishDiagnosticsParams]) {
        let broken_diags = diags
            .iter()
            .find(|d| d.uri == uri())
            .expect("diagnostics for the broken workflow");
        assert!(
            broken_diags.diagnostics.iter().any(|d| matches!(
                &d.code,
                Some(lsp_types::NumberOrString::String(c)) if c.starts_with("NIKA-")
            )),
            "a NIKA-* code was published: {:?}",
            broken_diags.diagnostics
        );
    }

    /// Assert every queued reply (10-16): hover · symbols · definition ·
    /// completion · prepareRename · rename edit · rename refusal.
    fn assert_query_replies(responses: &[Response]) {
        let response = |id: i32| {
            responses
                .iter()
                .find(|r| r.id == id.into())
                .unwrap_or_else(|| panic!("response {id} present"))
        };
        // hover mentions the verb
        let hover = response(10).response_result.clone().expect("hover result");
        assert!(hover.to_string().contains("infer"), "hover: {hover}");
        // documentSymbol returns the workflow outline with the task
        let symbols = response(11)
            .response_result
            .clone()
            .expect("symbols result");
        assert!(symbols.to_string().contains("greet"), "symbols: {symbols}");
        // definition resolves the island ref to a Location
        let def = response(12)
            .response_result
            .clone()
            .expect("definition result");
        assert!(def.to_string().contains("hello.nika.yaml"), "def: {def}");
        // completion offers providers at the model value
        let completion = response(13)
            .response_result
            .clone()
            .expect("completion result");
        assert!(
            completion.to_string().contains("ollama/"),
            "completion: {completion}"
        );
        // prepareRename answers the key token + placeholder
        let prep = response(14)
            .response_result
            .clone()
            .expect("prepare result");
        assert!(prep.to_string().contains("greet"), "prepare: {prep}");
        // rename returns a WorkspaceEdit moving the key + the dep + the ref
        let ws = response(15).response_result.clone().expect("rename result");
        let ws_str = ws.to_string();
        assert!(ws_str.contains("salute"), "rename edit: {ws_str}");
        assert_eq!(
            ws_str.matches("salute").count(),
            3,
            "key + after entry + island ref — three edits: {ws_str}"
        );
        // an invalid new name is a request ERROR carrying the teaching
        let refusal = response(16);
        let err = refusal
            .response_result
            .as_ref()
            .expect_err("rename refusal is an error");
        assert!(
            err.message.contains("snake_case"),
            "the refusal teaches the grammar: {}",
            err.message
        );
    }

    /// A hover request over `hello_uri` at the verb position.
    fn hover_at(client: &Connection, id: i32, hello_uri: &Uri, pos: Position) {
        request(
            client,
            id,
            HoverRequest::METHOD,
            lsp_types::HoverParams {
                text_document_position_params: at(hello_uri, pos),
                work_done_progress_params: WorkDoneProgressParams::default(),
            },
        );
    }

    #[test]
    fn cancelled_queued_request_answers_request_cancelled_without_compute() {
        // Fast typing: a burst [hover · cancel · hover] is already queued
        // when the server picks the batch up. The cancelled request must
        // come back as a -32800 RequestCancelled ERROR — never a computed
        // result the client already discarded — and the live request after
        // it must still compute normally.
        let (server, client) = Connection::memory();
        request(&client, 1, "initialize", InitializeParams::default());
        client
            .sender
            .send(Notification::new("initialized".to_owned(), serde_json::json!({})).into())
            .expect("initialized");
        let hello =
            "nika: hello\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n";
        let hello_uri = Uri::from_str("file:///hello.nika.yaml").expect("uri");
        open_uri(&client, &hello_uri, hello);
        let idx = crate::analysis::position::LineIndex::new(hello);
        let verb = idx.position(hello.find("infer").expect("verb") + 1);
        hover_at(&client, 10, &hello_uri, verb);
        cancel(&client, 10);
        hover_at(&client, 11, &hello_uri, verb);
        request(&client, 99, "shutdown", serde_json::Value::Null);
        client
            .sender
            .send(Notification::new("exit".to_owned(), serde_json::Value::Null).into())
            .expect("exit");

        let (_diags, responses) = run_server_and_drain(server, &client);

        let by_id = |id: i32| {
            responses
                .iter()
                .find(|r| r.id == id.into())
                .unwrap_or_else(|| panic!("response {id} present"))
        };
        let cancelled = by_id(10);
        let err = cancelled
            .response_result
            .as_ref()
            .expect_err("cancelled request → an error response, not a computed result");
        assert_eq!(err.code, -32800, "LSP RequestCancelled reserved code");
        let live = by_id(11);
        assert!(
            live.response_result.is_ok(),
            "the live request still computes"
        );
        let hover = live.response_result.clone().expect("hover result");
        assert!(hover.to_string().contains("infer"), "hover: {hover}");
    }

    #[test]
    fn cancel_after_the_answer_is_a_quiet_noop() {
        // A cancel landing AFTER its request was answered (the common case
        // in a sync loop) must not crash the loop, must not produce a
        // second response for the id, and must not disturb later requests.
        //
        // The scope body only DRIVES the interleaving — every assertion
        // happens after the scope. A panic inside the scope would deadlock
        // the join against a server still blocked on `recv` (the client
        // side of the memory connection stays alive through the unwind).
        let (server, client) = Connection::memory();
        request(&client, 1, "initialize", InitializeParams::default());
        client
            .sender
            .send(Notification::new("initialized".to_owned(), serde_json::json!({})).into())
            .expect("initialized");
        let hover_params = lsp_types::HoverParams {
            text_document_position_params: at(&uri(), Position::new(0, 0)),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let mut responses: Vec<Response> = Vec::new();
        let server_ref = &server;
        thread::scope(|scope| {
            scope.spawn(move || {
                let caps = serde_json::to_value(server_capabilities()).expect("caps");
                server_ref.initialize(caps).expect("initialize");
                serve(server_ref).expect("serve");
            });
            // Collect responses (≤10s) until `id` answers; never panics.
            let await_response = |sink: &mut Vec<Response>, id: i32| {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while std::time::Instant::now() < deadline {
                    if let Ok(Message::Response(r)) = client
                        .receiver
                        .recv_timeout(std::time::Duration::from_millis(200))
                    {
                        let hit = r.id == id.into();
                        sink.push(r);
                        if hit {
                            return;
                        }
                    }
                }
            };
            // An unopened doc hovers to ok-null — but it IS answered.
            request(&client, 10, HoverRequest::METHOD, hover_params.clone());
            await_response(&mut responses, 10);
            // The cancel arrives in a LATER batch than its target.
            cancel(&client, 10);
            request(&client, 11, HoverRequest::METHOD, hover_params.clone());
            await_response(&mut responses, 11);
            // Always shut the server down so the scope can join.
            request(&client, 99, "shutdown", serde_json::Value::Null);
            client
                .sender
                .send(Notification::new("exit".to_owned(), serde_json::Value::Null).into())
                .expect("exit");
            await_response(&mut responses, 99);
        });
        assert_eq!(
            responses.iter().filter(|r| r.id == 10.into()).count(),
            1,
            "exactly ONE response for the answered-then-cancelled request"
        );
        let live = responses
            .iter()
            .find(|r| r.id == 11.into())
            .expect("response 11 present");
        assert!(
            live.response_result.is_ok(),
            "request 11 unaffected by the stale cancel"
        );
        assert!(
            responses.iter().any(|r| r.id == 99.into()),
            "shutdown acknowledged"
        );
    }

    #[test]
    fn shutdown_with_exit_already_queued_terminates_cleanly() {
        // The whole tail [didOpen · shutdown · exit] drains into ONE batch.
        // The loop must find the exit inside its own batch — blindly
        // recv-ing from the live channel after the shutdown response
        // (lsp-server's `handle_shutdown`) would stall 30 seconds here and
        // then die on a protocol error.
        let (server, client) = Connection::memory();
        request(&client, 1, "initialize", InitializeParams::default());
        client
            .sender
            .send(Notification::new("initialized".to_owned(), serde_json::json!({})).into())
            .expect("initialized");
        did_open(&client, "nika: v1\n");
        request(&client, 99, "shutdown", serde_json::Value::Null);
        client
            .sender
            .send(Notification::new("exit".to_owned(), serde_json::Value::Null).into())
            .expect("exit");
        let (_diags, responses) = run_server_and_drain(server, &client);
        let shutdown = responses
            .iter()
            .find(|r| r.id == 99.into())
            .expect("shutdown response");
        assert!(shutdown.response_result.is_ok(), "shutdown acknowledged ok");
    }

    #[test]
    fn unexpected_message_between_shutdown_and_exit_is_a_protocol_error() {
        // Mirrors lsp-server's `handle_shutdown` strictness: after the
        // shutdown response the NEXT message must be `exit` — anything else
        // ends the loop with a protocol error (non-zero exit).
        let (server, client) = Connection::memory();
        request(&client, 99, "shutdown", serde_json::Value::Null);
        request(&client, 100, HoverRequest::METHOD, serde_json::Value::Null);
        client
            .sender
            .send(Notification::new("exit".to_owned(), serde_json::Value::Null).into())
            .expect("exit");
        // serve is already past the handshake by contract — run it directly
        // over the pre-queued tail.
        let result = serve(&server);
        assert!(
            matches!(result, Err(LspError::Protocol(_))),
            "a request between shutdown and exit is a protocol violation: {result:?}"
        );
    }
}

#[cfg(test)]
mod uri_tests {
    use std::str::FromStr;

    use super::file_uri_dir;

    fn uri(s: &str) -> lsp_types::Uri {
        lsp_types::Uri::from_str(s).expect("uri")
    }

    #[test]
    fn file_uris_yield_their_parent_percent_decoded() {
        assert_eq!(
            file_uri_dir(&uri("file:///tmp/proj/flow.nika.yaml")),
            Some(std::path::PathBuf::from("/tmp/proj"))
        );
        // %20 — the space class the decoding exists for.
        assert_eq!(
            file_uri_dir(&uri("file:///tmp/my%20proj/flow.nika.yaml")),
            Some(std::path::PathBuf::from("/tmp/my proj"))
        );
    }

    #[test]
    fn multibyte_percent_escapes_decode_as_utf8_not_latin1() {
        assert_eq!(
            file_uri_dir(&uri("file:///tmp/caf%C3%A9/flow.nika.yaml")),
            Some(std::path::PathBuf::from("/tmp/café"))
        );
    }

    #[test]
    fn a_windows_drive_path_sheds_the_uri_slash() {
        assert_eq!(
            file_uri_dir(&uri("file:///C:/proj/flow.nika.yaml")),
            Some(std::path::PathBuf::from("C:/proj"))
        );
    }

    #[test]
    fn non_file_schemes_lose_only_the_disk_lane() {
        assert_eq!(file_uri_dir(&uri("untitled:Untitled-1")), None);
        assert_eq!(file_uri_dir(&uri("https://example.com/a/b.yaml")), None);
    }
}
