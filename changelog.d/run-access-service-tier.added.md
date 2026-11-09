- **A workflow can ask its route for a service tier.** `run.access` gains
  `service_tier`, with the two words `standard` and `fast` (spec 01
  §run). On the codex seat over ACP (`via: codex`, `protocol: acp`) the
  session selects the model, the effort, the tier and the confinement
  mode in that order, then reads the whole selection back before its
  first prompt; `fast` is sent as codex-acp's `fast-mode: on` and
  `standard` as `off`. The binding is held for codex-acp 1.13.1 only:
  another release of the adapter, an adapter with no measured speed
  control, a model whose session lists no such option, or a read-back
  that does not hold the value refuses before the prompt (`NIKA-1805`).
  A tier no route can carry is refused before task 1 by `nika check` and
  by the run (`NIKA-1800`): an API or local route, a seat without a
  measured control, and an `infer:` task outside `protocol: acp`. The
  `access_selection` receipt gains a `service_tier` block only when the
  file declares a tier: `requested`, `option`, `offered`, `transmitted`,
  `configured` and `configured_source`, plus `turn` and `effective`,
  which read `unknown` because no ACP answer names the tier a turn
  carried or the tier that served it. Omitting the key is not
  `standard`: nothing is sent, the route keeps its own tier, and the
  file keeps its resume identity and its receipt. A resumed run whose
  tier changed re-runs its `infer:` and `agent:` tasks. A child workflow
  resolves its own declaration and never inherits its parent's tier.
  `nika --sdk-identity` and a resident's `/health` list
  `accessServiceTier` in `supportedCapabilities`: an engine without the
  token refuses the key at parse, so a client requires it before it
  admits a file that declares a tier. On `nika serve` the tier is a field
  of the file, never of the request, and the per-call receipt stays in
  the sealed journal. An editor served by an older `nika lsp` refuses the
  new key and loses hover and navigation on the file until it is updated.
