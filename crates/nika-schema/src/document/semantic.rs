// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The strict parser's reading of each top-level component of a document
//! (the name, the model, each input, constant, secret, output and task, the
//! permits, the run declaration), with every source span erased. Two
//! documents whose component reads the same here were read the same by the
//! parser, whatever bytes moved around it. The form is the AST's own derived
//! `Debug`, so a field the AST gains later is compared without a second list.

use std::collections::BTreeMap;

use super::Path;
use crate::raw::RawWorkflow;

/// Every component of `workflow` by id (`model` · `inputs/<name>` ·
/// `tasks/<id>` · …), each its span-free AST form.
pub(super) fn components(workflow: &RawWorkflow) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    out.insert(
        "nika".to_owned(),
        erase(&format!("{:?}", workflow.workflow)),
    );
    out.insert("model".to_owned(), erase(&format!("{:?}", workflow.model)));
    out.insert(
        "permits".to_owned(),
        erase(&format!("{:?}", workflow.permits)),
    );
    out.insert("run".to_owned(), erase(&format!("{:?}", workflow.run)));
    for (name, decl) in &workflow.inputs {
        out.insert(
            format!("inputs/{}", name.value),
            erase(&format!("{decl:?}")),
        );
    }
    for (name, decl) in &workflow.consts {
        out.insert(format!("const/{}", name.value), erase(&format!("{decl:?}")));
    }
    for (name, secret) in &workflow.secrets {
        out.insert(
            format!("secrets/{}", name.value),
            erase(&format!("{secret:?}")),
        );
    }
    for (name, decl) in &workflow.outputs {
        out.insert(
            format!("outputs/{}", name.value),
            erase(&format!("{decl:?}")),
        );
    }
    for task in &workflow.tasks {
        out.insert(
            format!("tasks/{}", task.value.id.value),
            erase(&format!("{:?}", task.value)),
        );
    }
    out
}

/// The component a path lies in: `Some("tasks/x")`, `Some("permits")`, or a
/// whole collection as `"tasks/"`; `None` for the root (every component).
pub(super) fn component(path: &Path) -> Option<String> {
    let segments = path.segments();
    let first = segments.first()?;
    if matches!(
        first.as_str(),
        "inputs" | "const" | "secrets" | "outputs" | "tasks"
    ) {
        return Some(match segments.get(1) {
            Some(name) => format!("{first}/{name}"),
            None => format!("{first}/"),
        });
    }
    Some(first.clone())
}

/// Whether component `id` lies inside the touched component `touched`.
pub(super) fn within(id: &str, touched: &str) -> bool {
    id == touched || (touched.ends_with('/') && id.starts_with(touched))
}

/// The `Debug` form with every `span: Span { … }` erased. A string value that
/// happens to spell a span is erased too, which can only make two components
/// look alike: the literal projection, compared beside this, still differs.
fn erase(debug: &str) -> String {
    const OPEN: &str = "span: Span { file: FileId(";
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(len) = span_len(tail) {
            out.push_str("span: _");
            rest = &tail[len..];
        } else {
            out.push_str(OPEN);
            rest = &tail[OPEN.len()..];
        }
    }
    out.push_str(rest);
    out
}

/// The length of `span: Span { file: FileId(N), start: ByteOffset(N), end: ByteOffset(N) }`
/// at the start of `text`, when it is exactly that.
fn span_len(text: &str) -> Option<usize> {
    let parts = [
        "span: Span { file: FileId(",
        "), start: ByteOffset(",
        "), end: ByteOffset(",
        ") }",
    ];
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let digits = text[at..].bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            at += digits;
        }
        if !text[at..].starts_with(part) {
            return None;
        }
        at += part.len();
    }
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::{component, erase, within};
    use crate::document::Path;

    #[test]
    fn spans_are_erased_and_nothing_else() {
        let debug = "Spanned { value: \"x\", span: Span { file: FileId(0), start: ByteOffset(4), end: ByteOffset(9) } }";
        assert_eq!(erase(debug), "Spanned { value: \"x\", span: _ }");
        let partial = "span: Span { file: FileId(x) }";
        assert_eq!(erase(partial), partial);
    }

    #[test]
    fn a_path_names_its_component() {
        assert_eq!(
            component(&Path::new(["tasks", "a", "retry"])).as_deref(),
            Some("tasks/a")
        );
        assert_eq!(component(&Path::new(["tasks"])).as_deref(), Some("tasks/"));
        assert_eq!(
            component(&Path::new(["permits", "fs"])).as_deref(),
            Some("permits")
        );
        assert_eq!(component(&Path::root()), None);
        assert!(within("tasks/a", "tasks/"));
        assert!(within("tasks/a", "tasks/a"));
        assert!(!within("tasks/ab", "tasks/a"));
    }
}
