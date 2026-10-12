- **Anthropic requests ask for prompt caching.** A conversation, or a request with tools,
  now carries the automatic cache marker (five minutes) and a breakpoint on its last tool,
  so a later turn reads its prefix at a tenth of the input price. A one-shot request sends
  no marker, since a cache write costs a quarter more than the input it holds, and a
  request that names its own `cache_control` keeps it unchanged.
