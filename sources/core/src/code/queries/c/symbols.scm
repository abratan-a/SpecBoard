; Definitions: @definition.<of>.<word> on the node, @name on its name. A type is one with its body
; (`struct order *o` names one, it does not define it).
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition.function.function
(function_definition declarator: (pointer_declarator declarator: (function_declarator declarator: (identifier) @name))) @definition.function.function
(function_definition declarator: (pointer_declarator declarator: (pointer_declarator declarator: (function_declarator declarator: (identifier) @name)))) @definition.function.function
(declaration declarator: (function_declarator declarator: (identifier) @name)) @definition.function.prototype
(declaration declarator: (pointer_declarator declarator: (function_declarator declarator: (identifier) @name))) @definition.function.prototype
(struct_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.type.struct
(union_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.type.union
(enum_specifier name: (type_identifier) @name body: (enumerator_list)) @definition.type.enum
(type_definition declarator: (type_identifier) @name) @definition.type.typedef
(preproc_def name: (identifier) @name) @definition.constant.define
(preproc_function_def name: (identifier) @name) @definition.macro.define
((translation_unit
  (declaration
    (type_qualifier) @qualifier
    declarator: (init_declarator declarator: (identifier) @name)) @definition.constant.const)
  (#eq? @qualifier "const"))

; The names a symbol calls.
(call_expression function: (identifier) @call)
(call_expression function: (field_expression field: (field_identifier) @call))
