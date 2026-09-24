- **Interrupted Session inference keeps its monetary exposure visible.** A
  Session request under an explicit monetary account that may be in flight
  is now recorded before it can leave: `.nika/session-state.json` carries a
  line saying the request may have been sent and billed, and only the write
  after its settlement removes it. A session that leaves during the call
  (two `Ctrl+C`) restores with that exposure and replays nothing. Its
  refusal now names the supported way on: restate a saved workflow's Run
  with an explicit ceiling. The conversation history records the same
  uncertainty when a call ends without usable settlement.
