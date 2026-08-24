- **The default sandbox arm stays fail-closed (#822).** `NIKA_SANDBOX`
  unset (`auto`) plus a `permits:` workflow plus no OS jail is NIKA-1710,
  not a composed Noop run. The composition gate is tested with a Noop
  decision so deleting the refuse arm cannot hide behind a host that
  ships Seatbelt. Doctor display of the row stays S3.
