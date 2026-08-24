- **`nika serve --bind` prints the token mint instead of an opaque
  credential refusal.** Missing `--token-file`, a short secret, or a
  group/world-readable file all teach
  `umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600
  .nika/serve.token` and never echo the bytes. The HTTP door still
  refuses to mint a secret on its own.
