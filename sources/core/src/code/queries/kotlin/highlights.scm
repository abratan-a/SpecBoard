; Kotlin's colours: the grammar ships no queries of its own. Earlier patterns win.

(line_comment) @comment
(block_comment) @comment
(shebang) @comment

(string_literal) @string
(multiline_string_literal) @string
(character_literal) @string
(escape_sequence) @escape
(interpolation ["$" "${" "}"] @punctuation.special)

(number_literal) @number
(float_literal) @number

(annotation) @attribute
(file_annotation) @attribute
(label) @label

(package_header (qualified_identifier) @module)
(import (qualified_identifier) @module)

(class_declaration name: (identifier) @type)
(object_declaration name: (identifier) @type)
(companion_object name: (identifier) @type)
(type_alias type: (identifier) @type)
(user_type (identifier) @type)
(enum_entry (identifier) @constant)

(function_declaration name: (identifier) @function)
(call_expression . (identifier) @function)
(call_expression . (navigation_expression (identifier) @function .))

(this_expression) @variable.builtin
(super_expression) @variable.builtin

(navigation_expression (identifier) @property .)

[
  "abstract" "actual" "annotation" "as" "as?" "by" "catch" "class" "companion" "const" "constructor"
  "crossinline" "data" "do" "dynamic" "else" "enum" "expect" "external" "final" "finally" "for" "fun"
  "get" "if" "import" "in" "!in" "infix" "init" "inline" "inner" "interface" "internal" "is" "!is"
  "lateinit" "noinline" "object" "open" "operator" "out" "override" "package" "private" "protected"
  "public" "return" "return@" "sealed" "set" "super" "suspend" "tailrec" "this" "throw" "try"
  "typealias" "val" "value" "var" "vararg" "when" "where" "while"
] @keyword

[
  "!" "!=" "!==" "%" "%=" "&&" "*" "*=" "+" "++" "+=" "-" "--" "-=" "->" ".." "..<" "/" "/=" "<"
  "<=" "=" "==" "===" ">" ">=" "?:" "||" "!!" "::"
] @operator

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ";" ":" "?."] @punctuation.delimiter
