A Session's project record (`.nika/session-state.json`) now keeps each new cost
observation in the provider-owned durable form: route origins replace full endpoints,
while the accounting fields keep their meaning. The line recorded before an unknown-cost
dispatch names the route's origin instead of its endpoint. Entries already recorded are
kept as written, and the live account's exact route, consent and authority are unchanged.
