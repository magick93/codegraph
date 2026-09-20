/**
 * Tree-sitter grammar for the rexlang `.mox` modeling language.
 *
 * Source of truth: rexlang `docs/LANGUAGE.md` and
 * `crates/rex-syntax/src/{parser,lexer}.rs`. Keywords are reserved;
 * everything else (`id`, `readonly`, `to`, `format`, `create`, `convert`,
 * constraint keywords, primitive type names such as `String`/`int`) is
 * contextual: they lex as `identifier` except in the grammar position that
 * expects them (tree-sitter keyword extraction via `word`).
 *
 * ── Node inventory (stable names; the LSP writes queries against these) ──
 *
 *   source_file
 *   ├── package_declaration            `package` qualified_name
 *   ├── import_declaration             `import` path: string              (.actor surface)
 *   ├── import_schema_declaration      `import` `schema` path: string (`as` alias)?
 *   ├── annotation_declaration         `annotation` value: string (`as` name)?
 *   ├── class_declaration              `class` name (extends_clause)? `{` feature* `}`
 *   │   └── feature                    modifiers then exactly one kind:
 *   │       ├── modifier               `id` | `readonly`
 *   │       ├── attribute              type multiplicity? name (`=` default)? constraint_block?
 *   │       ├── containment            `contains` type multiplicity? name opposite_clause?
 *   │       ├── reference              `refers` type multiplicity? name opposite_clause?
 *   │       ├── container_feature      `container` type name opposite_clause?
 *   │       ├── operation_declaration  `op` type name parameters op_body?
 *   │       └── derived_declaration    `derived` type multiplicity? name op_body?
 *   ├── interface_declaration          `interface` name `{` binding_entry* `}`
 *   ├── enum_declaration               `enum` name `{` enum_literal+ `}`
 *   ├── datatype_declaration           `type` name `wraps` (`opaque` | type)? datatype_block?
 *   │   └── datatype_block             binding_entry | format_entry | create_block | convert_block
 *   │       └── target_body            target name + raw_body (create/convert/op/derived)
 *   ├── vocabulary_declaration         `vocabulary` name `from` source `{` entries `}`
 *   │   ├── version_entry              `version` string
 *   │   ├── key_entry                  `key` name
 *   │   └── facet_entry                `facet` type name
 *   └── actors_block                   `actors` name `{` items `}` (inline or `.actor` file)
 *       ├── actor_declaration          (`actor` | `agent`) name (`extends` superclass)?
 *       ├── capability                 `capability` name `on` type
 *       ├── purpose_declaration        `purpose` name
 *       ├── grant_declaration          `grant` actor `{` grant_entry* `}`
 *       │   └── grant_entry            effect_entry | cedar_entry
 *       │       ├── effect_entry       (`permit` | `forbid`) capability when_clause? obligation*
 *       │       ├── when_clause        `when` raw_parens (condition text is opaque)
 *       │       ├── obligation         `obligation` name
 *       │       └── cedar_entry        `cedar` raw_body (policy text is opaque)
 *       ├── delegation_declaration     `delegation` name `{` from/to/purpose/entries `}`
 *       │   ├── from_clause / to_clause / purpose_clause
 *       │   └── grant_entry            (cedar_entry is rejected by rexlang, tolerated here)
 *       └── never_both_declaration     `never_both` `{` name (`,` name)* `}`
 *
 *   Shared leaves: qualified_name (dotted), identifier, escaped_identifier
 *   (`^keyword`), string, number, boolean, multiplicity, raw_body, raw_parens,
 *   line_comment / line_doc_comment / block_comment / block_doc_comment.
 *
 * Type positions are the `type` fields on extends_clause, attribute,
 * containment, reference, container_feature, operation_declaration,
 * derived_declaration, capability, facet_entry and the datatype `wraps`
 * target; the `superclass` field marks actor extends targets.
 */
module.exports = grammar({
  name: 'mox',

  word: $ => $.identifier,

  extras: $ => [
    /\s+/,
    $.line_comment,
    $.line_doc_comment,
    $.block_comment,
    $.block_doc_comment,
  ],

  conflicts: $ => [],

  rules: {
    // ── Top level ─────────────────────────────────────────────
    source_file: $ => repeat($._top_level_item),

    _top_level_item: $ => choice(
      $.package_declaration,
      $.import_declaration,
      $.import_schema_declaration,
      $.annotation_declaration,
      $.class_declaration,
      $.interface_declaration,
      $.enum_declaration,
      $.datatype_declaration,
      $.vocabulary_declaration,
      $.actors_block,
    ),

    package_declaration: $ => seq(
      'package', field('name', $.qualified_name),
    ),

    // `schema` is a contextual keyword: an identifier in every other
    // position, a keyword only directly after `import`.
    import_schema_declaration: $ => seq(
      'import', 'schema', field('path', $.string),
      optional(seq('as', field('alias', $._name))),
    ),

    import_declaration: $ => seq(
      'import', field('path', $.string),
    ),

    annotation_declaration: $ => seq(
      'annotation', field('value', $.string),
      optional(seq('as', field('name', $._name))),
    ),

    // ── Classes and features ──────────────────────────────────
    class_declaration: $ => seq(
      'class', field('name', $._name),
      optional($.extends_clause),
      '{', repeat($.feature), '}',
    ),

    extends_clause: $ => seq(
      'extends',
      field('type', $.qualified_name),
      repeat(seq(',', field('type', $.qualified_name))),
    ),

    feature: $ => seq(
      repeat($.modifier),
      $._feature_kind,
    ),

    modifier: $ => choice('id', 'readonly'),

    _feature_kind: $ => choice(
      $.containment,
      $.reference,
      $.container_feature,
      $.operation_declaration,
      $.derived_declaration,
      $.attribute,
    ),

    attribute: $ => seq(
      field('type', $.qualified_name),
      optional(field('multiplicity', $.multiplicity)),
      field('name', $._name),
      optional(seq('=', field('default', $._default_value))),
      optional($.constraint_block),
    ),

    containment: $ => seq(
      'contains',
      field('type', $.qualified_name),
      optional(field('multiplicity', $.multiplicity)),
      field('name', $._name),
      optional($.opposite_clause),
    ),

    reference: $ => seq(
      'refers',
      field('type', $.qualified_name),
      optional(field('multiplicity', $.multiplicity)),
      field('name', $._name),
      optional($.opposite_clause),
    ),

    container_feature: $ => seq(
      'container',
      field('type', $.qualified_name),
      field('name', $._name),
      optional($.opposite_clause),
    ),

    opposite_clause: $ => seq(
      'opposite', field('name', $._name),
    ),

    operation_declaration: $ => seq(
      'op',
      field('type', $.qualified_name),
      field('name', $._name),
      field('parameters', $.parameters),
      optional(field('body', $.op_body)),
    ),

    derived_declaration: $ => seq(
      'derived',
      field('type', $.qualified_name),
      optional(field('multiplicity', $.multiplicity)),
      field('name', $._name),
      optional(field('body', $.op_body)),
    ),

    parameters: $ => seq(
      '(',
      optional(seq($.parameter, repeat(seq(',', $.parameter)))),
      ')',
    ),

    parameter: $ => seq(
      field('type', $.qualified_name),
      field('name', $._name),
    ),

    // ── Verbatim bodies ────────────────────────────────────────
    // An operation/derived body holds one or more per-target blocks:
    // `{ rust { ... } expr { ... } }`. The neutral expression language
    // lives in a single `expr { ... }` block. A bare `{ ... }` body is
    // rejected by rexlang itself (Tier 1 / Tier 2), so it is a parse
    // error here too, which keeps the grammar deterministic.
    op_body: $ => seq(
      '{', repeat($.target_body), '}',
    ),

    target_body: $ => seq(
      field('target', $._name),
      field('body', $.raw_body),
    ),

    // Opaque balanced-brace content. The content reuses the lexical tokens
    // (strings stay intact, so braces inside a string cannot unbalance the
    // body); only punctuation runs are matched by a catch-all.
    raw_body: $ => seq(
      '{', repeat($._raw_atom), '}',
    ),

    _raw_atom: $ => choice(
      $.raw_body,
      $.string,
      $.number,
      $.identifier,
      $._raw_symbol,
    ),

    _raw_symbol: $ => prec(-1, /[^{}"\s]+/),

    // Opaque balanced-paren content (the text of a `when (...)` guard).
    raw_parens: $ => seq(
      '(', repeat($._raw_paren_atom), ')',
    ),

    _raw_paren_atom: $ => choice(
      $.raw_parens,
      $.string,
      $.number,
      $.identifier,
      $._raw_paren_symbol,
    ),

    _raw_paren_symbol: $ => prec(-1, /[^()"\s]+/),

    // ── Multiplicity: `[]` `[n]` `[n..m]` `[n..*]` ────────────
    multiplicity: $ => seq(
      '[',
      optional(seq(
        field('lower', $.number),
        optional(seq('..', field('upper', choice($.number, '*')))),
      )),
      ']',
    ),

    // ── Constraints ───────────────────────────────────────────
    constraint_block: $ => seq(
      '{', repeat($.constraint_entry), '}',
    ),

    constraint_entry: $ => choice(
      seq(
        $._valued_constraint_keyword,
        field('value', choice($.string, $.number)),
      ),
      seq('unique'),
    ),

    _valued_constraint_keyword: $ => choice(
      'pattern', 'minLength', 'maxLength', 'minimum', 'maximum',
    ),

    _default_value: $ => choice(
      $.string,
      $.number,
      $.boolean,
      $._name,
    ),

    boolean: $ => choice('true', 'false'),

    // ── Interfaces ────────────────────────────────────────────
    interface_declaration: $ => seq(
      'interface', field('name', $._name),
      '{', repeat($.binding_entry), '}',
    ),

    binding_entry: $ => seq(
      field('key', $._name),
      field('value', $.string),
    ),

    // ── Enums ─────────────────────────────────────────────────
    enum_declaration: $ => seq(
      'enum', field('name', $._name),
      '{', repeat1($.enum_literal), '}',
    ),

    enum_literal: $ => seq(
      field('name', $._name),
      optional(seq('as', field('label', $.string))),
      optional(seq('=', field('value', $.number))),
    ),

    // ── Datatypes ─────────────────────────────────────────────
    datatype_declaration: $ => seq(
      'type', field('name', $._name), 'wraps',
      optional(choice(
        'opaque',
        field('type', $.qualified_name),
      )),
      optional($.datatype_block),
    ),

    datatype_block: $ => seq(
      '{', repeat($._datatype_entry), '}',
    ),

    _datatype_entry: $ => choice(
      $.format_entry,
      $.binding_entry,
      $.create_block,
      $.convert_block,
    ),

    // `format` is reserved: the unescaped key declares the format hint.
    format_entry: $ => seq(
      'format', field('value', $.string),
    ),

    create_block: $ => seq(
      'create', '{', repeat($.target_body), '}',
    ),

    convert_block: $ => seq(
      'convert', '{', repeat($.target_body), '}',
    ),

    // ── Vocabularies ──────────────────────────────────────────
    vocabulary_declaration: $ => seq(
      'vocabulary', field('name', $._name), 'from',
      field('source', $.string),
      '{', repeat($._vocabulary_entry), '}',
    ),

    _vocabulary_entry: $ => choice(
      $.version_entry,
      $.key_entry,
      $.facet_entry,
    ),

    version_entry: $ => seq(
      'version', field('value', $.string),
    ),

    key_entry: $ => seq(
      'key', field('name', $._name),
    ),

    facet_entry: $ => seq(
      'facet',
      field('type', $.qualified_name),
      field('name', $._name),
    ),

    // ── Actors (inline `.mox` block and standalone `.actor` file) ──
    actors_block: $ => seq(
      'actors', field('name', $._name),
      '{', repeat($._actors_item), '}',
    ),

    _actors_item: $ => choice(
      $.actor_declaration,
      $.capability,
      $.purpose_declaration,
      $.grant_declaration,
      $.delegation_declaration,
      $.never_both_declaration,
    ),

    actor_declaration: $ => seq(
      choice('actor', 'agent'),
      field('name', $._name),
      optional(seq('extends', field('superclass', $._name))),
    ),

    capability: $ => seq(
      'capability', field('name', $._name), 'on',
      field('type', $.qualified_name),
    ),

    purpose_declaration: $ => seq(
      'purpose', field('name', $._name),
    ),

    grant_declaration: $ => seq(
      'grant', field('actor', $._name),
      '{', repeat($.grant_entry), '}',
    ),

    grant_entry: $ => choice(
      $.effect_entry,
      $.cedar_entry,
    ),

    effect_entry: $ => seq(
      field('effect', choice('permit', 'forbid')),
      field('capability', $._name),
      optional($.when_clause),
      repeat($.obligation),
    ),

    when_clause: $ => seq(
      'when', field('condition', $.raw_parens),
    ),

    obligation: $ => seq(
      'obligation', field('name', $._name),
    ),

    cedar_entry: $ => seq(
      'cedar', field('body', $.raw_body),
    ),

    delegation_declaration: $ => seq(
      'delegation', field('name', $._name),
      '{', repeat($._delegation_item), '}',
    ),

    _delegation_item: $ => choice(
      $.from_clause,
      $.to_clause,
      $.purpose_clause,
      $.grant_entry,
    ),

    from_clause: $ => seq(
      'from', field('name', $._name),
    ),

    to_clause: $ => seq(
      'to', field('name', $._name),
    ),

    purpose_clause: $ => seq(
      'purpose', field('name', $._name),
    ),

    never_both_declaration: $ => seq(
      'never_both', '{',
      field('capability', $._name),
      repeat(seq(',', field('capability', $._name))),
      '}',
    ),

    // ── Names ─────────────────────────────────────────────────
    qualified_name: $ => seq(
      $._name, repeat(seq('.', $._name)),
    ),

    _name: $ => choice(
      $.identifier,
      $.escaped_identifier,
    ),

    // ── Lexical rules ─────────────────────────────────────────
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    escaped_identifier: $ => token(seq('^', /[a-zA-Z_][a-zA-Z0-9_]*/)),

    string: $ => token(seq(
      '"',
      repeat(choice(
        token.immediate(seq('\\', /./)),
        token.immediate(/[^"\\\n]/),
      )),
      '"',
    )),

    number: $ => /-?[0-9]+/,

    line_doc_comment: $ => token(prec(1, seq('///', /[^\n]*/))),
    line_comment: $ => token(seq('//', /[^\n]*/)),

    block_doc_comment: $ => token(prec(1, seq(
      '/**', /[^*]*\*+([^/*][^*]*\*+)*/, '/',
    ))),
    block_comment: $ => token(seq(
      '/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/',
    )),
  },
});
