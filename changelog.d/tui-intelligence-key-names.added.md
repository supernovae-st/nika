- **`/intelligence` names the key a provider uses and can test it.** API
  rows show the variable that supplies the key (`key: ANTHROPIC_API_KEY`)
  and warn when two set variables differ, naming the one Nika uses. A
  `Test the key` row on the current or opened API connection sends one free
  model-list request only when you pick it and shows the verdict inline
  (valid, refused, unreachable, unknown); the key is never printed.
