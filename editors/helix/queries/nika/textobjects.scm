; Nika Workflow — Tree-sitter Text Objects for Helix
;
; Enables structural selection in Helix:
;   maf / mif  — select around/inside function (= task block)
;   mac / mic  — select around/inside class (= workflow document)
;   mae / mie  — select around/inside entry (= key-value pair)
;   ] f / [ f  — jump to next/previous task
;
; Based on tree-sitter-yaml node types used by Helix.

; ═══════════════════════════════════════════════════════════════════
; Function: each task is a "function"
; A task = one block_sequence_item under the tasks: array.
; ═══════════════════════════════════════════════════════════════════

(block_sequence_item
  (block_node
    (block_mapping) @function.inside)) @function.around

; ═══════════════════════════════════════════════════════════════════
; Class: the entire document (the workflow)
; ═══════════════════════════════════════════════════════════════════

(document
  (block_node) @class.inside) @class.around

; ═══════════════════════════════════════════════════════════════════
; Entry: each key-value pair (same as base YAML)
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  (_) @entry.inside) @entry.around

; ═══════════════════════════════════════════════════════════════════
; Comment
; ═══════════════════════════════════════════════════════════════════

(comment) @comment.inside

(comment)+ @comment.around
