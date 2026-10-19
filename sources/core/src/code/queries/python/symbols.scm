; Definitions: @definition.<of>.<word> on the node, @name on its name. A module's names in capitals
; are its constants, as Python writes them.
(class_definition name: (identifier) @name) @definition.type.class
(function_definition name: (identifier) @name) @definition.function.def
(module
  (expression_statement
    (assignment left: (identifier) @name)) @definition.constant.constant
  (#match? @name "^[A-Z][A-Z0-9_]*$"))

; The names a symbol calls.
(call function: (identifier) @call)
(call function: (attribute attribute: (identifier) @call))
