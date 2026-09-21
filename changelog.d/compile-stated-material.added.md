- **A stated folder or file inside an object is the material it reads.**
  « fais-moi un digest des notes dans ./notes et écris-le dans ./digest.md »
  and « Traduis ./notes/brief.md en anglais et écris la traduction dans
  ./out/brief-en.md » compile on the deterministic door: a make head
  (`fais-moi`, `fammi`, `hazme`) drafts only when its object opens with a
  produced-content noun; a local path inside the object of a draft, extract,
  classify, validate or compute is the material the operation consumes when
  it is not a destination, so a read step of that path is emitted and no
  phantom `inputs.item` is declared; a folder is read as every file directly
  under it (`./notes` → `./notes/*`), baked into the candidate where the
  human sees and may edit it.
