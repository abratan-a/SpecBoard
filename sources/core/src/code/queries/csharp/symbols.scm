; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class_declaration name: (identifier) @name) @definition.type.class
(struct_declaration name: (identifier) @name) @definition.type.struct
(interface_declaration name: (identifier) @name) @definition.type.interface
(enum_declaration name: (identifier) @name) @definition.type.enum
(record_declaration name: (identifier) @name) @definition.type.record
(delegate_declaration name: (identifier) @name) @definition.type.delegate
(namespace_declaration name: [(identifier) (qualified_name)] @name) @definition.module.namespace
(file_scoped_namespace_declaration name: [(identifier) (qualified_name)] @name) @definition.module.namespace
(method_declaration name: (identifier) @name) @definition.function.method
(constructor_declaration name: (identifier) @name) @definition.function.constructor
(destructor_declaration name: (identifier) @name) @definition.function.destructor
(operator_declaration operator: _ @name) @definition.function.operator
((field_declaration
  (modifier) @modifier
  (variable_declaration (variable_declarator name: (identifier) @name))) @definition.constant.const
  (#eq? @modifier "const"))

; The names a symbol calls.
(invocation_expression function: (identifier) @call)
(invocation_expression function: (member_access_expression name: (identifier) @call))
(invocation_expression function: (generic_name (identifier) @call))
(invocation_expression function: (member_access_expression name: (generic_name (identifier) @call)))
(object_creation_expression type: (identifier) @call)
