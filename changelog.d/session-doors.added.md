- **A conversation's run can be steered, stopped and watched while it works.** The Session
  host (`nika/session-host@1`, both doors) gains `steer` and `follow_up`: a line sent while a
  turn of the conversation runs is queued with its identity (`l1`, `l2`, …) and shown in
  `busy.queued`, then enters after the calls under way (an ACP agent is asked once to stop) or
  when the run would end, as the person's next cited line; a line no run reads is refused,
  never kept for later. Stop of such a turn now settles as the typed outcome `stopped` (how it
  reached the intelligence, the lines returned unsent, the draft kept). Each tool the
  conversation's intelligence calls reaches the event stream as typed activity (call, tool,
  started/finished/failed, elapsed), never its arguments. On `nika/session-work@0`, additively:
  `queued`, and a `choice` on each offered model this machine's inventory knows (route,
  billing, list price).
