- **A Claude call is priced, so the Run ledger, `--max-cost-usd` and a Session allowance
  count it.** The Anthropic wire never marked a reply's usage complete, so every Claude
  call stayed cost-unknown ("provider did not report usage") and a Session allowance with
  a Claude author was refused. A reply whose usage accounts for every billed meter (input,
  output, cache read and 5-minute cache write, no server tool, standard tier, global
  inference) is now complete: it is priced at the model's admission tariff, else the
  pricing snapshot on Anthropic's own endpoint, and an allowance reserves, sends once and
  settles a Claude text call through the Anthropic wire, the model named exactly, by
  nickname (`anthropic/sonnet`) or through the `claude` alias. A reply missing a meter, or
  a Claude Haiku 5.5 prompt above its admitted 100,000 tokens, stays unknown, never zero;
  under an allowance, a Haiku 5.5 request whose bytes could exceed that tier is refused
  before anything is sent.
