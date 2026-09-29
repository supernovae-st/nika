- **Decide spelling drops with canonical relations.**
  Spelling checks confirm unmatched values in pairs, then decide a dropped spelling's cause
  with one private counterfactual: every string relation compares canonical (NFC) forms while
  every value keeps its bytes. A drop that disappears is a byte comparison and is refused
  however the literal is written, including one literal that also inspects its own bytes. A
  drop that persists, such as a requested length, threshold or encoding, is recorded for the
  semantic judges and is never a pass. The emitted program is unchanged, and the verifier and
  its probes share one jq engine. An earlier exchange of string constants also refused
  requested values of the literal itself; that overclaim is withdrawn.
