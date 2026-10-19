; Definitions: @definition.<of>.<word> on the node, @name on its name. The first pattern that finds a
; node says what it is.
(class_declaration "interface" name: (identifier) @name) @definition.type.interface
(class_declaration (modifiers (class_modifier "enum")) name: (identifier) @name) @definition.type.enum
(class_declaration name: (identifier) @name) @definition.type.class
(object_declaration name: (identifier) @name) @definition.type.object
(companion_object name: (identifier) @name) @definition.type.object
(type_alias type: (identifier) @name) @definition.type.typealias
(function_declaration name: (identifier) @name) @definition.function.fun
(secondary_constructor "constructor" @name) @definition.function.constructor
(source_file
  (property_declaration
    (modifiers (property_modifier "const"))
    (variable_declaration (identifier) @name)) @definition.constant.const)
(object_declaration
  (class_body
    (property_declaration
      (modifiers (property_modifier "const"))
      (variable_declaration (identifier) @name)) @definition.constant.const))
(companion_object
  (class_body
    (property_declaration
      (modifiers (property_modifier "const"))
      (variable_declaration (identifier) @name)) @definition.constant.const))

; The names a symbol calls.
(call_expression . (identifier) @call)
(call_expression . (navigation_expression (identifier) @call .))
