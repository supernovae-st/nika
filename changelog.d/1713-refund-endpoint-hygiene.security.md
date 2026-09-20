- **Refuse credential-bearing and cleartext refund endpoints.** The
  bounded support composition rejects a refund endpoint whose query
  carries a credential-like parameter (`api_key`, `token`, `secret`,
  `sig`, `auth`) and a plain `http` destination other than a loopback
  development host, so a literal URL can never smuggle a secret into the
  workflow or POST a refund in cleartext.
