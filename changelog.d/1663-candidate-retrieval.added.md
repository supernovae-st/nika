- **Compile gains a private candidate-retrieval layer with measured recall.**
  `nika_onboard::compile::retrieve` ranks the 22 canonical skeletons and a
  compact projection of the spec's 220 pattern families (Apache-2.0
  development knowledge embedded under `assets/`) with BM25 over one
  normalized token stream: French diacritics fold, function words drop, a
  light stemmer and a closed English/French alias table map everyday
  operation words onto the pattern vocabulary. `retrieve_by_ops` takes the
  operation words of a semantic plan. A hit is a candidate to read, never a
  selection verdict or authority; the hermetic recall test prints recall@1 and
  recall@5 on 30 seen example intents and 10 unseen paraphrases.
