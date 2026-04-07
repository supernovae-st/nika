; Nika Workflow — Tree-sitter Indentation for Helix
;
; Controls automatic indentation when pressing Enter.
; Nika workflows use 2-space indentation (standard YAML).
;
; Based on Helix's tree-sitter-yaml indent patterns.

; Block scalars (literal | and folded >) preserve indentation
(block_scalar) @indent @extend

; Indent sequence items only if they span more than one line, e.g.
;
; - id: my_task
;   infer: "prompt"
;
; but not single-line items:
; - value
((block_sequence_item) @item @indent.always @extend
  (#not-one-line? @item))

; Map pair without a value (key followed by newline):
;
; tasks:
;   <cursor indented here>
((block_mapping_pair
    key: (_) @key
    !value
  ) @indent.always @extend
)

; Map pair where key and value are on different lines:
;
; infer:
;   prompt: |
;     Multi-line content
((block_mapping_pair
    key: (_) @key
    value: (_) @val
    (#not-same-line? @key @val)
  ) @indent.always @extend
)
