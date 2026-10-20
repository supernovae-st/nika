- **An agent account that cannot write workflows says so before you choose it.** The
  intelligence list offered « copilot · Agent account » with no warning. Once chosen, every
  request failed with « authoring unavailable · seat `copilot` is not infer-grade for
  json_schema ». The list now reads « copilot · Agent account · cannot write workflows here » on
  the connection's row and on its own. The check is the same one applied once a seat is chosen:
  one turn, no implicit tool, an answer checked against a JSON schema, and an observable model.
  Accounts that can write workflows show nothing new.
