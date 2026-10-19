- **A long conversation no longer overflows an OpenAI-compatible model's window.** Nika asks a
  route for its whole published output capacity, and an OpenAI-compatible route counts the prompt
  and that output against one context window. On OpenRouter's DeepSeek V4 Flash (943,718 output
  tokens in a 1,048,576-token window) every call failed with HTTP 400 once the conversation passed
  about 105,000 tokens: « you requested about 1060562 tokens (… 943718 in the output) », after an
  authoring loop of 30 calls and 13 minutes. The output budget sent is now what the prompt leaves
  of the window, the prompt counted generously from the request's size, less a margin of 1/64 of
  the window. A budget that already fits, and a route with no known window, are sent unchanged.
