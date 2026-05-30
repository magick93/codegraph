; Keywords (grammar-level tokens)
"domain" @keyword
"schema" @keyword
"view" @keyword
"container" @keyword
"component" @keyword
"action" @keyword
"module" @keyword
"input" @keyword
"output" @keyword
"params" @keyword
"label" @keyword
"on" @keyword

; Event type keywords
"select" @keyword.function
"submit" @keyword.function
"click" @keyword.function
"change" @keyword.function
"load" @keyword.function
"save" @keyword.function
"cancel" @keyword.function
"delete" @keyword.function
"confirm" @keyword.function
"back" @keyword.function

; Flow control / action keywords
"navigate" @keyword.control
"refresh" @keyword.control
(stay_statement) @keyword.control

; Types
(type_ref) @type

; Strings
(string) @string

; Numbers
(number) @number

; Booleans
(boolean) @boolean

; Comments
(comment) @comment

; Identifiers
(identifier) @variable

; Property keys
(property_assignment
  key: (identifier) @property)

; Common IFML property names
(property_assignment
  key: (identifier)
  (#eq? @property "type"))
(property_assignment
  key: (identifier)
  (#eq? @property "data"))
(property_assignment
  key: (identifier)
  (#eq? @property "fields"))
(property_assignment
  key: (identifier)
  (#eq? @property "mode"))
(property_assignment
  key: (identifier)
  (#eq? @property "filter"))
(property_assignment
  key: (identifier)
  (#eq? @property "sort"))
(property_assignment
  key: (identifier)
  (#eq? @property "landmark"))
(property_assignment
  key: (identifier)
  (#eq? @property "modal"))
(property_assignment
  key: (identifier)
  (#eq? @property "xor"))
(property_assignment
  key: (identifier)
  (#eq? @property "default"))

; Binding keys
(binding_pair
  key: (identifier) @property)

; Parameter names
(parameter_decl
  name: (identifier) @parameter)

; Event parameter names
(event_param
  (identifier) @parameter)

; Function call names
(call_expr
  (identifier) @function)

; Event action function names
(navigate_action
  "navigate" @function.builtin)
(refresh_action
  "refresh" @function.builtin)
(action_invocation
  "action" @function.builtin)

; Field access dot operator
(field_expr
  "." @operator)

; Operators
"==" @operator
"!=" @operator
"<" @operator
"<=" @operator
">" @operator
">=" @operator
"~=" @operator
"!~" @operator
"+" @operator
"-" @operator
"*" @operator
"/" @operator
"%" @operator
"!" @operator
"&&" @operator
"||" @operator

; Delimiters
";" @punctuation.delimiter
":" @punctuation.delimiter
"." @punctuation.delimiter
"," @punctuation.delimiter
"(" @punctuation.delimiter
")" @punctuation.delimiter
"{" @punctuation.delimiter
"}" @punctuation.delimiter
"[" @punctuation.delimiter
"]" @punctuation.delimiter
"->" @punctuation.delimiter
