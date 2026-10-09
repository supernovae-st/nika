- **An endpoint's route is no longer owed as a local directory.** A
  route the request sends to on an endpoint it states
  (`POST /notifications/stock` to the stated local sink) is realized by
  a send to exactly that URL. A send to another route or origin, or a
  GET, still leaves it owed; a local path the request reads stays owed,
  even after a sentence that writes; a file name with spaces keeps its
  exact extent.
