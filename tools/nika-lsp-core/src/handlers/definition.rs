//! Go-to-definition handler.
use crate::analysis::context::CursorContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionResult { pub offset: u32, pub end_offset: u32, pub file: Option<String> }

pub fn definition(text: &str, _offset: u32, context: &CursorContext) -> Option<DefinitionResult> {
  match context {
    CursorContext::DependsOn { prefix, .. } => find_task_def(text, prefix.trim()),
    CursorContext::WithBlock { partial_ref, .. } => find_task_def(text, partial_ref.trim().trim_start_matches('$')),
    CursorContext::Template { partial_expr, .. } => {
      if let Some(rest) = partial_expr.strip_prefix("with.") {
        let alias = rest.split('.').next().unwrap_or("");
        find_with_source(text, alias)
      } else { None }
    }
    _ => None,
  }
}

fn find_task_def(text: &str, name: &str) -> Option<DefinitionResult> {
  if name.is_empty() { return None; }
  for needle in [format!("- id: {name}"), format!("- id: \"{name}\""), format!("- id: '{name}'")] {
    if let Some(pos) = text.find(&needle) {
      return Some(DefinitionResult { offset: pos as u32, end_offset: (pos + needle.len()) as u32, file: None });
    }
  }
  None
}

fn find_with_source(text: &str, alias: &str) -> Option<DefinitionResult> {
  let pat = format!("{alias}: $");
  if let Some(pos) = text.find(&pat) {
    let after = &text[pos + pat.len()..];
    let task_ref: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
    if !task_ref.is_empty() { return find_task_def(text, &task_ref); }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn find_task() { assert!(find_task_def("tasks:\n  - id: step1\n", "step1").is_some()); }
  #[test]
  fn not_found() { assert!(find_task_def("tasks:\n  - id: step1\n", "nope").is_none()); }
  #[test]
  fn depends_on() {
    let ctx = CursorContext::DependsOn { task_id: None, existing_deps: vec![], prefix: "step1".into() };
    assert!(definition("tasks:\n  - id: step1\n", 0, &ctx).is_some());
  }
  #[test]
  fn with_ref() {
    let ctx = CursorContext::WithBlock { task_id: None, alias: None, partial_ref: "$step1".into() };
    assert!(definition("tasks:\n  - id: step1\n", 0, &ctx).is_some());
  }
}
