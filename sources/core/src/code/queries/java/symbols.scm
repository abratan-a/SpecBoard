; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class_declaration name: (identifier) @name) @definition.type.class
(interface_declaration name: (identifier) @name) @definition.type.interface
(enum_declaration name: (identifier) @name) @definition.type.enum
(record_declaration name: (identifier) @name) @definition.type.record
(annotation_type_declaration name: (identifier) @name) @definition.type.annotation
(method_declaration name: (identifier) @name) @definition.function.method
(constructor_declaration name: (identifier) @name) @definition.function.constructor
(compact_constructor_declaration name: (identifier) @name) @definition.function.constructor
(field_declaration
  (modifiers "static" "final")
  declarator: (variable_declarator name: (identifier) @name)) @definition.constant.final
(constant_declaration declarator: (variable_declarator name: (identifier) @name)) @definition.constant.final

; The names a symbol calls.
(method_invocation name: (identifier) @call)
(object_creation_expression type: (type_identifier) @call)
