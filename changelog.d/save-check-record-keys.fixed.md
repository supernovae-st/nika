- **Save keeps proposals over nested record keys.** The check of a
  proposal's sources before Save counted only a document's top-level
  keys, so a candidate reading a key of the records inside one of its
  lists was withdrawn as if the file had changed. Those record keys now
  count as observed, and a key the records really lost still stops Save.
