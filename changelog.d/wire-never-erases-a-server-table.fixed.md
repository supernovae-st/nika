- **`nika wire` no longer erases a client's server table.** When a config held its servers under a
  value that is not a JSON object (an array under `mcpServers`, `servers`, `mcp`,
  `context_servers` or `amp.mcpServers`), five writers replaced it with `{}` and wrote the file,
  and every server the user had listed was gone. Now each writer refuses such a value with « is not
  a JSON object » and leaves the file byte-identical. A TOML `mcp_servers` that is not a table was
  already refused and now says so the same way. The preview says `skipped` instead of « would
  update ». `wire detected` and `wire all` plan every client before the first write: one config
  that cannot be wired as it is leaves every config untouched (exit ENV, « nothing written »), so
  the first client is no longer written before the second is found invalid. Every write is read
  back, and a file that does not hold what was written is restored as it was.
