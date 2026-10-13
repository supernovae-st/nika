- **Decision partitions retain the exact serialized request bytes.**
  `Partition::begin` returns the optional carried request as `Vec<u8>` instead of
  a JSON value. Callers must preserve those bytes rather than serialize a parsed
  value again. Model-part qualification also exposes its typed findings; this
  preview's public API change is not exclusively additive.
