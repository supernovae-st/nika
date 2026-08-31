- **Oversized HTTP uploads now return their typed 413 response.** Nika drains
  authenticated bodies with constant memory and the existing request timeout
  before closing, so Node clients no longer lose the response to `EPIPE`.
