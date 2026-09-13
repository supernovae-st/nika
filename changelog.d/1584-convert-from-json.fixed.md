- **`nika:convert` reads a string input as JSON text under `from: json`.** A
  JSON document handed over as a string — an exec stdout, a `nika:read`, a
  `recover:` literal — is parsed the way every other `from:` format reads its
  text, so `to: csv` no longer refuses it with « CSV output needs an array of
  objects »; a string that is not JSON text stays the string value it always
  was (#1584).
