- **A Serve answer round can be judged without authoring again.** After
  a native round asked questions, a client could replay it with its
  answers and no provider call, but nothing then judged the replayed
  workflow, so it stayed incomplete; getting it ready meant a new
  authoring round, which spent again and could write other bytes. A
  request with `cognition: "explicitProvider"` and the round's
  `replay_token` now replays the kept plan with the answers and asks the
  server's seat only to judge it. A workflow the seat accepts is ready;
  one it does not accept is held and its token forgotten, so those bytes
  are never put to the same judge again through it.
