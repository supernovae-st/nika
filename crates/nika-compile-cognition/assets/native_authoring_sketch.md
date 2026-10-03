# The sketch · structure first, words second

You do not write the `.nika` file. You write a SKETCH of it, and later you fill its typed holes;
the compiler emits the file, derives every permit from what your tasks reach, and judges the
result against the request as it judges any candidate.

## Call 1 · the sketch

Answer one JSON object: `{"name": "<kebab-name>", "tasks": [...], "questions": [...], "gaps": [...],
"notes": "<one line>"}`. Each task is `{"id", "verb", "tool"?, "reads"?, "writes"?, "hosts"?,
"after"?, "with"?, "gated_by"?, "for_each"?, "purpose"}`:

- `id` · snake_case, unique; a task references only EARLIER tasks.
- `verb` · one of `infer` (one model call), `invoke` (a builtin `nika:<name>` or an MCP tool
  `mcp:<server>/<tool>`, named in `tool`), `exec` (a process), `agent` (a governed loop). Only an
  `invoke` names a `tool`.
- `reads` / `writes` · the paths the task reads or writes, EXACTLY as the request states them (a
  file, a folder, a glob the request implies). Nothing the request never states. A tool that
  always reads or writes a file states it here: an `edit` in both `reads` and `writes`, a chart,
  an image or a speech output in `writes`, an image filter's source in `reads`.
- `hosts` · the hosts a `nika:fetch` or `nika:notify` reaches, exactly as stated. A system the
  request names without a host (a CRM, « notre API ») has NO host here: its endpoint is a
  placeholder you will fill as `${{ const.<system>_endpoint }}` and ask under `questions`.
- `with` · the data edges: `[{"name": "<local name>", "from": "<earlier task id>"}]`; the task
  reads `${{ with.<name> }}` where the compiler binds that task's output. A name is a snake_case
  identifier, once per task, never `approved` on a gated task nor `items` on a looping one (the
  compiler binds those).
- `after` · control edges to earlier tasks whose output the task does not read.
- `gated_by` · the earlier `nika:prompt` task whose answer must approve this effect (every
  approval the request states is one such task before the effect it guards).
- `for_each` · the earlier task whose output is the list this task loops over.
- `purpose` · one line on what the task is for: the hole's brief to yourself.

`questions` are the business values the request leaves open (`const.<snake_slug>`, a label, a
type, why); `gaps` name the clauses no task realizes. Structure only: no prompt, no jq, no
argument — those are the holes. A task carries only the fields above, each of its type (a
string, or an array of strings); a field outside them or a value of another type refuses the
sketch by its path, nothing is dropped.

## Call 2 · the holes

The compiler lists the holes the accepted sketch leaves, each as `task.field` with a kind and
the task's purpose: `prompt` (text; reads `${{ with.<name> }}` and `${{ inputs.<x> }}` only),
`schema` (a JSON schema object, optional), `expression` (a jq program over the task's `input`),
`command` (an argv array), `args.<name>` (one builtin argument, the builtin's own name),
`args` (the whole argument object of another builtin). Answer `{"fills": [{"task", "field",
"value"}], "notes"}` and fill ONLY the listed holes, each once, each with its `value` of the
hole's kind (a nonempty string for a prompt, template, jq program, URL or mode; an object for a
schema or an `args` object; a nonempty array of strings for an argv); fill every hole not marked
optional. The sketch owns every path or glob a task reaches, its edge input, its edge-bound
content and its channel: an `args` object never names one of them (another tool's own argument
of the same name stays yours), and a content template reads every edge its task is bound to. A
builtin's own file argument (a chart's `out` or `data.path`, an image's `input` or `output_dir`,
a speech `output_dir`) is one of the paths its task states in `reads` or `writes`, never another
task's. A value that names a path, a host or a tool the sketch never stated is refused; an open
endpoint is `${{ const.<system>_endpoint }}`. A refusal names the hole; you fill that hole
again, never the whole file.
