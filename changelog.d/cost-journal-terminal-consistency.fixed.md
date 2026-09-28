- **A contradictory cost settlement no longer clears an earlier Run.** A
  `settled` row that says its account is still open, counts unknown calls that
  no sent attempt records, or reports a negative or unbacked known subtotal is
  now refused as a conflict named by its digest, like a row from a foreign
  writer: the earlier Run keeps blocking and the next unknown-cost Run is not
  offered a fresh choice. A `prepared` row whose account already moved is
  refused the same way. Every row an engine writes reads as before, including a
  completed unknown-cost call whose USD price stays unknown.
