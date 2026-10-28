- **`nika serve` lists the files a `for_each` fan-out saved.** A job whose
  `for_each:` called `nika:chart` once per item wrote each item's file, yet
  `GET /v1/jobs/{id}/artifacts` listed only the single calls' files: the
  manifest read the tool from an `invoke · <tool>` note, and a fan-out's
  note is `for_each · <n> items`. A fan-out's tool is now read from the
  task's own journal: the one tool every tool-plane permit decision of the
  task allowed, when every item settled ok, no agent frame came from the
  task and its value holds one answer per item. Each item's file is listed
  with its journaled size and digest and downloads like any other. A
  fan-out with a recovered or failed item still lists none of its files.
