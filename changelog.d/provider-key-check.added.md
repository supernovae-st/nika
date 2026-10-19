- **An Anthropic key can be checked on request, without running a model.** `check_key` (and
  the Session's `check_api_key`) sends one GET of Anthropic's model list with the key, its
  API version and the endpoint a run would use (`NIKA_ANTHROPIC_BASE_URL` honored). It
  answers valid, refused (401 or 403, in the provider's words with the key withheld),
  unreachable or unknown, spends no token, never follows a redirect or retries, and runs
  only when a person asks. Another provider answers unknown without a request: an
  OpenAI-compatible model list may answer without a key, so it would prove none.
