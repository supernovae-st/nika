- **An unstable build trims the authoring prompt for evaluation.** The
  `unstable-prompt-trims` feature of `nika-compile-seats`, off by default,
  makes the document door show the language's schema without its
  descriptions (29,749 bytes compact, 9,705 without them). It also narrows
  the callable contracts to what the reader's typed plan reaches: `fetch`
  only for a fetched, looked-up or explored source or an outbound effect,
  `convert` for a computation, `prompt` for an approved effect, `notify`
  for a sent one. A default build sends the whole prompt, as before; the
  feature exists so an evaluation can measure authoring quality with and
  without the trims.
