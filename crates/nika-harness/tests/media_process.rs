// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real child process, synthetic protocol bytes only. No installed harness, auth or model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use nika_harness::{HarnessAdapter, SpawnedHarness};
use nika_kernel::ai::harness::{AgentBackend as _, HarnessEvent, HarnessRequest};

const PEER: &str = r"
import json,sys,base64,struct,zlib,random
mode=sys.argv[1]
def send(v): print(json.dumps(v),flush=True)
def recv(): return json.loads(sys.stdin.readline())
def reply(q,v):send({'jsonrpc':'2.0','id':q['id'],'result':v})
def update(v,s='media-test'):send({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':s,'update':v}})
q=recv();assert q['method']=='initialize';reply(q,{'protocolVersion':1})
q=recv();assert q['method']=='session/new';reply(q,{'sessionId':'media-test'})
q=recv();assert q['method']=='session/prompt'
start={'sessionUpdate':'tool_call','toolCallId':'image-1','title':'Image generation','status':'in_progress'}
update(start)
png='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lmYAAAAASUVORK5CYII='
if mode=='large':
 def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
 rng=random.Random(23)
 pixels=b''.join(b'\x00'+rng.randbytes(1024*3) for _ in range(1024))
 image=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1024,1024,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')
 png=base64.b64encode(image).decode()
if mode=='overflow': png='A'*(8*1024*1024+1)
if mode=='path':png=''
result={'sessionUpdate':'tool_call_update','toolCallId':'image-1','status':'completed','rawOutput':{'status':'completed','result':png,'savedPath':'/not-opened/receipt.png'},'content':[] if not png else [{'type':'content','content':{'type':'image','mimeType':'image/png','data':png}}]}
update(result,'foreign-session')
if mode=='failed':result['status']='failed'
update(result)
if mode=='eof':sys.exit(0)
update({'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'kept text'}})
reply(q,{'stopReason':'end_turn'})
";

#[tokio::test]
async fn process_images_survive_but_foreign_failed_oversized_and_unsettled_runs_never_succeed() {
    for mode in ["png", "large", "path", "failed", "overflow", "eof"] {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("peer.py");
        std::fs::write(&script, PEER).unwrap();
        let adapter = HarnessAdapter::new("media-peer", "python3")
            .unwrap()
            .with_args(vec![script.to_string_lossy().into_owned(), mode.into()]);
        let mut stream = SpawnedHarness::new(adapter)
            .run_agent(HarnessRequest::new("fixture", dir.path()))
            .await
            .unwrap();
        let (mut images, mut completed, mut failed) = (0, 0, false);
        while let Some(event) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
            match event {
                Ok(HarnessEvent::ImageObserved { image }) => {
                    images += 1;
                    if mode == "large" {
                        assert!(image.received_bytes.unwrap() > 1024 * 1024);
                        assert!(image.observation().to_string().len() < 1024);
                        assert!(image.stored_blob.is_none(), "transport never writes a blob");
                    }
                    assert_eq!(
                        image.reported_saved_path.as_deref(),
                        Some("/not-opened/receipt.png")
                    );
                }
                Ok(HarnessEvent::Completed { outcome }) => {
                    completed += 1;
                    assert_eq!(outcome.output, "kept text");
                    assert_eq!(outcome.images.len(), 1);
                    assert_eq!(
                        outcome.images[0].data.is_some(),
                        matches!(mode, "png" | "large")
                    );
                }
                Err(error) => {
                    failed = true;
                    assert!(
                        !error.is_transient(),
                        "no replay after image activity: {error}"
                    );
                }
                _ => {}
            }
        }
        if matches!(mode, "png" | "large" | "path") {
            assert_eq!(images, 1, "foreign session ignored");
            assert_eq!(completed, 1);
            assert!(!failed);
        } else {
            assert_eq!(completed, 0);
            assert!(failed);
        }
    }
}
