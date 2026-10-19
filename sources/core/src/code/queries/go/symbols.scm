; Definitions: @definition.<of>.<word> on the node, @name on its name. A method names its type by
; its receiver (@owner.receiver): it is listed in that type.
(type_spec name: (type_identifier) @name type: (struct_type)) @definition.type.struct
(type_spec name: (type_identifier) @name type: (interface_type)) @definition.type.interface
(type_spec
  name: (type_identifier) @name
  type: [
    (type_identifier) (qualified_type) (pointer_type) (slice_type) (array_type) (map_type)
    (channel_type) (function_type) (generic_type) (negated_type)
  ]) @definition.type.type
(type_alias name: (type_identifier) @name) @definition.type.type
(method_elem name: (field_identifier) @name) @definition.function.func
(function_declaration name: (identifier) @name) @definition.function.func
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: [
        (type_identifier) @owner.receiver
        (pointer_type (type_identifier) @owner.receiver)
        (generic_type type: (type_identifier) @owner.receiver)
        (pointer_type (generic_type type: (type_identifier) @owner.receiver))
      ]))
  name: (field_identifier) @name) @definition.method.func
(const_spec name: (identifier) @name) @definition.constant.const

; The names a symbol calls.
(call_expression function: (identifier) @call)
(call_expression function: (selector_expression field: (field_identifier) @call))
