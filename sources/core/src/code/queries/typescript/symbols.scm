; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class_declaration name: (type_identifier) @name) @definition.type.class
(abstract_class_declaration name: (type_identifier) @name) @definition.type.class
(interface_declaration name: (type_identifier) @name) @definition.type.interface
(enum_declaration name: (identifier) @name) @definition.type.enum
(type_alias_declaration name: (type_identifier) @name) @definition.type.type
(internal_module name: [(identifier) (nested_identifier)] @name) @definition.module.namespace
(module name: [(identifier) (nested_identifier)] @name) @definition.module.module
(function_declaration name: (identifier) @name) @definition.function.function
(generator_function_declaration name: (identifier) @name) @definition.function.function
(function_signature name: (identifier) @name) @definition.function.function
((method_definition name: (property_identifier) @name) @definition.function.constructor
  (#eq? @name "constructor"))
((method_definition name: [(property_identifier) (private_property_identifier)] @name) @definition.function.method
  (#not-eq? @name "constructor"))
(method_signature name: (property_identifier) @name) @definition.function.method
(abstract_method_signature name: (property_identifier) @name) @definition.function.method
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression) (generator_function)])) @definition.function.function
(variable_declaration
  (variable_declarator
    name: (identifier) @name
    value: [(arrow_function) (function_expression) (generator_function)])) @definition.function.function
(program
  (lexical_declaration
    kind: "const"
    (variable_declarator
      name: (identifier) @name
      value: [(number) (string) (template_string) (true) (false) (array) (object) (regex) (as_expression) (satisfies_expression)])) @definition.constant.const)
(program
  (export_statement
    declaration: (lexical_declaration
      kind: "const"
      (variable_declarator
        name: (identifier) @name
        value: [(number) (string) (template_string) (true) (false) (array) (object) (regex) (as_expression) (satisfies_expression)])) @definition.constant.const))

; A class's field whose value is a function: a method of it.
(public_field_definition name: [(property_identifier) (private_property_identifier)] @name value: [(arrow_function) (function_expression) (generator_function)])
  @definition.function.method

; An object's member whose value is a function, in an object that is a top-level const's value or
; that a function returns: a method of the const, or of the function (`@members`).
(program
  (lexical_declaration
    kind: "const"
    (variable_declarator
      value: [
        (object
          (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
            @definition.function.method)
        (as_expression (object
            (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
              @definition.function.method))
        (satisfies_expression (object
            (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
              @definition.function.method))
      ])))
(program
  (export_statement
    declaration: (lexical_declaration
      kind: "const"
      (variable_declarator
        value: [
          (object
            (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
              @definition.function.method)
          (as_expression (object
              (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
                @definition.function.method))
          (satisfies_expression (object
              (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
                @definition.function.method))
        ]))))
(return_statement (object
  (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
    @definition.function.method))
(arrow_function body: (parenthesized_expression (object
    (pair key: (property_identifier) @name value: [(arrow_function) (function_expression) (generator_function)])
      @definition.function.method)))

; An object a function returns, whose members are the function's (`@members`), and the functions it
; may be inside (`@scope`): kept only when it is the function's own, not a callback's inside it.
(return_statement (object) @members)
(arrow_function body: (parenthesized_expression (object) @members))
[
  (function_declaration)
  (generator_function_declaration)
  (function_expression)
  (arrow_function)
  (generator_function)
  (method_definition)
] @scope

; A top-level const whose value is a call given a function: a function of the const's name.
(program
  (lexical_declaration
    kind: "const"
    (variable_declarator
      name: (identifier) @name
      value: (call_expression arguments: (arguments [(arrow_function) (function_expression)]))))
  @definition.function.function)
(program
  (export_statement
    declaration: (lexical_declaration
      kind: "const"
      (variable_declarator
        name: (identifier) @name
        value: (call_expression arguments: (arguments [(arrow_function) (function_expression)]))))
    @definition.function.function))

; A test's calls, named by the text given first (a template with nothing put in it too): describe
; and suite hold the others, it and test are run. `.only` and the like as the call itself.
((call_expression
  function: (identifier) @test
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.module.describe
  (#eq? @test "describe"))
((call_expression
  function: (member_expression object: (identifier) @test property: (property_identifier) @variant)
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.module.describe
  (#eq? @test "describe")
  (#match? @variant "^(only|skip|todo|concurrent)$"))
((call_expression
  function: (identifier) @test
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.module.suite
  (#eq? @test "suite"))
((call_expression
  function: (member_expression object: (identifier) @test property: (property_identifier) @variant)
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.module.suite
  (#eq? @test "suite")
  (#match? @variant "^(only|skip|todo|concurrent)$"))
((call_expression
  function: (identifier) @test
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.function.it
  (#eq? @test "it"))
((call_expression
  function: (member_expression object: (identifier) @test property: (property_identifier) @variant)
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.function.it
  (#eq? @test "it")
  (#match? @variant "^(only|skip|todo|concurrent)$"))
((call_expression
  function: (identifier) @test
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.function.test
  (#eq? @test "test"))
((call_expression
  function: (member_expression object: (identifier) @test property: (property_identifier) @variant)
  arguments: (arguments . [(string (string_fragment) @name) (template_string . (string_fragment) @name .)]))
  @definition.function.test
  (#eq? @test "test")
  (#match? @variant "^(only|skip|todo|concurrent)$"))

; The names a symbol calls.
(call_expression function: (identifier) @call)
(call_expression function: (member_expression property: [(property_identifier) (private_property_identifier)] @call))
(new_expression constructor: (identifier) @call)
