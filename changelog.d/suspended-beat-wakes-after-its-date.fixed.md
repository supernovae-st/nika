- **A suspended beat wakes after its `jusqu_au`.** A beat suspended with `actif: false`
  and `jusqu_au: 2026-08-31` stayed asleep forever under the OS units: `nika arm fire`
  skipped it as inactive at every slot, `nika arm` kept it DORMANT, and `nika arm --emit`
  wrote it no unit, while `nika serve` woke it on the machine's calendar. Both firers now
  read one law: the suspension ends at midnight after its date in the cadence's own zone
  (a `TZ=Europe/Paris` cadence wakes at 00:00 Paris on 1 September), and the beat fires
  again with its silence counted from that wake, so the slots it slept through are
  neither reported missed nor caught up. A suspended beat keeps its OS unit, and
  `nika arm disarm <label> --write` rewrites it instead of deleting it: the firer
  journals « inactive — <raison> · jusqu_au <date> » until the date. A beat still active
  in `nika.yaml` is not disarmed by the flag; the gesture is in the file.
