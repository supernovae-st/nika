- **The editor's `secrets` hover names the sources that work.** `nika lsp` described `secrets:` as
  « Masked, vault-backed references ». Spec 01 §secrets withdrew `vault`, and no engine resolves it.
  The hover now says the references point at an environment variable (`source: env`) or a file
  (`source: file`).
