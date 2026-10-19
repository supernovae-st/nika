- **`egress/001` warns about a private key in a prompt.** A `prompt:` or
  `system:` text holding a PEM private-key block (a line
  `-----BEGIN … PRIVATE KEY-----`) is a credential bound for the model
  provider: the linter flags it and the check verdict lists it in
  `hints[]`, on inline text and on the text of a prompt file alike, where
  the door read that file. It refuses nothing and never repeats the text;
  a secret the provider needs belongs in `secrets:`.
