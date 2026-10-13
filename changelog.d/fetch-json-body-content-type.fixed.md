- **`nika:fetch` sends a JSON `body:` with `content-type: application/json`.** An object,
  array or other non-string `body:` was serialized as JSON but sent with no content type,
  so a receiver that parses by type read it as empty. It now carries
  `application/json` unless the author's `headers:` already name a content type; a
  string body is still sent as written.
