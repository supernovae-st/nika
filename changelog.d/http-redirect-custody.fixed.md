- **Keep redirect credentials within their authorized origin.**
  HTTP redirects remove target URL credentials when crossing origins, so a
  response cannot reintroduce Basic authentication after sensitive headers are
  stripped. Invalid redirect diagnostics omit the response's Location value,
  keeping its credentials, path and query out of errors and traces. Same-origin
  relative redirects retain caller-supplied Basic authentication.
