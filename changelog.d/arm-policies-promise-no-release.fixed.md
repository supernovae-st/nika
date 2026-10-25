- **Unsupported schedule policies state the present limit, not a future release.** `nika arm fire`
  refused `chevauchement: remplacer`, `après_saut: à-complétion`, `manqué: rattraper` and
  `décalage:` with « arrive avec serve v0.2 », the unit emitter said the same, and `nika arm` read
  « not supported by this firer (arrives with serve v0.2) ». No such release is planned. The firer
  now says « ce tireur ne tient pas cette politique » (this firer does not keep this policy), then
  what it does today, as before. For the two catch-up policies, `rattraper` and
  `{ rattraper: N }`, which the resident `nika serve` already keeps, the refusal adds « `nika serve`
  la tient » and `nika arm` adds « `nika serve` keeps it ».
