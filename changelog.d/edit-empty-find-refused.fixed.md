- **`nika:edit` refuses an empty `find:` instead of rewriting the whole file.** An empty
  `find:`, such as a templated input left empty, matched between every character: the
  replacement landed between every byte (`price: 10` became `XpXrXiXcXeX…`) and the
  task succeeded. It is now `NIKA-BUILTIN-EDIT-001`, refused before the file is read.
