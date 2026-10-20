- **Anthropic calls are priced again.** Every Anthropic reply now carries `"speed": "standard"` in
  its `usage` object, a key the completeness rule did not know, so each call counted as « provider
  did not report usage »: runs on Claude showed « cost · UNKNOWN · 1 unpriced call(s) » and no
  allowance settled them. The standard speed is now a known axis beside the standard tier and
  global inference; a fast reply, billed at another rate, still leaves its call unpriced.
