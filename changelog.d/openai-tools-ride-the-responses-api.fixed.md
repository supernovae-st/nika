- **GPT-6 and GPT-5.6 work with tools.** Any `agent:` task or tool-using author on GPT-6 Astra,
  Sol, 6.1 Sol or Luna, or GPT-5.6 Sol, failed with HTTP 400 « Function tools with
  reasoning_effort are not supported … use /v1/responses »: these models refuse function tools
  on Chat Completions. A request with function tools to OpenAI's own endpoint now rides its
  Responses API (stateless: `store: false`, reasoning items kept encrypted and sent back in
  place), priced at the same verified tariff. A GPT-6 Astra agent that reads a file now answers
  in three turns ($0.01).
