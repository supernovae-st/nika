- **Server authoring requires explicit authority for extra requests.**
  Native authoring defaults to one physical request, separates repair
  preferences from request grants, and records requested and observed model
  identities and grant usage. Caller limits can only narrow the operator
  ceiling. Redirects are disabled; provider retries and fallback requests
  consume the same explicit grant and appear in physical-request counters.
