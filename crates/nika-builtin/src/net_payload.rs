// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:fetch` payload families — `form:` (urlencoded) and
//! `multipart:` (RFC 7578 · file parts under the `permits.fs` READ
//! boundary). Split from `net.rs` (the 1500-LOC file cap): the request
//! ASSEMBLY lives here; transport, extraction and the traverse crawl
//! stay with their owners (`net.rs` · `net_traverse.rs`).

use bytes::Bytes;
use nika_kernel::io::fs::FsReadDyn;
use nika_kernel::io::http::{HttpMethod, HttpRequest};

use crate::permits::{FsAccess, FsBoundary};
use crate::wire::{self, Part};
use crate::{Args, BuiltinFailure};

use crate::net::build_request;

/// Assemble the outgoing request: the base (`build_request` · url ·
/// headers · raw `body:`) plus the vNext payload families — `form:`
/// (urlencoded) and `multipart:` (RFC 7578 · file parts read under the
/// `permits.fs` READ boundary). At most ONE of `body:`/`form:`/
/// `multipart:`; the payload families own their `content-type` (a
/// user-supplied one is a conflict, refused loudly) and need a
/// body-bearing method.
pub(crate) async fn prepare_request<F: FsReadDyn>(
    method: HttpMethod,
    url: &str,
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> Result<HttpRequest, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let declared = [
        args.contains_key("body"),
        args.contains_key("form"),
        args.contains_key("multipart"),
    ];
    if declared.iter().filter(|present| **present).count() > 1 {
        return Err(BuiltinFailure::new(
            C,
            "at most one of `body:` · `form:` · `multipart:` (builtins-v0.1.md §nika:fetch)",
        ));
    }
    let wants_payload = args.contains_key("form") || args.contains_key("multipart");
    if wants_payload
        && !matches!(
            method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        )
    {
        return Err(BuiltinFailure::new(
            C,
            "`form:`/`multipart:` need a body-bearing method — set `method: POST` (or PUT/PATCH)",
        ));
    }
    let mut request = build_request(method, url, args).map_err(|m| BuiltinFailure::new(C, m))?;
    if wants_payload
        && request
            .headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-type"))
    {
        return Err(BuiltinFailure::new(
            C,
            "`form:`/`multipart:` set their own content-type — drop the `headers:` entry",
        ));
    }
    if let Some(form) = args.get("form") {
        let encoded = form_body(form).map_err(|m| BuiltinFailure::new(C, m))?;
        request.headers.insert(
            "content-type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        );
        request.body = Some(encoded.into_bytes().into());
    }
    if let Some(spec) = args.get("multipart") {
        let parts = resolve_multipart(fs, boundary, spec).await?;
        let borrowed: Vec<Part<'_>> = parts.iter().map(OwnedPart::as_part).collect();
        let (body, content_type) =
            wire::multipart(&borrowed).map_err(|m| BuiltinFailure::new(C, m))?;
        request
            .headers
            .insert("content-type".to_owned(), content_type);
        request.body = Some(body.into());
    }
    Ok(request)
}

/// `form:` → `application/x-www-form-urlencoded` pairs. Scalars only —
/// nesting has no canonical urlencoded shape, so it is refused rather
/// than guessed (reshape upstream with `nika:jq`).
fn form_body(form: &serde_json::Value) -> Result<String, String> {
    let Some(map) = form.as_object() else {
        return Err("`form:` must be an object of scalar fields".to_owned());
    };
    let mut pairs = Vec::with_capacity(map.len());
    for (key, value) in map {
        let text = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => {
                return Err(format!(
                    "`form.{key}:` must be a string, number or boolean — reshape nested \
                     data with nika:jq or send `body:` JSON instead"
                ));
            }
        };
        pairs.push((key.as_str(), text));
    }
    Ok(wire::form_urlencode(&pairs))
}

/// One resolved multipart part — file bytes already read (and permit-
/// gated); borrows are handed to the wire encoder.
struct OwnedPart {
    name: String,
    filename: Option<String>,
    mime: Option<String>,
    payload: PartPayload,
}

enum PartPayload {
    Text(String),
    File(Bytes),
}

impl OwnedPart {
    fn as_part(&self) -> Part<'_> {
        match &self.payload {
            PartPayload::Text(value) => Part::Text {
                name: &self.name,
                value,
            },
            PartPayload::File(bytes) => Part::File {
                name: &self.name,
                filename: self.filename.as_deref().unwrap_or("part"),
                mime: self.mime.as_deref().unwrap_or("application/octet-stream"),
                bytes,
            },
        }
    }
}

/// The closed part shape: `{name, value}` text XOR `{name, path,
/// filename?, content_type?}` file. Every `path:` is enforced against
/// the `permits.fs` READ boundary BEFORE any byte is read
/// (`NIKA-SEC-004` on escape — the image edit-input precedent).
const MULTIPART_PART_KEYS: [&str; 5] = ["name", "value", "path", "filename", "content_type"];
/// Upload ceiling — assets-not-blobs: bigger payloads ship by URL.
const MAX_MULTIPART_BYTES: usize = 32 * 1024 * 1024;
const MAX_MULTIPART_PARTS: usize = 64;

async fn resolve_multipart<F: FsReadDyn>(
    fs: &F,
    boundary: &FsBoundary,
    spec: &serde_json::Value,
) -> Result<Vec<OwnedPart>, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let fail = |m: String| BuiltinFailure::new(C, m);
    let Some(items) = spec.as_array() else {
        return Err(fail("`multipart:` must be an array of parts".to_owned()));
    };
    if items.is_empty() {
        return Err(fail("`multipart:` needs at least one part".to_owned()));
    }
    if items.len() > MAX_MULTIPART_PARTS {
        return Err(fail(format!(
            "`multipart:` caps at {MAX_MULTIPART_PARTS} parts (got {})",
            items.len()
        )));
    }
    let mut total = 0usize;
    let mut parts = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Some(map) = item.as_object() else {
            return Err(fail(format!("multipart part {i} must be an object")));
        };
        if let Some(unknown) = map
            .keys()
            .find(|k| !MULTIPART_PART_KEYS.contains(&k.as_str()))
        {
            return Err(fail(format!(
                "multipart part {i}: unknown key `{unknown}` — the shape is \
                 {{name, value}} or {{name, path, filename?, content_type?}}"
            )));
        }
        let Some(name) = map.get("name").and_then(serde_json::Value::as_str) else {
            return Err(fail(format!("multipart part {i} needs a string `name:`")));
        };
        parts.push(resolve_part(fs, boundary, name, map, &mut total).await?);
        if total > MAX_MULTIPART_BYTES {
            return Err(fail(format!(
                "multipart payload exceeds {MAX_MULTIPART_BYTES} bytes (~32 MiB) — \
                 ship large assets by URL instead"
            )));
        }
    }
    Ok(parts)
}

/// Resolve one part (text value XOR permit-gated file read).
async fn resolve_part<F: FsReadDyn>(
    fs: &F,
    boundary: &FsBoundary,
    name: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    total: &mut usize,
) -> Result<OwnedPart, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let fail = |m: String| BuiltinFailure::new(C, m);
    match (map.get("value"), map.get("path")) {
        (Some(_), Some(_)) | (None, None) => Err(fail(format!(
            "multipart part `{name}`: exactly one of `value:` (text) | `path:` (file)"
        ))),
        (Some(value), None) => {
            let Some(text) = value.as_str() else {
                return Err(fail(format!(
                    "multipart part `{name}`: `value:` must be a string — stringify \
                     upstream with nika:jq"
                )));
            };
            if map.contains_key("filename") || map.contains_key("content_type") {
                return Err(fail(format!(
                    "multipart part `{name}`: `filename:`/`content_type:` belong to \
                     file parts (`path:`)"
                )));
            }
            *total += text.len();
            Ok(OwnedPart {
                name: name.to_owned(),
                filename: None,
                mime: None,
                payload: PartPayload::Text(text.to_owned()),
            })
        }
        (None, Some(path)) => {
            let Some(path) = path.as_str() else {
                return Err(fail(format!(
                    "multipart part `{name}`: `path:` must be a string"
                )));
            };
            boundary.enforce(fs, path, FsAccess::Read).await?;
            let bytes = fs.read(std::path::Path::new(path)).await.map_err(|e| {
                fail(format!(
                    "multipart part `{name}`: `{path}` could not be read: {e}"
                ))
            })?;
            *total += bytes.len();
            let filename = match map.get("filename") {
                Some(serde_json::Value::String(f)) => Some(f.clone()),
                Some(_) => {
                    return Err(fail(format!(
                        "multipart part `{name}`: `filename:` must be a string"
                    )));
                }
                None => std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned()),
            };
            let mime = match map.get("content_type") {
                Some(serde_json::Value::String(m)) => Some(m.clone()),
                Some(_) => {
                    return Err(fail(format!(
                        "multipart part `{name}`: `content_type:` must be a string"
                    )));
                }
                None => None,
            };
            Ok(OwnedPart {
                name: name.to_owned(),
                filename,
                mime,
                payload: PartPayload::File(bytes),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use nika_kernel_mock::{MockFs, MockHttp};

    use crate::permits::FsBoundary;
    use crate::{Args, BuiltinOutcome};

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    /// The no-fs/no-boundary adapter (file-part tests build their own).
    async fn fetch(http: &MockHttp, args: &Args) -> BuiltinOutcome {
        crate::net::fetch(http, &MockFs::new(), &FsBoundary::unbounded(), args).await
    }

    // ─── fetch vNext · form: + multipart: (stdlib §fetch) ──────────────

    #[tokio::test]
    async fn multipart_uploads_file_part_bytes_under_the_boundary() {
        let fs = MockFs::new().with_file("out/bg.png", b"\x89PNG-fake-image-bytes".to_vec());
        let boundary = FsBoundary::declared(vec!["out/**".to_owned()], Vec::new());
        let http =
            MockHttp::new().enqueue_ok(200, br#"{"url":"https://cdn.test/bg.png"}"#.to_vec());
        let out = crate::net::fetch(
            &http,
            &fs,
            &boundary,
            &args(serde_json::json!({
                "url": "https://api.test/upload", "method": "POST", "mode": "jq", "jq": ".url",
                "multipart": [
                    { "name": "directory", "value": "assets" },
                    { "name": "file", "path": "out/bg.png", "content_type": "image/png" }
                ]
            })),
        )
        .await
        .expect("upload succeeds");
        assert_eq!(out, serde_json::json!("https://cdn.test/bg.png"));
        let sent = http.sent_requests();
        let req = sent.first().expect("one request");
        let body = req.body.as_ref().expect("multipart body");
        assert!(
            body.windows(21).any(|w| w == b"\x89PNG-fake-image-bytes"),
            "file bytes ride verbatim"
        );
        let text = String::from_utf8_lossy(body);
        assert!(text.contains("name=\"directory\"\r\n\r\nassets"));
        assert!(text.contains("filename=\"bg.png\"") && text.contains("Content-Type: image/png"));
        let ct = req.headers.get("content-type").expect("content-type set");
        assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
    }

    #[tokio::test]
    async fn multipart_path_outside_the_boundary_is_sec_004() {
        let fs = MockFs::new().with_file("secrets/key.pem", b"private".to_vec());
        let boundary = FsBoundary::declared(vec!["out/**".to_owned()], Vec::new());
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = crate::net::fetch(
            &http,
            &fs,
            &boundary,
            &args(serde_json::json!({
                "url": "https://api.test/upload", "method": "POST",
                "multipart": [ { "name": "file", "path": "secrets/key.pem" } ]
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == crate::permits::SEC_DENIED),
            "escape must be NIKA-SEC-004: {fail:?}"
        );
        assert!(
            http.sent_requests().is_empty(),
            "no byte leaves the machine"
        );
    }

    #[tokio::test]
    async fn multipart_part_shape_violations_are_loud() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        for (spec, needle) in [
            (serde_json::json!([]), "at least one part"),
            (serde_json::json!([{ "name": "f" }]), "exactly one of"),
            (
                serde_json::json!([{ "name": "f", "value": "v", "path": "p" }]),
                "exactly one of",
            ),
            (
                serde_json::json!([{ "name": "f", "value": "v", "surprise": 1 }]),
                "unknown key",
            ),
            (
                serde_json::json!([{ "name": "f", "value": "v", "filename": "x" }]),
                "file parts",
            ),
        ] {
            let fail = fetch(
                &http,
                &args(serde_json::json!({
                    "url": "https://api.test", "method": "POST", "multipart": spec
                })),
            )
            .await;
            assert!(
                matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                    && f.message.contains(needle)),
                "wanted `{needle}` in {fail:?}"
            );
        }
    }

    #[tokio::test]
    async fn multipart_conflicting_user_content_type_is_refused() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "method": "POST",
                "headers": { "Content-Type": "application/json" },
                "multipart": [ { "name": "f", "value": "v" } ]
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("content-type")),
            "{fail:?}"
        );
    }

    #[tokio::test]
    async fn form_encodes_urlencoded_body_and_content_type() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test/submit", "method": "POST", "mode": "raw",
                "form": { "a": "b c", "n": 5, "flag": true }
            })),
        )
        .await
        .expect("form post succeeds");
        assert_eq!(out, serde_json::Value::String("ok".to_owned()));
        let sent = http.sent_requests();
        let req = sent.first().expect("one request");
        let body = req.body.as_ref().expect("form body present");
        assert_eq!(
            std::str::from_utf8(body).expect("utf8"),
            "a=b+c&flag=true&n=5",
            "urlencoded · space→+ · BTreeMap key order"
        );
        assert_eq!(
            req.headers.get("content-type").map(String::as_str),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[tokio::test]
    async fn form_on_get_is_a_clear_error() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "form": { "a": "b" }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("POST")),
            "{fail:?}"
        );
        assert!(http.sent_requests().is_empty(), "no request spent");
    }

    #[tokio::test]
    async fn body_form_multipart_are_mutually_exclusive() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "method": "POST", "body": "x",
                "form": { "a": "b" }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("at most one")),
            "{fail:?}"
        );
    }
}
