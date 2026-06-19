module.exports = grammar({
  name: 'paideia',

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.module_decl,
      $.macro_decl,
      $.comment,
    ),

    // =========================================================================
    // Module declarations
    // =========================================================================

    module_decl: $ => seq(
      'module',
      field('name', $.identifier),
      '=',
      $._module_body,
    ),

    _module_body: $ => choice(
      $.structure_expr,
      $.functor_expr,
    ),

    structure_expr: $ => seq(
      'structure',
      '{',
      repeat($._structure_item),
      '}',
    ),

    _structure_item: $ => choice(
      $.function_decl,
      $.let_decl,
      $.type_decl,
      $.effect_decl,
      $.macro_decl,
      $.comment,
    ),

    functor_expr: $ => seq(
      'functor',
      '(',
      field('param', $.identifier),
      ':',
      field('param_type', $.identifier),
      ')',
      '=',
      $._module_body,
    ),

    // =========================================================================
    // Function declarations
    // =========================================================================

    function_decl: $ => seq(
      'let',
      field('name', $.identifier),
      optional(seq(':', $._function_type)),
      '=',
      'fn',
      '(',
      optional(commaSep($.parameter)),
      ')',
      optional($.effect_row),
      optional($.capability_row),
      '->',
      $._type,
      $._function_body,
    ),

    _function_type: $ => seq(
      '(',
      optional(commaSep($._type)),
      ')',
      '->',
      $._type,
      optional($.effect_row),
      optional($.capability_row),
    ),

    _function_body: $ => choice(
      $.block_expr,
      $.fn_body_expr,
    ),

    fn_body_expr: $ => choice(
      $.identifier,
      $.number,
      $.string,
      $.boolean,
      $.unit,
      $.call_expr,
      $.binary_op_expr,
      $.lambda_expr,
      $.with_handler_expr,
      $.parenthesized_expr,
    ),

    parameter: $ => seq(
      $.identifier,
      optional(seq(':', $._type)),
    ),

    // =========================================================================
    // Let bindings
    // =========================================================================

    let_decl: $ => seq(
      'let',
      field('name', $.identifier),
      optional(seq(':', $._type)),
      '=',
      $._expression,
    ),

    // =========================================================================
    // Type declarations
    // =========================================================================

    type_decl: $ => seq(
      'type',
      field('name', $.identifier),
      '=',
      $._type,
    ),

    // =========================================================================
    // Effect declarations
    // =========================================================================

    effect_decl: $ => seq(
      'effect',
      field('name', $.identifier),
      '{',
      repeat($.effect_operation),
      '}',
    ),

    effect_operation: $ => seq(
      'op',
      field('name', $.identifier),
      ':',
      '(',
      optional(commaSep($._type)),
      ')',
      '->',
      $._type,
    ),

    // =========================================================================
    // Macro declarations
    // =========================================================================

    macro_decl: $ => choice(
      $.single_rule_macro,
      $.multi_rule_macro,
    ),

    single_rule_macro: $ => seq(
      'macro',
      field('name', $.identifier),
      '(',
      optional(commaSep($.macro_pattern)),
      ')',
      '=>',
      $.macro_template,
    ),

    multi_rule_macro: $ => seq(
      'macro',
      field('name', $.identifier),
      '{',
      repeat(seq(
        '(',
        optional(commaSep($.macro_pattern)),
        ')',
        '=>',
        $.macro_template,
        ';',
      )),
      '}',
    ),

    macro_pattern: $ => choice(
      seq('$', $.identifier, ':', choice('expr', 'ident', 'type', 'literal', 'block', 'tt')),
      $.identifier,
    ),

    macro_template: $ => repeat1(choice(
      $.identifier,
      '.',
      ',',
      '(',
      ')',
      '{',
      '}',
      '[',
      ']',
      '=',
      '->',
      '|',
      ':',
      ';',
      $.number,
      $.string,
      seq('$', $.identifier),
    )),

    // =========================================================================
    // Effect row and capability set annotations
    // =========================================================================

    effect_row: $ => seq(
      '!',
      '{',
      optional(commaSep($.identifier)),
      '}',
    ),

    capability_row: $ => seq(
      '@',
      '{',
      optional(commaSep($.identifier)),
      '}',
    ),

    // =========================================================================
    // Types
    // =========================================================================

    _type: $ => choice(
      $.identifier,
      $.linear_type,
      $.affine_type,
      $.function_type_expr,
      $.tuple_type,
      $.parenthesized_type,
    ),

    linear_type: $ => seq('linear', ':', $.identifier),
    affine_type: $ => seq('affine', ':', $.identifier),

    function_type_expr: $ => seq(
      '(',
      optional(commaSep($._type)),
      ')',
      '->',
      $._type,
      optional($.effect_row),
      optional($.capability_row),
    ),

    tuple_type: $ => seq(
      '(',
      commaSep($._type),
      ')',
    ),

    parenthesized_type: $ => seq('(', $._type, ')'),

    // =========================================================================
    // Expressions
    // =========================================================================

    _expression: $ => choice(
      $.identifier,
      $.number,
      $.string,
      $.boolean,
      $.unit,
      $.call_expr,
      $.binary_op_expr,
      $.unary_op_expr,
      $.lambda_expr,
      $.match_expr,
      $.if_expr,
      $.block_expr,
      $.with_handler_expr,
      $.perform_expr,
      $.unsafe_block_expr,
      $.parenthesized_expr,
    ),

    call_expr: $ => seq(
      $.identifier,
      '(',
      optional(commaSep($._expression)),
      ')',
    ),

    binary_op_expr: $ => {
      const operators = [
        [prec.left(6), choice('+', '-')],
        [prec.left(7), choice('*', '/')],
        [prec.left(5), '='],
      ];

      return choice(...operators.map(([precedence, op]) =>
        precedence(seq($._expression, op, $._expression))
      ));
    },

    unary_op_expr: $ => seq(
      choice('-', '!'),
      $._expression,
    ),

    lambda_expr: $ => seq(
      'fn',
      '(',
      optional(commaSep($.parameter)),
      ')',
      optional($.effect_row),
      optional($.capability_row),
      '->',
      $._type,
      $._expression,
    ),

    match_expr: $ => seq(
      'match',
      $._expression,
      '{',
      repeat(seq(
        $.pattern,
        '=>',
        $._expression,
        ';',
      )),
      '}',
    ),

    pattern: $ => choice(
      $.identifier,
      '|',
      $.number,
      $.string,
      seq('(', optional(commaSep($.pattern)), ')'),
    ),

    if_expr: $ => seq(
      'if',
      $._expression,
      $.block_expr,
      optional(seq('else', choice($.block_expr, $.if_expr))),
    ),

    block_expr: $ => seq(
      '{',
      repeat($._statement),
      optional($._expression),
      '}',
    ),

    _statement: $ => choice(
      $.let_decl,
      $.return_stmt,
      seq($._expression, ';'),
    ),

    return_stmt: $ => seq('return', optional($._expression), ';'),

    with_handler_expr: $ => seq(
      'with',
      $.identifier,
      'handle',
      $.identifier,
      $.block_expr,
    ),

    perform_expr: $ => seq(
      'perform',
      $.call_expr,
    ),

    unsafe_block_expr: $ => seq(
      'unsafe',
      '{',
      repeat(seq(
        choice('effects', 'capabilities', 'justification', 'block'),
        ':',
        choice($.identifier, $.string, $.block_expr),
        ',',
      )),
      '}',
    ),

    parenthesized_expr: $ => seq('(', $._expression, ')'),

    // =========================================================================
    // Literals and primitives
    // =========================================================================

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,
    number: $ => /(?:0x[0-9a-fA-F_]+|[0-9]+(?:_[0-9]+)*)/,
    string: $ => /"[^"]*"/,
    boolean: $ => choice('true', 'false'),
    unit: $ => '()',

    comment: $ => token(prec(-1, /\/\/[^\n]*/)),
  },

  extras: $ => [/\s/, $.comment],
});

function commaSep(rule) {
  return optional(seq(rule, repeat(seq(',', rule))));
}
