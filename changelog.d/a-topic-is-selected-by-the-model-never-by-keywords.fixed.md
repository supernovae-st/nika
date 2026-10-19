- **A topic is selected by the model, never by keywords.** « les 5 top stories de la front page
  qui parlent d'IA » was written as a fixed keyword regex: it missed AI stories that do not use
  the words and kept stories that only mention them, so one story in five was wrong in every run.
  The author is now told that selecting items by what they are about is a judgment: an `infer`
  step returns the matching identifiers, and a deterministic step keeps them in the source's
  order. A filter on words the person names exactly stays deterministic.
