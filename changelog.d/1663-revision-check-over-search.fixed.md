- **A revision check over a search reruns the search just before the action.** The
  recheck only knew a lookup's record; a request that retrieves passages from a guide, drafts
  an answer and asks to recheck the current version before posting was refused for want of a
  retrievable source (eco-60 E09B). The reader admits a search as that source, and the
  assembler reruns the grep under `revision_reread`, compares the hits with the first ones
  and lets `revision_admit` gate the action; changed hits are a changed version.
