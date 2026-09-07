// Focused source excerpts, checked against Nika 0.116.2's builtin catalog.
// Setup and other tasks are explicitly folded; this is not a runnable export.
export const YAML_EXCERPTS = {
  csv: `  csv:
    invoke:
      tool: nika:read
      args:
        path: ./sales.csv`,
  parse: `  parse:
    with:
      csv: \u0024{{ tasks.csv.output }}
    invoke:
      tool: nika:convert
      args:
        input: \u0024{{ with.csv }}
        from: csv
        to: json`,
  pages_a: `  pages:
    for_each:
      items: \u0024{{ const.competitors }}
      max_parallel: 3
    retry:
      max_attempts: 3
    timeout: "30s"
    invoke:
      tool: nika:fetch
      args:
        url: "\u0024{{ item }}"
        mode: article`,
};

const escapeHtml = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
export function renderYaml(source) {
  return source.split('\n').map((line, index) => {
    const match = line.match(/^(\s*)([a-z_]+):(.*)$/);
    const html = match ? `${match[1]}<b>${match[2]}:</b><span>${escapeHtml(match[3])}</span>` : escapeHtml(line);
    return `<span class="yaml-source-line ${index === 0 ? 'yaml-task-name' : ''}">${html}</span>`;
  }).join('');
}
