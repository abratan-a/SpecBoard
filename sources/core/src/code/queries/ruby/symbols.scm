; Definitions: @definition.<of>.<word> on the node, @name on its name.
(class name: [(constant) (scope_resolution)] @name) @definition.type.class
(module name: [(constant) (scope_resolution)] @name) @definition.module.module
(method name: _ @name) @definition.function.def
(singleton_method name: _ @name) @definition.function.def
(assignment left: (constant) @name) @definition.constant.constant

; The names a symbol calls.
(call method: (identifier) @call)
