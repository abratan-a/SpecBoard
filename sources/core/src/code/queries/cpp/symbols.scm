; Definitions: @definition.<of>.<word> on the node, @name on its name. A member defined outside its
; class is named with its class (`'Order::total'`): the class is often in another file.
(function_definition declarator: (function_declarator declarator: [(identifier) (field_identifier) (qualified_identifier) (operator_name) (destructor_name)] @name)) @definition.function.function
(function_definition declarator: (pointer_declarator declarator: (function_declarator declarator: [(identifier) (field_identifier) (qualified_identifier)] @name))) @definition.function.function
(function_definition declarator: (reference_declarator (function_declarator declarator: [(identifier) (field_identifier) (qualified_identifier)] @name))) @definition.function.function
(declaration declarator: (function_declarator declarator: [(identifier) (field_identifier) (qualified_identifier) (operator_name) (destructor_name)] @name)) @definition.function.prototype
(field_declaration declarator: (function_declarator declarator: [(field_identifier) (operator_name) (destructor_name)] @name)) @definition.function.prototype
(class_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.type.class
(struct_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.type.struct
(union_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.type.union
(enum_specifier name: (type_identifier) @name body: (enumerator_list)) @definition.type.enum
(type_definition declarator: (type_identifier) @name) @definition.type.typedef
(alias_declaration name: (type_identifier) @name) @definition.type.using
(concept_definition name: (identifier) @name) @definition.type.concept
(namespace_definition name: [(namespace_identifier) (nested_namespace_specifier)] @name) @definition.module.namespace
(preproc_def name: (identifier) @name) @definition.constant.define
(preproc_function_def name: (identifier) @name) @definition.macro.define
((translation_unit
  (declaration
    (type_qualifier) @qualifier
    declarator: (init_declarator declarator: (identifier) @name)) @definition.constant.const)
  (#any-of? @qualifier "const" "constexpr"))
((namespace_definition
  body: (declaration_list
    (declaration
      (type_qualifier) @qualifier
      declarator: (init_declarator declarator: (identifier) @name)) @definition.constant.const))
  (#any-of? @qualifier "const" "constexpr"))

; The names a symbol calls.
(call_expression function: (identifier) @call)
(call_expression function: (field_expression field: (field_identifier) @call))
(call_expression function: (qualified_identifier name: (identifier) @call))
(call_expression function: (template_function name: (identifier) @call))
