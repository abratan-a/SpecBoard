; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class_declaration declaration_kind: "class" name: (type_identifier) @name) @definition.type.class
(class_declaration declaration_kind: "struct" name: (type_identifier) @name) @definition.type.struct
(class_declaration declaration_kind: "enum" name: (type_identifier) @name) @definition.type.enum
(class_declaration declaration_kind: "actor" name: (type_identifier) @name) @definition.type.actor
(protocol_declaration name: (type_identifier) @name) @definition.type.protocol
(typealias_declaration name: (type_identifier) @name) @definition.type.typealias
(function_declaration name: (simple_identifier) @name) @definition.function.func
(function_declaration
  name: [
    "!=" "!==" "%" "%=" "&" "*" "*=" "+" "++" "+=" "-" "--" "-=" "/" "/=" "<" "<<" "<=" "=" "=="
    "===" ">" ">=" ">>" "^" (bang) (custom_operator)
  ] @name) @definition.function.func
(protocol_function_declaration name: (simple_identifier) @name) @definition.function.func
(init_declaration name: "init" @name) @definition.function.init
(macro_declaration (simple_identifier) @name) @definition.macro.macro
(source_file
  (property_declaration
    (value_binding_pattern "let")
    name: (pattern bound_identifier: (simple_identifier) @name)) @definition.constant.let)

; Blocks whose members belong to a type named elsewhere in the file.
(class_declaration
  declaration_kind: "extension"
  name: [
    (user_type (type_identifier) @owner)
    (user_type (type_identifier) (type_identifier) @owner)
  ]) @impl.extension

; The names a symbol calls.
(call_expression . (simple_identifier) @call)
(call_expression . (navigation_expression suffix: (navigation_suffix suffix: (simple_identifier) @call)))
