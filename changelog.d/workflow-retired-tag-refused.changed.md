- **A workflow named exactly `v1`, `v2`, … must be renamed.** `nika:`
  names the file, and `v1` is the retired schema tag it once held. A
  workflow named that way is now refused at parse (`NIKA-PARSE-003`) with
  a remedy that says so, as `nika.yaml` already refused it. Only the
  whole marker is refused: `vault`, `v2ray` and `v1-migration` stay
  ordinary names.
