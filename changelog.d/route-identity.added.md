- **Providers own a provider route's public identity.** `route_origin` names
  an endpoint `scheme://host:port` from the `url` crate's WHATWG parse, the
  one the transport connects with, and refuses userinfo. `canonical_endpoint`
  accepts only an exact `https` serialization without userinfo, query or
  fragment. `route_label` and `durable_calls` project a route and per-dispatch
  call records to origins, with no endpoint path left in any field. Traces,
  the Run terminal receipt, new cost-journal rows, Session history and the
  review screens now name routes through them; pricing and in-memory
  records keep their exact endpoints.
