- **An unsealed journal now points at the run key, not at `nika sign`.** `nika trace verify` said
  « `nika sign` seals future runs » and the Session's `/proof` said « `nika sign` signs future runs ».
  `nika sign` binds a workflow file to its author; it never seals a run. Both now say that a run key
  (`nika key init`) makes later runs seal, which is what turns the next journal SEALED.
