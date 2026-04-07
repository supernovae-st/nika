; Nika Workflow Language — Bracket Matching for Zed

(flow_mapping
  "{" @open
  "}" @close)

(flow_sequence
  "[" @open
  "]" @close)

(double_quote_scalar
  "\"" @open
  "\"" @close)

(single_quote_scalar
  "'" @open
  "'" @close)
