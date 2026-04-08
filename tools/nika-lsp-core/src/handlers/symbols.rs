// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Document symbols handler — outline view.

#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub offset: u32,
    pub end_offset: u32,
    pub children: Vec<SymbolEntry>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    File,
    Module,
    Function,
    Property,
    Variable,
}

pub fn document_symbols(text: &str) -> Vec<SymbolEntry> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let offset = byte_off(text, i);
        if trimmed.starts_with("schema:") {
            symbols.push(SymbolEntry {
                name: trimmed.to_string(),
                kind: SymbolKind::Property,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("workflow:") {
            symbols.push(SymbolEntry {
                name: trimmed.to_string(),
                kind: SymbolKind::File,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed == "tasks:" {
            let tasks = extract_tasks(&lines, i + 1, text);
            let end = tasks
                .last()
                .map(|t| t.end_offset)
                .unwrap_or((offset + line.len()) as u32);
            symbols.push(SymbolEntry {
                name: format!("tasks ({})", tasks.len()),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: end,
                children: tasks,
            });
        } else if trimmed.starts_with("provider:") {
            symbols.push(SymbolEntry {
                name: trimmed.to_string(),
                kind: SymbolKind::Property,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("context:") {
            symbols.push(SymbolEntry {
                name: "context".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("include:") {
            symbols.push(SymbolEntry {
                name: "include".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("skills:") {
            symbols.push(SymbolEntry {
                name: "skills".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("agents:") {
            symbols.push(SymbolEntry {
                name: "agents".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("inputs:") {
            symbols.push(SymbolEntry {
                name: "inputs".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("mcp:") {
            let srvs = extract_mcp(&lines, i + 1, text);
            let end = srvs
                .last()
                .map(|s| s.end_offset)
                .unwrap_or((offset + line.len()) as u32);
            symbols.push(SymbolEntry {
                name: format!("mcp ({})", srvs.len()),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: end,
                children: srvs,
            });
        } else if trimmed.starts_with("artifacts:") {
            symbols.push(SymbolEntry {
                name: "artifacts".into(),
                kind: SymbolKind::Module,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("log:") && !line.starts_with(' ') {
            symbols.push(SymbolEntry {
                name: "log".into(),
                kind: SymbolKind::Property,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        } else if trimmed.starts_with("model:") && !line.starts_with(' ') {
            symbols.push(SymbolEntry {
                name: trimmed.to_string(),
                kind: SymbolKind::Property,
                offset: offset as u32,
                end_offset: (offset + line.len()) as u32,
                children: vec![],
            });
        }
    }
    symbols
}

fn extract_tasks(lines: &[&str], start: usize, text: &str) -> Vec<SymbolEntry> {
    let mut tasks = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        let indent = lines[i].len() - trimmed.len();
        if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            break;
        }
        if let Some(id_val) = trimmed.strip_prefix("- id:") {
            let id = id_val.trim().trim_matches('"').trim_matches('\'');
            let ts = byte_off(text, i);
            let mut children = Vec::new();
            let mut te = ts + lines[i].len();
            let mut j = i + 1;
            while j < lines.len() {
                let ct = lines[j].trim();
                let ci = lines[j].len() - ct.len();
                if ci <= indent && !ct.is_empty() && !ct.starts_with('#') {
                    break;
                }
                te = byte_off(text, j) + lines[j].len();
                for v in ["infer", "exec", "fetch", "invoke", "agent"] {
                    if ct.starts_with(&format!("{v}:")) {
                        let co = byte_off(text, j);
                        children.push(SymbolEntry {
                            name: v.into(),
                            kind: SymbolKind::Function,
                            offset: co as u32,
                            end_offset: (co + lines[j].len()) as u32,
                            children: vec![],
                        });
                    }
                }
                // Task sub-fields as outline children
                for (field, sk) in [
                    ("with:", SymbolKind::Variable),
                    ("depends_on:", SymbolKind::Property),
                    ("content:", SymbolKind::Property),
                    ("for_each:", SymbolKind::Property),
                    ("retry:", SymbolKind::Property),
                    ("guardrails:", SymbolKind::Property),
                    ("artifact:", SymbolKind::Property),
                    ("decompose:", SymbolKind::Property),
                ] {
                    if ct.starts_with(field) {
                        let co = byte_off(text, j);
                        children.push(SymbolEntry {
                            name: field.trim_end_matches(':').into(),
                            kind: sk,
                            offset: co as u32,
                            end_offset: (co + lines[j].len()) as u32,
                            children: vec![],
                        });
                    }
                }
                j += 1;
            }
            tasks.push(SymbolEntry {
                name: id.to_string(),
                kind: SymbolKind::Function,
                offset: ts as u32,
                end_offset: te as u32,
                children,
            });
            i = j;
            continue;
        }
        i += 1;
    }
    tasks
}

fn extract_mcp(lines: &[&str], start: usize, text: &str) -> Vec<SymbolEntry> {
    let mut srvs = Vec::new();
    for (i, _) in lines.iter().enumerate().skip(start) {
        let trimmed = lines[i].trim();
        let indent = lines[i].len() - trimmed.len();
        if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            break;
        }
        if indent == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-') {
            let name = trimmed.trim_end_matches(':');
            let o = byte_off(text, i);
            srvs.push(SymbolEntry {
                name: name.into(),
                kind: SymbolKind::Module,
                offset: o as u32,
                end_offset: (o + lines[i].len()) as u32,
                children: vec![],
            });
        }
    }
    srvs
}

fn byte_off(text: &str, line: usize) -> usize {
    let mut o = 0;
    for (i, l) in text.lines().enumerate() {
        if i == line {
            return o;
        }
        o += l.len() + 1;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty() {
        assert!(document_symbols("").is_empty());
    }
    #[test]
    fn schema() {
        let s = document_symbols("schema: v0.12\n");
        assert_eq!(s.len(), 1);
    }
    #[test]
    fn tasks_count() {
        let s = document_symbols("tasks:\n  - id: a\n    infer: x\n  - id: b\n    exec: y\n");
        assert!(s[0].name.contains("2"));
        assert_eq!(s[0].children.len(), 2);
    }
    #[test]
    fn task_children() {
        let s = document_symbols("tasks:\n  - id: a\n    infer: x\n    with:\n      d: x\n");
        let task = &s[0].children[0];
        assert!(task.children.iter().any(|c| c.name == "infer"));
        assert!(task.children.iter().any(|c| c.name == "with"));
    }
    #[test]
    fn mcp_servers() {
        let s = document_symbols("mcp:\n  srv1:\n    cmd: x\n  srv2:\n    cmd: y\n");
        assert!(s[0].name.contains("2"));
    }
    #[test]
    fn task_new_children() {
        let s = document_symbols(
            "tasks:\n  - id: a\n    infer: x\n    for_each: [1,2]\n    retry:\n      max: 3\n    guardrails:\n      - type: length\n",
        );
        let task = &s[0].children[0];
        assert!(task.children.iter().any(|c| c.name == "for_each"));
        assert!(task.children.iter().any(|c| c.name == "retry"));
        assert!(task.children.iter().any(|c| c.name == "guardrails"));
    }
    #[test]
    fn root_artifacts_and_log() {
        let s = document_symbols(
            "schema: v0.12\nartifacts:\n  default: ./out\nlog:\n  level: info\nmodel: gpt-4o\n",
        );
        assert!(s.iter().any(|e| e.name == "artifacts"), "missing artifacts");
        assert!(s.iter().any(|e| e.name == "log"), "missing log");
        assert!(
            s.iter().any(|e| e.name.starts_with("model:")),
            "missing model"
        );
    }
}
