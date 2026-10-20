- **The card under a resumed run counts the leg before its gate.** A run that a human gate split
  in two showed no cost on its card, because the resumed leg made no model call of its own and
  the paused leg leaves no card. The card now adds what the earlier legs observed in this
  session spent, following `resumed_from`: « ✔ Ran in 1.3s · … · $0.0438 · 1 model call ». The
  session's spend so far counts the paused leg too, so the card, the Session's summary and the
  status row agree.
