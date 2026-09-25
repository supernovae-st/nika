// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Protocol fixtures: a requested model is not an observed model until the
//! ACP configuration response confirms it. No provider or credential is used.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_core::Stream as _;
use nika_kernel::ai::harness::{HarnessError, HarnessRequest};
use serde_json::{Value, json};
use std::pin::Pin;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn options(current: &str) -> Value {
    json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
        "currentValue": current, "options": [{"value": "default", "name": "Default"},
        {"value": "chosen", "name": "Chosen"}]}]})
}

#[tokio::test]
async fn absent_stale_or_malformed_model_confirmation_refuses_before_prompt() {
    for response in [
        json!({}),
        options("default"),
        options("chosen")["configOptions"].clone(),
    ] {
        let (ours, theirs) = tokio::io::duplex(16 * 1024);
        let (read, write) = tokio::io::split(ours);
        let (agent_read, mut agent_write) = tokio::io::split(theirs);
        let agent = tokio::spawn(async move {
            let mut lines = BufReader::new(agent_read).lines();
            for method in ["initialize", "session/new", "session/set_config_option"] {
                let line = lines.next_line().await.expect("read").expect("request");
                let request: Value = serde_json::from_str(&line).expect("JSON request");
                assert_eq!(request["method"], method);
                let result = match method {
                    "initialize" => json!({"protocolVersion": 1}),
                    "session/new" => {
                        let mut result = options("default");
                        result["sessionId"] = json!("test-session");
                        result
                    }
                    _ => {
                        assert_eq!(request["params"]["value"], "chosen");
                        response.clone()
                    }
                };
                let answer = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
                agent_write
                    .write_all(format!("{answer}\n").as_bytes())
                    .await
                    .expect("reply");
            }
            assert!(
                lines.next_line().await.expect("read").is_none(),
                "no prompt after an unconfirmed selection"
            );
        });
        let mut stream = nika_harness::drive(
            read,
            write,
            HarnessRequest::new("hello", "/tmp").with_requested_model("chosen"),
        );
        let outcome = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        assert!(
            matches!(outcome, Some(Err(HarnessError::Refused { .. }))),
            "{outcome:?}"
        );
        drop(stream);
        agent.await.expect("scripted peer completed");
    }
}
