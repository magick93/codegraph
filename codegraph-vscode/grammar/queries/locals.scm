; Parameter declarations define local variables
(parameter_decl
  name: (identifier) @local.definition)

; References to parameters inside expressions
(parameter_decl
  (identifier) @local.reference)

; Event parameters define local variables in scope
(event_param
  (identifier) @local.definition)

; Event parameter references
(event_param
  (identifier) @local.reference)

; Binding pair values reference locals
(binding_pair
  value: (expression
    (identifier) @local.reference))

; Field expressions reference locals on the LHS
(field_expr
  (identifier) @local.reference)
