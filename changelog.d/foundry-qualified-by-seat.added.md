- **The decision model now qualifies recalled Foundry knowledge before
  the author reads it.** Each reference the word and graph recall found
  is put to the selected decision model against the whole request; one
  it finds unrelated never reaches the author, one it cannot judge stays
  shown as a hypothesis. `decision.knowledge_qualification` records what
  was found, shown, discarded and unqualified, every answer, and which
  shown code the candidate kept. Without a decision model the recall is
  shown unqualified and the record says so.
