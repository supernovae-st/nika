# Runbook · checkout-api

**Owner** · payments · **Tier** · 1 (revenue-path) · **SLO** · 99.95% / 30d

## What it does

Terminates the checkout POST, validates the cart, calls the PSP, writes the
order row. Stateless behind the edge; the only stateful dependency is
`orders-db` (primary + one replica).

## Health

| Signal | Where | Healthy |
|---|---|---|
| `checkout_api_5xx_rate` | metrics | < 0.1% over 5m |
| `psp_latency_p99` | metrics | < 1200 ms |
| `orders_db_conns_in_use` | metrics | < 80% of pool |
| service state | status endpoint `.current.state` | `operational` |

## Known failure modes

1. **PSP timeout storm** — the provider degrades, requests pile up, the
   connection pool saturates. *Mitigation ·* raise the PSP client timeout floor
   and shed traffic at the edge. Recovers on its own once the provider does.
2. **Pool exhaustion after a deploy** — a release that widens a query holds
   connections longer. *Mitigation ·* roll back the release first, tune second.
3. **Replica lag > 30s** — reads served stale, duplicate order rows appear.
   *Mitigation ·* pin reads to primary, then let the replica catch up.

## First five minutes

1. Read the status endpoint. Note the state and the last transition.
2. `git log --since="<incident window>"` — a release inside the window is the
   first suspect, every time.
3. Check `orders_db_conns_in_use`. Saturated pool → failure mode 1 or 2.
4. If a release is in the window, roll it back before diagnosing further.
5. Declare in `#incident-checkout`. Timestamps, not adjectives.

## Do not

- Do not restart the pods to "clear it" before capturing the pool metrics —
  that erases the evidence that distinguishes mode 1 from mode 2.
- Do not mark the incident resolved on a single green poll. Wait one settle
  window and re-check; flapping recoveries are the norm here.
