; Nika Workflow Language — Language Injections for Zed
;
; Nika workflows are YAML with embedded content. We do not inject
; a separate language since we use the YAML grammar directly and
; apply Nika-specific highlighting through highlights.scm.
;
; Future: if Nika gains a dedicated tree-sitter grammar, template
; expressions ({{...}}) could be injected as a separate language.
