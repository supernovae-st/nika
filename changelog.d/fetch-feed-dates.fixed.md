- **An author knows what a fetched feed returns.** The `nika:fetch`
  contract an authoring model reads named the extraction modes only by
  pointing to another page, so an author filtered a feed's items by the
  feed's own date text and kept nothing. It now shows what each mode
  returns, the feed's shape, and that feed dates are RFC 3339 instants
  with an explicit offset; `nika:date` joins the everyday contracts to
  compute a bound such as today.
