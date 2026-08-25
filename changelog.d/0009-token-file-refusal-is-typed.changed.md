- **Token-file refusal is typed.** `ServerError::Credential` now names
  unreadable, not a regular file, insecure mode, or invalid material.
  `nika serve --bind` still prints the openssl mint and never echoes
  bytes. Missing `--token-file` is unchanged. Paths stay dropped.
