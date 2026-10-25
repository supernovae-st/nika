- **The ADR index carries `amends`.** `docs/adr/README.md` says `docs/adr/index.json` holds the
  full relation graph, but `scripts/adr/generate-index.sh` emitted every edge except `amends`,
  so the twelve ADRs that amend another (ADR-154 amends three) read as amending nothing. The
  generator now emits it, the index is regenerated, and a hygiene self-test pins the edge.
