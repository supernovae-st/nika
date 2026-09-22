- **`--authoring-model` and `--decision-model` ride the provider client.** The CLI seated
  an authoring or decision call on the default fetch client: its 30 s idle-read guard cut
  every buffered cloud call at 30 s whatever `--authoring-timeout` asked (a reasoning seat
  answered 408 after 30 s), and its SSRF floor refused a local seat on `127.0.0.1`. The
  seats now use the runtime's provider client (`nika_runtime::compose::provider_http`, now
  public): the same fixed endpoint allowlist, the transport ceiling above the requested
  timeout, no SSRF floor. A local seat reports its socket's own refusal.
