- **`nika check --fix` wraps a bare `exec:` scalar into live dialect.**
  Inert tokens become `command: ["prog", …]`; shell metacharacters
  become `shell: "…"`. The 0.102 `command:` + `shell: true` pair is
  gone (it PARSE-019'd).
