; Definitions: @definition.<of>.<word> on the node, @name on its name. A function directly in a type
; or an impl is said as a method by the extraction, not here.
(struct_item name: (type_identifier) @name) @definition.type.struct
(enum_item name: (type_identifier) @name) @definition.type.enum
(union_item name: (type_identifier) @name) @definition.type.union
(type_item name: (type_identifier) @name) @definition.type.type
(trait_item name: (type_identifier) @name) @definition.type.trait
(mod_item name: (identifier) @name) @definition.module.mod
(macro_definition name: (identifier) @name) @definition.macro.macro_rules
(const_item name: (identifier) @name) @definition.constant.const
(static_item name: (identifier) @name) @definition.constant.static
(function_item name: (identifier) @name) @definition.function.fn
(function_signature_item name: (identifier) @name) @definition.function.fn

; Blocks whose members belong to a type named elsewhere in the file: @impl.<word> on the block,
; @owner on the type's name.
(impl_item
  type: [
    (type_identifier) @owner
    (generic_type type: (type_identifier) @owner)
    (scoped_type_identifier name: (type_identifier) @owner)
    (reference_type type: (type_identifier) @owner)
  ]) @impl.impl

; The names a symbol calls.
(call_expression function: (identifier) @call)
(call_expression function: (field_expression field: (field_identifier) @call))
(call_expression function: (scoped_identifier name: (identifier) @call))
(call_expression function: (generic_function function: (identifier) @call))
(call_expression function: (generic_function function: (field_expression field: (field_identifier) @call)))
(call_expression function: (generic_function function: (scoped_identifier name: (identifier) @call)))
(macro_invocation macro: (identifier) @call)
