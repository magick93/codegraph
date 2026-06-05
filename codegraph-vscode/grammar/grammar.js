module.exports = grammar({
  name: 'ifml',

  extras: $ => [
    $.comment,
    /\s+/,
  ],

  conflicts: $ => [],

  rules: {
    // ── Top-level ──────────────────────────────────────────────
    source_file: $ => repeat($._definition),

    _definition: $ => choice(
      $.domain_declaration,
      $.view_declaration,
      $.action_declaration,
      $.module_declaration,
    ),

    // ── Domain declaration ──────────────────────────────────────
    domain_declaration: $ => seq(
      'domain', $.string,
      '{', 'schema', $.string, ';', '}',
    ),

    // ── View declaration ────────────────────────────────────────
    view_declaration: $ => seq('view', $.string, $.view_body),

    // ── Container declaration ────────────────────────────────────
    container_declaration: $ => seq('container', $.string, $.view_body),

    // ── Component declaration ────────────────────────────────────
    component_declaration: $ => seq('component', $.string, $.component_body),

    // ── Action declaration ───────────────────────────────────────
    action_declaration: $ => seq('action', $.string, $.action_body),

    // ── Module declaration ───────────────────────────────────────
    module_declaration: $ => seq(
      'module', $.string,
      '{',
      'input', $.parameter_block,
      'output', $.parameter_block,
      repeat(choice(
        $.property_assignment,
        $.container_declaration,
        $.component_declaration,
        $.event_handler,
      )),
      '}',
    ),

    // ── View body (shared by view and container) ─────────────────
    view_body: $ => seq(
      '{',
      optional($.params_block),
      optional($.label_declaration),
      repeat(choice(
        $.property_assignment,
        $.container_declaration,
        $.component_declaration,
        $.event_handler,
      )),
      '}',
    ),

    // ── Component body ──────────────────────────────────────────
    component_body: $ => seq(
      '{',
      repeat(choice(
        $.property_assignment,
        $.event_handler,
      )),
      '}',
    ),

    // ── Action body ─────────────────────────────────────────────
    action_body: $ => seq(
      '{',
      repeat(choice(
        $.property_assignment,
        $.event_handler,
      )),
      '}',
    ),

    // ── Params & labels ────────────────────────────────────────
    params_block: $ => seq('params', $.parameter_block, ';'),

    parameter_block: $ => seq('{', commaSep($.parameter_decl), '}'),

    parameter_decl: $ => seq(
      field('name', $.identifier), ':', field('type', $.type_ref),
    ),

    label_declaration: $ => seq('label', $.string, ';'),

    // ── Property assignment ──────────────────────────────────────
    property_assignment: $ => seq(
      field('key', $.identifier), ':', field('value', $.value_expression), ';',
    ),

    value_expression: $ => choice($.expression, $.array_literal),

    array_literal: $ => seq('[', commaSep($.value_expression), ']'),

    // ── Event handler ────────────────────────────────────────────
    event_handler: $ => seq(
      'on', field('type', $.event_type),
      optional($.event_param),
      '->', field('action', $.event_action),
      ';',
    ),

    event_type: $ => choice(
      'select', 'submit', 'click', 'change', 'load',
      'save', 'cancel', 'delete', 'confirm', 'back',
      $.identifier,
    ),

    event_param: $ => seq('(', commaSep1($.identifier), ')'),

    event_action: $ => choice(
      $.navigate_action,
      $.refresh_action,
      $.action_invocation,
      $.stay_statement,
    ),

    navigate_action: $ => seq(
      'navigate', '(', $.string, optional(seq(',', $.parameter_binding)), ')',
    ),

    refresh_action: $ => seq(
      'refresh', '(', $.string, optional(seq(',', $.parameter_binding)), ')',
    ),

    action_invocation: $ => seq(
      'action', '(', $.string, optional(seq(',', $.action_body)), ')',
    ),

    stay_statement: $ => 'stay',

    parameter_binding: $ => seq('{', commaSep($.binding_pair), '}'),

    binding_pair: $ => seq(
      field('key', $.identifier), ':', field('value', $.expression),
    ),

    // ── Type references ──────────────────────────────────────────
    type_ref: $ => choice(
      'Uuid', 'String', 'Int', 'Float', 'Boolean', 'DateTime',
      $.identifier,
    ),

    // ── C-like expressions ───────────────────────────────────────
    expression: $ => $._logical_or,

    _logical_or: $ => prec.left(0, seq(
      $._logical_and,
      repeat(seq('||', $._logical_and)),
    )),

    _logical_and: $ => prec.left(1, seq(
      $._comparison,
      repeat(seq('&&', $._comparison)),
    )),

    _comparison: $ => prec.left(2, seq(
      $._addition,
      repeat(seq($._comparison_op, $._addition)),
    )),

    _comparison_op: $ => choice('==', '!=', '<', '<=', '>', '>=', '~=', '!~'),

    _addition: $ => prec.left(3, seq(
      $._multiplication,
      repeat(seq($._add_op, $._multiplication)),
    )),

    _add_op: $ => choice('+', '-'),

    _multiplication: $ => prec.left(4, seq(
      $._unary,
      repeat(seq($._mul_op, $._unary)),
    )),

    _mul_op: $ => choice('*', '/', '%'),

    _unary: $ => choice(
      prec(5, seq('!', $._unary)),
      prec(5, seq('-', $._unary)),
      prec(5, $._primary),
    ),

    _primary: $ => choice(
      $.string,
      $.number,
      $.boolean,
      $.call_expr,
      $.field_expr,
      $.identifier,
      $.group_expr,
    ),

    call_expr: $ => seq($.identifier, '(', commaSep($.expression), ')'),

    field_expr: $ => seq($.identifier, '.', $.identifier, repeat(seq('.', $.identifier))),

    group_expr: $ => seq('(', $.expression, ')'),

    // ── Lexical rules ────────────────────────────────────────────
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    string: $ => token(seq(
      '"',
      repeat(choice(
        token.immediate(seq('\\', /./)),
        token.immediate(/[^"\\\n]/),
      )),
      '"',
    )),

    number: $ => token(seq(
      /[0-9]+/,
      optional(seq('.', /[0-9]+/)),
    )),

    boolean: $ => choice('true', 'false'),

    comment: $ => token(seq('//', /[^\n]*/)),
  },
});

function commaSep(rule) {
  return optional(commaSep1(rule));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
