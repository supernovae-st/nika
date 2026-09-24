// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Mechanics-only loopback peer used via the cfg(test) HTTP substitution.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
pub(crate) fn response(text: &str) -> Value {
    json!({"model":"deepseek-v4-pro","id":"session-fixture","choices":[{"message":{"content":text},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":100,"total_tokens":120}})
}
pub(crate) struct Peer {
    pub url: String,
    seen: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Peer {
    pub(crate) fn start(script: Vec<(u16, Value)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let addr = listener.local_addr().expect("addr");
        let url = format!("http://{addr}/chat/completions");
        let seen = Arc::new(Mutex::new(vec![]));
        let requests = seen.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let thread = std::thread::spawn(move || {
            for s in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = s.expect("stream");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .expect("timeout");
                let Some(body) = read(&mut stream) else {
                    continue;
                };
                let i = requests.lock().expect("seen").len();
                requests.lock().expect("seen").push(body);
                let (status, value) = script.get(i).or_else(|| script.last()).expect("script");
                let data = value.to_string();
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).expect("reply");
            }
        });
        Self {
            url,
            seen,
            stop,
            thread: Some(thread),
        }
    }
    pub(crate) fn bodies(&self) -> Vec<Value> {
        self.seen.lock().expect("seen").clone()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let addr = self
            .url
            .trim_start_matches("http://")
            .split('/')
            .next()
            .expect("addr");
        let _ = TcpStream::connect(addr);
        if let Some(t) = self.thread.take() {
            t.join().expect("peer");
        }
    }
}
fn read(stream: &mut TcpStream) -> Option<Value> {
    let mut data = vec![];
    let mut chunk = [0; 8192];
    let end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
    let n: usize = head
        .lines()
        .find_map(|s| s.strip_prefix("content-length:"))
        .and_then(|s| s.trim().parse().ok())?;
    while data.len() < end + n {
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..count]);
    }
    serde_json::from_slice(&data[end..end + n]).ok()
}
