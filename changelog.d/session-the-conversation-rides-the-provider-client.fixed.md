- **The conversation and the authoring seat ride the provider client.**
  The session's reasoner and its authoring seat built the FETCH client
  (`ReqwestHttp::new`: SSRF enforced, 30 s), not the engine's provider
  client (`nika_runtime::compose`: SSRF off on purpose, the transport
  ceiling): a local engine on `127.0.0.1` (`ollama` · `lmstudio` · …) or a
  seat behind a loopback endpoint answered « SSRF blocked » at the first
  conversation line, and a long local answer was cut at 30 s. The two paths
  now build the provider plane's client (SSRF disabled — the endpoints come
  from the fixed provider profiles, never from workflow data — and the
  600 s transport ceiling; the per-request deadline stays the wire layer's).
  The stalled-seat scenarios (a loopback endpoint that accepts and never
  answers) were false passes until now: the call failed at once and the
  recovery card's « say it again » satisfied the needle; they stall for
  real and prove the door leaves at once on `Ctrl+C` twice and `SIGTERM`.
