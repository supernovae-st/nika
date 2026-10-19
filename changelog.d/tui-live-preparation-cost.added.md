- **The live line shows what preparation costs while Nika works.** While Nika prepares, the line
  now sums what the returned author calls cost as the Session settled them, at list price:
  `Preparing · 45s · 12 author calls · ~$0.12 · Ctrl+C stop`. A call that returned without a
  price is counted beside the sum (`~$0.12 + 2 unpriced`); while no call is priced, no cost is
  shown at all. A finished call's activity mark now carries its settled cost (`CallCost`), read
  from the preparation scope's own journal of returned requests: an estimate, never an invoice.
