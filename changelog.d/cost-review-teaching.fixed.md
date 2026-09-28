- **Serve's cost-review guidance names the right remedy and keeps its route.**
  The per-run ceiling's remedy (`--run-cost-ceiling none`) now accompanies
  only the refusal the ceiling itself causes. A review refused for another
  reason (an unsupported shape, a project `ceiling:` of zero, a held lease)
  keeps its own words. A refused job's bounded message no longer erases the
  review route it names (`POST /v1/cost-reviews`). Only this server's exact
  route literals survive the path scrub; every other path-like token is still
  dropped.
