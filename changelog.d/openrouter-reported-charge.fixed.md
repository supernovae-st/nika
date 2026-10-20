- **A paid OpenRouter call settles at the charge OpenRouter reports.** A paid
  OpenRouter reply always read as unknown cost, and an admitted paid route would have
  turned its account Uncertain at the first settlement. A non-BYOK reply from
  OpenRouter's own endpoint with complete usage now settles at its `usage.cost`, which
  receipts name as a provider-reported charge; its catalogue tariff only reserves. A
  BYOK reply, a gateway's `cost` or an unknown fee stays unknown. A charge above the
  reservation still settles known, and that route then refuses its next call until its
  catalogue tariff is refreshed.
