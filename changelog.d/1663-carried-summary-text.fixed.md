- **A computed summary carried to a webhook is posted as its JSON text.** The carry law
  already serialized the computed rows, the extracted fields, a validation and the records
  through a `<slug>_text` stage; the compute summary (a count, a total) is one of them.
  Measured on the sealed-v3 lane: a request posting the number of affected tanks checked
  clean and failed at run because `nika:notify` refused a message that was an object.
