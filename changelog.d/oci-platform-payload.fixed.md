- Release payload verification now pulls each platform by the child digest
  selected from the validated OCI index. This avoids classic Docker's
  multi-platform parent-digest collision without changing daemon storage,
  deleting image references, executing image content, or weakening either
  native binary hash comparison. Invalid parent indexes refuse before pulls.
