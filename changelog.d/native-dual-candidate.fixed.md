- **The native door reads one candidate when the seat fills both answer
  fields.** In 0.121.0 a seat that filled both `candidate` and
  `candidate_lines` stopped the native door with
  « answer carries two candidates » before any judgment (4 of 4 live runs of
  one seat). Two byte-identical texts are now one candidate, judged with no
  extra call; different texts are refused as conflicting, and nothing is
  chosen or bought. `candidate_lines` reads as its elements joined with a
  line feed, and `candidate` reads as sent. The same rule holds for the JSON
  objects of one seat text, in the native, sketch and proposal decoders: two
  different answers are refused instead of reading the first, an identical
  repetition is one answer, and template or prose braces no longer buy a
  syntax repair.
