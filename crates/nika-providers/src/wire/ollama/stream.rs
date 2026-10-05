//! Bounded NDJSON, not SSE. EOF without the native terminal marker is failure.

use super::{ToolNameMap, contents, map_http_err, parse, refused, stop, usage};
use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::{
    ai::provider::{ContentBlock, InferEvent, ProviderError},
    http::HttpError,
};
use std::{
    collections::{BTreeSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};

const FRAME_CAP: usize = 1024 * 1024;

pub(super) struct NativeStream {
    body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
    bytes: Vec<u8>,
    pending: VecDeque<Result<InferEvent, ProviderError>>,
    names: ToolNameMap,
    tool_ids: BTreeSet<String>,
    done: bool,
    closed: bool,
}
impl NativeStream {
    pub(super) fn new(
        body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
        names: ToolNameMap,
    ) -> Self {
        Self {
            body,
            bytes: Vec::new(),
            pending: VecDeque::new(),
            names,
            tool_ids: BTreeSet::new(),
            done: false,
            closed: false,
        }
    }

    fn line(&mut self) -> Result<(), ProviderError> {
        if self.bytes.iter().all(u8::is_ascii_whitespace) {
            self.bytes.clear();
            return Ok(());
        }
        if self.done {
            return Err(refused("data follows the terminal completion marker"));
        }
        let value = parse(&self.bytes)?;
        self.bytes.clear();
        for block in contents(&value, &self.names, &mut self.tool_ids)? {
            match block {
                ContentBlock::Text { text } => {
                    self.pending.push_back(Ok(InferEvent::Delta { text }));
                }
                ContentBlock::Thinking { text } => {
                    self.pending.push_back(Ok(InferEvent::Thinking { text }));
                }
                ContentBlock::ToolUse { id, name, input } => {
                    self.pending.push_back(Ok(InferEvent::ToolUseStart {
                        id: id.clone(),
                        name,
                    }));
                    self.pending.push_back(Ok(InferEvent::ToolUseDelta {
                        id,
                        partial_json: input.to_string(),
                    }));
                }
                _ => {}
            }
        }
        if value.get("done").and_then(serde_json::Value::as_bool) == Some(true) {
            self.done = true;
            if let Some(usage) = usage(&value) {
                self.pending.push_back(Ok(InferEvent::Usage(usage)));
            }
            // Delay Done until EOF: a malformed trailing frame must not look successful.
            self.pending.push_back(Ok(InferEvent::Done {
                stop_reason: stop(&value, !self.tool_ids.is_empty()),
                request_id: None,
                finish_reason_raw: value
                    .get("done_reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            }));
        }
        Ok(())
    }

    fn feed(&mut self, chunk: &[u8]) -> Result<(), ProviderError> {
        for &byte in chunk {
            if byte == b'\n' {
                self.line()?;
            } else {
                if self.bytes.len() >= FRAME_CAP {
                    return Err(refused("NDJSON frame exceeds 1 MiB"));
                }
                self.bytes.push(byte);
            }
        }
        Ok(())
    }

    fn fail(&mut self, error: ProviderError) {
        self.closed = true;
        self.pending.clear();
        self.bytes.clear();
        self.pending.push_back(Err(error));
    }
}

impl Stream for NativeStream {
    type Item = Result<InferEvent, ProviderError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            let holds_done =
                matches!(this.pending.front(), Some(Ok(InferEvent::Done { .. }))) && !this.closed;
            if !holds_done && let Some(event) = this.pending.pop_front() {
                return Poll::Ready(Some(event));
            }
            if this.closed {
                return Poll::Ready(None);
            }
            match this.body.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => {
                    if let Err(error) = this.feed(&chunk) {
                        this.fail(error);
                    }
                }
                Poll::Ready(Some(Err(error))) => this.fail(map_http_err(&error)),
                Poll::Ready(None) => {
                    if let Err(error) = this.line() {
                        this.fail(error);
                    } else if !this.done {
                        this.fail(refused("NDJSON ended before done=true; partial output is not a completed answer"));
                    }
                    this.closed = true;
                }
            }
        }
    }
}
