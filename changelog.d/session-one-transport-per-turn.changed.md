- **A Session turn keeps one connection to its model route.** The
  conversation's calls in one turn share one client and its runtime,
  built at the turn's first call, instead of a new client per request;
  each request is still admitted and can still be stopped on its own.
