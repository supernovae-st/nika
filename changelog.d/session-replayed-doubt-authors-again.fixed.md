- **A replayed workflow the verifier doubts is never replayed to it
  again.** An answer round replays the workflow and judges it, but never
  repairs it: when the verifier found it unfaithful, the session stopped
  with « built but not proposed » and asked for the request again. When
  the verifier locates a defect (a part of the request it names the
  failing step for, or finds no step performing, or a step doing
  something not asked), the session now writes the workflow again under
  every answer already given, where the authoring round's own judgment
  and repairs run. A verifier that doubted the replay without locating a
  defect gives no defect to write again from: the replay stays held, and
  its record is dropped by the compiler, by the session round (whose
  retry with a stronger model then writes the workflow afresh) and by
  `nika compile`, which removes its plan record and says so, so no later
  round replays those bytes. An answer round over a COLD or WARM plan
  now judges the whole request again on the bytes it replays.
