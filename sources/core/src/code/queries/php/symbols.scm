; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class_declaration name: (name) @name) @definition.type.class
(interface_declaration name: (name) @name) @definition.type.interface
(trait_declaration name: (name) @name) @definition.type.trait
(enum_declaration name: (name) @name) @definition.type.enum
(namespace_definition name: (namespace_name) @name body: (compound_statement)) @definition.module.namespace
(function_definition name: (name) @name) @definition.function.function
(method_declaration name: (name) @name) @definition.function.function
(const_declaration (const_element (name) @name)) @definition.constant.const

; The names a symbol calls.
(function_call_expression function: (name) @call)
(function_call_expression function: (qualified_name (name) @call .))
(member_call_expression name: (name) @call)
(nullsafe_member_call_expression name: (name) @call)
(scoped_call_expression name: (name) @call)
(object_creation_expression (name) @call)
