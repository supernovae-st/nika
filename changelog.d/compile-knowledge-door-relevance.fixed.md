- **The knowledge door keeps relevance, a secondary obligation's block and an honest
  receipt (builder `knowledge-door-v3`).**
  - Patterns and blocks were deduplicated in a map, then cut to their caps in id order.
    A pattern the request matched word for word could lose its slot to generic rows
    whose ids sort first. The leading family's blocks could also crowd out the block
    of a second obligation.
  - Both are now taken in turn, per recalled family and then by direct match, and
    each source's best-covering block comes first.
  - A row the byte cap left out was still recorded as selected, as if presented. The
    receipt now separates available, selected, excluded with a reason, and presented.
    It states `no_match` when nothing is recalled, and the seat then reads the card
    alone.
  - A block is presented with its row's holes, effects, authority, capabilities,
    callables, known failure modes and version, beside its code (at most 1 KiB).
  - The record names its selector: the door's Rust BM25 over the Foundry graph, not
    the Foundry producer's selection.
  - The shared reader lives beside authoring in the existing `nika-onboard`
    member; the CLI-host compatibility path is preserved without duplicating logic.
  - The bounded decision adapter is hosted beside the shared TypeSafe transport;
    Session retains its monetary admission and persisted observations.
