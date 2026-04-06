// Pure YAML parsing utilities — no vscode dependency.
// Extracted for testability with vitest.

const TASK_ID_REGEX = /^\s*-\s*id:\s*(\S+)/;
const VERB_REGEX = /^\s+(infer|exec|fetch|invoke|agent)[\s:]/;

export interface ParsedTask {
  id: string;
  line: number;
  verb: string;
}

export function parseWorkflowTasks(content: string): ParsedTask[] {
  const lines = content.split('\n');
  const tasks: ParsedTask[] = [];
  let currentTask: { id: string; line: number } | null = null;

  for (let i = 0; i < lines.length; i++) {
    const idMatch = lines[i].match(TASK_ID_REGEX);
    if (idMatch) {
      if (currentTask) {
        tasks.push({ ...currentTask, verb: 'unknown' });
      }
      currentTask = { id: idMatch[1], line: i };
      continue;
    }

    if (currentTask) {
      const verbMatch = lines[i].match(VERB_REGEX);
      if (verbMatch) {
        tasks.push({ id: currentTask.id, line: currentTask.line, verb: verbMatch[1] });
        currentTask = null;
      }
    }
  }

  if (currentTask) {
    tasks.push({ ...currentTask, verb: 'unknown' });
  }

  return tasks;
}
