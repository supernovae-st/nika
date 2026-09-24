- **Short DeepSeek JSON calls use low thinking effort by default.** On
  catalogued routes with effort control, a structured request capped at
  8,192 output tokens or less asks for low effort, leaving the same finite
  token limit and cost admission in place. Explicit caller settings win;
  other routes and ordinary text generation retain their defaults.
