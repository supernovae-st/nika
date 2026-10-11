- **An OpenAI conversation keeps its prompt cache together.** A request can name the conversation
  it continues (`InferRequest::cache_key`, a hash of the conversation's id); the OpenAI wire sends
  it as `prompt_cache_key`, so one conversation's calls route and account their cached prefix
  together. Other providers ignore it, and a caller's own `prompt_cache_key` stays unchanged.
