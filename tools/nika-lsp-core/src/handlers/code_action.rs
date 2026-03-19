//! Code action handler — quick fixes.

#[derive(Debug, Clone)]
pub struct CodeActionEntry { pub title: String, pub kind: CodeActionKind, pub edit: Option<TextEdit>, pub is_preferred: bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeActionKind { QuickFix, Refactor }
#[derive(Debug, Clone)]
pub struct TextEdit { pub offset: u32, pub end_offset: u32, pub new_text: String }

pub fn code_actions(text: &str, start_offset: u32, _end_offset: u32) -> Vec<CodeActionEntry> {
  let mut actions = Vec::new();
  if !text.contains("schema:") {
    actions.push(CodeActionEntry {
      title: "Add schema version".into(), kind: CodeActionKind::QuickFix, is_preferred: true,
      edit: Some(TextEdit { offset: 0, end_offset: 0, new_text: "schema: nika/workflow@0.12\n".into() }),
    });
  }
  let line_start = text[..start_offset as usize].rfind('\n').map(|p| p + 1).unwrap_or(0);
  let line_end = text[start_offset as usize..].find('\n').map(|p| start_offset as usize + p).unwrap_or(text.len());
  let line = &text[line_start..line_end];
  let trimmed = line.trim();
  if let Some(rest) = trimmed.strip_prefix("infer:") {
    let prompt = rest.trim().trim_matches('"').trim_matches('\'');
    if !prompt.is_empty() && !prompt.contains('\n') {
      let indent: String = " ".repeat(line.len() - trimmed.len());
      actions.push(CodeActionEntry {
        title: "Expand shorthand infer".into(), kind: CodeActionKind::Refactor, is_preferred: false,
        edit: Some(TextEdit { offset: line_start as u32, end_offset: line_end as u32, new_text: format!("{indent}infer:\n{indent}  prompt: |\n{indent}    {prompt}") }),
      });
    }
  }
  actions
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn missing_schema() { assert!(code_actions("workflow: x\n", 0, 0).iter().any(|a| a.title.contains("schema"))); }
  #[test]
  fn has_schema() { assert!(!code_actions("schema: x\n", 0, 0).iter().any(|a| a.title.contains("schema"))); }
  #[test]
  fn expand_infer() { let t = "    infer: \"hi\"\n"; let o = t.find("infer").unwrap() as u32; assert!(code_actions(t, o, o).iter().any(|a| a.title.contains("Expand"))); }
}
