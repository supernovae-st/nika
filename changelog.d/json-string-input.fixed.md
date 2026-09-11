- **`nika:jq` and `nika:validate` explain JSON containers passed as strings
  (#1520).** A type error on an encoded object or array now names the input
  shape and teaches explicit `fromjson` decoding after `nika:read`, without
  repeating the input's contents. String operations, string schemas and
  explicit decoding retain their behavior; validation keeps its structured
  report and error paths. No implicit parsing or file access is added.
