# ENGINE-CONTROLLED PHASE · source recovery · the whole `.nika`, judged like any candidate

This section is the engine's, appended to these instructions when the phase opens; it supersedes
the sketch instruction above for the rest of the conversation. The structured doors ended without
an accepted candidate for this request, and the operator's explicit recovery policy now asks YOU
for the complete `.nika` source. The request, the facts the compiler holds you to,
the observed world and the answers already given (the first message) are unchanged, and every
finding below still applies: do not repeat what was refused.

Answer one JSON object `{"candidate", "candidate_lines", "questions", "gaps", "notes"}`:

- `candidate` · the whole workflow as YAML text with real newlines — or leave it empty and send
  `candidate_lines`, one physical line per element, indentation preserved.
- `permits:` · grant exactly what each task reaches: the stated paths as literals, the stated
  hosts as literals, never a wildcard. An absent grant is zero authority.
- Read every source the request names and write every destination it names, at the exact path
  it states; gate every approval the request states with a `nika:prompt` before the effect it
  guards; perform no effect the request prohibits; invent no path, host or value.
- `questions` · only business values the request leaves open, each `const.<snake_slug>` and
  declared empty under `const:` (`<slug>: ""`). A column, field or value the observed world
  states is written, never asked; a jq program, a glob or a pattern is yours to write.
- `gaps` · each clause of the request you cannot realize, verbatim. A clause never disappears.
- `notes` · one line.

The compiler parses, checks and judges your source against the original request, rehearses it
when a rehearsal host is offered, and asks the whole-request judgment before anything is READY.
You grant nothing and run nothing: saving and running require the caller's explicit action.
