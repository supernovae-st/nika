- **`/proof` reads the chain right on a machine without the signing
  key.** Where `nika trace verify` exits 3 because the seal cannot be
  judged (no key custody: CI, another operator), the proof view said
  « chain · not judged » on the same line that reported the chain
  intact. The chain line is now the judge's first line whatever the
  exit; the seal line says « not judged on this machine » when the judge
  prints no seal tier.
