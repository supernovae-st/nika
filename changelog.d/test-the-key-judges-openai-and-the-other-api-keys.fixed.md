- **« Test the key » judges an OpenAI, xAI, DeepSeek, Mistral, Groq, OpenRouter or Gemini key.**
  Only Anthropic's key could be checked: an invalid OpenAI key read « unknown », and its HTTP 401
  came with the first message. On each provider's own endpoint, one free GET of a listing that
  authenticates the key now says valid or refused (Gemini's `API_KEY_INVALID` included); an
  operator's endpoint, which may list to anyone, is still not judged.
