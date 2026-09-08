`invoke: tool` now checks its final output against the task's `returns:`
contract before publishing success. A violation raises the recoverable,
non-transient `NIKA-TYPE-101` error without discarding the completed call's
attestation or reported cost. An optional string field may be absent, but
a present null or number is rejected; tools without `returns:` are unchanged.
