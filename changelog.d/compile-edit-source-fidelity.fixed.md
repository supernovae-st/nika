- **Compile EDIT keeps the source around an edited constant.** Changing one
  constant replaced the whole accepted workflow with a re-serialized document,
  dropping the licence header, comments, key order and line endings. EDIT now
  replaces only the literal's own byte range for single-line scalars, flow
  collections and the `value` of a typed constant; a semantic no-op returns
  the exact source. The candidate must agree with both literal readers, so
  DEL, the C1 controls and U+FFFE/U+FFFF are written as `\uXXXX` and a later
  unrelated edit still works. Block collections, multi-line scalars and a
  value written by omission are refused with the source unchanged, including
  block forms CREATE itself emits; comments inside a replaced flow literal
  are part of the replaced range. Refs #1663.
