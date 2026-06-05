#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 218
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 122
#define ALIAS_COUNT 0
#define TOKEN_COUNT 65
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 5
#define MAX_ALIAS_SEQUENCE_LENGTH 9
#define PRODUCTION_ID_COUNT 5

enum ts_symbol_identifiers {
  anon_sym_domain = 1,
  anon_sym_LBRACE = 2,
  anon_sym_schema = 3,
  anon_sym_SEMI = 4,
  anon_sym_RBRACE = 5,
  anon_sym_view = 6,
  anon_sym_container = 7,
  anon_sym_component = 8,
  anon_sym_action = 9,
  anon_sym_module = 10,
  anon_sym_input = 11,
  anon_sym_output = 12,
  anon_sym_params = 13,
  anon_sym_COMMA = 14,
  anon_sym_COLON = 15,
  anon_sym_label = 16,
  anon_sym_LBRACK = 17,
  anon_sym_RBRACK = 18,
  anon_sym_on = 19,
  anon_sym_DASH_GT = 20,
  anon_sym_select = 21,
  anon_sym_submit = 22,
  anon_sym_click = 23,
  anon_sym_change = 24,
  anon_sym_load = 25,
  anon_sym_save = 26,
  anon_sym_cancel = 27,
  anon_sym_delete = 28,
  anon_sym_confirm = 29,
  anon_sym_back = 30,
  anon_sym_LPAREN = 31,
  anon_sym_RPAREN = 32,
  anon_sym_navigate = 33,
  anon_sym_refresh = 34,
  sym_stay_statement = 35,
  anon_sym_Uuid = 36,
  anon_sym_String = 37,
  anon_sym_Int = 38,
  anon_sym_Float = 39,
  anon_sym_Boolean = 40,
  anon_sym_DateTime = 41,
  anon_sym_PIPE_PIPE = 42,
  anon_sym_AMP_AMP = 43,
  anon_sym_EQ_EQ = 44,
  anon_sym_BANG_EQ = 45,
  anon_sym_LT = 46,
  anon_sym_LT_EQ = 47,
  anon_sym_GT = 48,
  anon_sym_GT_EQ = 49,
  anon_sym_TILDE_EQ = 50,
  anon_sym_BANG_TILDE = 51,
  anon_sym_PLUS = 52,
  anon_sym_DASH = 53,
  anon_sym_STAR = 54,
  anon_sym_SLASH = 55,
  anon_sym_PERCENT = 56,
  anon_sym_BANG = 57,
  anon_sym_DOT = 58,
  sym_identifier = 59,
  sym_string = 60,
  sym_number = 61,
  anon_sym_true = 62,
  anon_sym_false = 63,
  sym_comment = 64,
  sym_source_file = 65,
  sym__definition = 66,
  sym_domain_declaration = 67,
  sym_view_declaration = 68,
  sym_container_declaration = 69,
  sym_component_declaration = 70,
  sym_action_declaration = 71,
  sym_module_declaration = 72,
  sym_view_body = 73,
  sym_component_body = 74,
  sym_action_body = 75,
  sym_params_block = 76,
  sym_parameter_block = 77,
  sym_parameter_decl = 78,
  sym_label_declaration = 79,
  sym_property_assignment = 80,
  sym_value_expression = 81,
  sym_array_literal = 82,
  sym_event_handler = 83,
  sym_event_type = 84,
  sym_event_param = 85,
  sym_event_action = 86,
  sym_navigate_action = 87,
  sym_refresh_action = 88,
  sym_action_invocation = 89,
  sym_parameter_binding = 90,
  sym_binding_pair = 91,
  sym_type_ref = 92,
  sym_expression = 93,
  sym__logical_or = 94,
  sym__logical_and = 95,
  sym__comparison = 96,
  sym__comparison_op = 97,
  sym__addition = 98,
  sym__add_op = 99,
  sym__multiplication = 100,
  sym__mul_op = 101,
  sym__unary = 102,
  sym__primary = 103,
  sym_call_expr = 104,
  sym_field_expr = 105,
  sym_group_expr = 106,
  sym_boolean = 107,
  aux_sym_source_file_repeat1 = 108,
  aux_sym_module_declaration_repeat1 = 109,
  aux_sym_component_body_repeat1 = 110,
  aux_sym_parameter_block_repeat1 = 111,
  aux_sym_array_literal_repeat1 = 112,
  aux_sym_event_param_repeat1 = 113,
  aux_sym_parameter_binding_repeat1 = 114,
  aux_sym__logical_or_repeat1 = 115,
  aux_sym__logical_and_repeat1 = 116,
  aux_sym__comparison_repeat1 = 117,
  aux_sym__addition_repeat1 = 118,
  aux_sym__multiplication_repeat1 = 119,
  aux_sym_call_expr_repeat1 = 120,
  aux_sym_field_expr_repeat1 = 121,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_domain] = "domain",
  [anon_sym_LBRACE] = "{",
  [anon_sym_schema] = "schema",
  [anon_sym_SEMI] = ";",
  [anon_sym_RBRACE] = "}",
  [anon_sym_view] = "view",
  [anon_sym_container] = "container",
  [anon_sym_component] = "component",
  [anon_sym_action] = "action",
  [anon_sym_module] = "module",
  [anon_sym_input] = "input",
  [anon_sym_output] = "output",
  [anon_sym_params] = "params",
  [anon_sym_COMMA] = ",",
  [anon_sym_COLON] = ":",
  [anon_sym_label] = "label",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [anon_sym_on] = "on",
  [anon_sym_DASH_GT] = "->",
  [anon_sym_select] = "select",
  [anon_sym_submit] = "submit",
  [anon_sym_click] = "click",
  [anon_sym_change] = "change",
  [anon_sym_load] = "load",
  [anon_sym_save] = "save",
  [anon_sym_cancel] = "cancel",
  [anon_sym_delete] = "delete",
  [anon_sym_confirm] = "confirm",
  [anon_sym_back] = "back",
  [anon_sym_LPAREN] = "(",
  [anon_sym_RPAREN] = ")",
  [anon_sym_navigate] = "navigate",
  [anon_sym_refresh] = "refresh",
  [sym_stay_statement] = "stay_statement",
  [anon_sym_Uuid] = "Uuid",
  [anon_sym_String] = "String",
  [anon_sym_Int] = "Int",
  [anon_sym_Float] = "Float",
  [anon_sym_Boolean] = "Boolean",
  [anon_sym_DateTime] = "DateTime",
  [anon_sym_PIPE_PIPE] = "||",
  [anon_sym_AMP_AMP] = "&&",
  [anon_sym_EQ_EQ] = "==",
  [anon_sym_BANG_EQ] = "!=",
  [anon_sym_LT] = "<",
  [anon_sym_LT_EQ] = "<=",
  [anon_sym_GT] = ">",
  [anon_sym_GT_EQ] = ">=",
  [anon_sym_TILDE_EQ] = "~=",
  [anon_sym_BANG_TILDE] = "!~",
  [anon_sym_PLUS] = "+",
  [anon_sym_DASH] = "-",
  [anon_sym_STAR] = "*",
  [anon_sym_SLASH] = "/",
  [anon_sym_PERCENT] = "%",
  [anon_sym_BANG] = "!",
  [anon_sym_DOT] = ".",
  [sym_identifier] = "identifier",
  [sym_string] = "string",
  [sym_number] = "number",
  [anon_sym_true] = "true",
  [anon_sym_false] = "false",
  [sym_comment] = "comment",
  [sym_source_file] = "source_file",
  [sym__definition] = "_definition",
  [sym_domain_declaration] = "domain_declaration",
  [sym_view_declaration] = "view_declaration",
  [sym_container_declaration] = "container_declaration",
  [sym_component_declaration] = "component_declaration",
  [sym_action_declaration] = "action_declaration",
  [sym_module_declaration] = "module_declaration",
  [sym_view_body] = "view_body",
  [sym_component_body] = "component_body",
  [sym_action_body] = "action_body",
  [sym_params_block] = "params_block",
  [sym_parameter_block] = "parameter_block",
  [sym_parameter_decl] = "parameter_decl",
  [sym_label_declaration] = "label_declaration",
  [sym_property_assignment] = "property_assignment",
  [sym_value_expression] = "value_expression",
  [sym_array_literal] = "array_literal",
  [sym_event_handler] = "event_handler",
  [sym_event_type] = "event_type",
  [sym_event_param] = "event_param",
  [sym_event_action] = "event_action",
  [sym_navigate_action] = "navigate_action",
  [sym_refresh_action] = "refresh_action",
  [sym_action_invocation] = "action_invocation",
  [sym_parameter_binding] = "parameter_binding",
  [sym_binding_pair] = "binding_pair",
  [sym_type_ref] = "type_ref",
  [sym_expression] = "expression",
  [sym__logical_or] = "_logical_or",
  [sym__logical_and] = "_logical_and",
  [sym__comparison] = "_comparison",
  [sym__comparison_op] = "_comparison_op",
  [sym__addition] = "_addition",
  [sym__add_op] = "_add_op",
  [sym__multiplication] = "_multiplication",
  [sym__mul_op] = "_mul_op",
  [sym__unary] = "_unary",
  [sym__primary] = "_primary",
  [sym_call_expr] = "call_expr",
  [sym_field_expr] = "field_expr",
  [sym_group_expr] = "group_expr",
  [sym_boolean] = "boolean",
  [aux_sym_source_file_repeat1] = "source_file_repeat1",
  [aux_sym_module_declaration_repeat1] = "module_declaration_repeat1",
  [aux_sym_component_body_repeat1] = "component_body_repeat1",
  [aux_sym_parameter_block_repeat1] = "parameter_block_repeat1",
  [aux_sym_array_literal_repeat1] = "array_literal_repeat1",
  [aux_sym_event_param_repeat1] = "event_param_repeat1",
  [aux_sym_parameter_binding_repeat1] = "parameter_binding_repeat1",
  [aux_sym__logical_or_repeat1] = "_logical_or_repeat1",
  [aux_sym__logical_and_repeat1] = "_logical_and_repeat1",
  [aux_sym__comparison_repeat1] = "_comparison_repeat1",
  [aux_sym__addition_repeat1] = "_addition_repeat1",
  [aux_sym__multiplication_repeat1] = "_multiplication_repeat1",
  [aux_sym_call_expr_repeat1] = "call_expr_repeat1",
  [aux_sym_field_expr_repeat1] = "field_expr_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_domain] = anon_sym_domain,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_schema] = anon_sym_schema,
  [anon_sym_SEMI] = anon_sym_SEMI,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_view] = anon_sym_view,
  [anon_sym_container] = anon_sym_container,
  [anon_sym_component] = anon_sym_component,
  [anon_sym_action] = anon_sym_action,
  [anon_sym_module] = anon_sym_module,
  [anon_sym_input] = anon_sym_input,
  [anon_sym_output] = anon_sym_output,
  [anon_sym_params] = anon_sym_params,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_label] = anon_sym_label,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [anon_sym_on] = anon_sym_on,
  [anon_sym_DASH_GT] = anon_sym_DASH_GT,
  [anon_sym_select] = anon_sym_select,
  [anon_sym_submit] = anon_sym_submit,
  [anon_sym_click] = anon_sym_click,
  [anon_sym_change] = anon_sym_change,
  [anon_sym_load] = anon_sym_load,
  [anon_sym_save] = anon_sym_save,
  [anon_sym_cancel] = anon_sym_cancel,
  [anon_sym_delete] = anon_sym_delete,
  [anon_sym_confirm] = anon_sym_confirm,
  [anon_sym_back] = anon_sym_back,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_navigate] = anon_sym_navigate,
  [anon_sym_refresh] = anon_sym_refresh,
  [sym_stay_statement] = sym_stay_statement,
  [anon_sym_Uuid] = anon_sym_Uuid,
  [anon_sym_String] = anon_sym_String,
  [anon_sym_Int] = anon_sym_Int,
  [anon_sym_Float] = anon_sym_Float,
  [anon_sym_Boolean] = anon_sym_Boolean,
  [anon_sym_DateTime] = anon_sym_DateTime,
  [anon_sym_PIPE_PIPE] = anon_sym_PIPE_PIPE,
  [anon_sym_AMP_AMP] = anon_sym_AMP_AMP,
  [anon_sym_EQ_EQ] = anon_sym_EQ_EQ,
  [anon_sym_BANG_EQ] = anon_sym_BANG_EQ,
  [anon_sym_LT] = anon_sym_LT,
  [anon_sym_LT_EQ] = anon_sym_LT_EQ,
  [anon_sym_GT] = anon_sym_GT,
  [anon_sym_GT_EQ] = anon_sym_GT_EQ,
  [anon_sym_TILDE_EQ] = anon_sym_TILDE_EQ,
  [anon_sym_BANG_TILDE] = anon_sym_BANG_TILDE,
  [anon_sym_PLUS] = anon_sym_PLUS,
  [anon_sym_DASH] = anon_sym_DASH,
  [anon_sym_STAR] = anon_sym_STAR,
  [anon_sym_SLASH] = anon_sym_SLASH,
  [anon_sym_PERCENT] = anon_sym_PERCENT,
  [anon_sym_BANG] = anon_sym_BANG,
  [anon_sym_DOT] = anon_sym_DOT,
  [sym_identifier] = sym_identifier,
  [sym_string] = sym_string,
  [sym_number] = sym_number,
  [anon_sym_true] = anon_sym_true,
  [anon_sym_false] = anon_sym_false,
  [sym_comment] = sym_comment,
  [sym_source_file] = sym_source_file,
  [sym__definition] = sym__definition,
  [sym_domain_declaration] = sym_domain_declaration,
  [sym_view_declaration] = sym_view_declaration,
  [sym_container_declaration] = sym_container_declaration,
  [sym_component_declaration] = sym_component_declaration,
  [sym_action_declaration] = sym_action_declaration,
  [sym_module_declaration] = sym_module_declaration,
  [sym_view_body] = sym_view_body,
  [sym_component_body] = sym_component_body,
  [sym_action_body] = sym_action_body,
  [sym_params_block] = sym_params_block,
  [sym_parameter_block] = sym_parameter_block,
  [sym_parameter_decl] = sym_parameter_decl,
  [sym_label_declaration] = sym_label_declaration,
  [sym_property_assignment] = sym_property_assignment,
  [sym_value_expression] = sym_value_expression,
  [sym_array_literal] = sym_array_literal,
  [sym_event_handler] = sym_event_handler,
  [sym_event_type] = sym_event_type,
  [sym_event_param] = sym_event_param,
  [sym_event_action] = sym_event_action,
  [sym_navigate_action] = sym_navigate_action,
  [sym_refresh_action] = sym_refresh_action,
  [sym_action_invocation] = sym_action_invocation,
  [sym_parameter_binding] = sym_parameter_binding,
  [sym_binding_pair] = sym_binding_pair,
  [sym_type_ref] = sym_type_ref,
  [sym_expression] = sym_expression,
  [sym__logical_or] = sym__logical_or,
  [sym__logical_and] = sym__logical_and,
  [sym__comparison] = sym__comparison,
  [sym__comparison_op] = sym__comparison_op,
  [sym__addition] = sym__addition,
  [sym__add_op] = sym__add_op,
  [sym__multiplication] = sym__multiplication,
  [sym__mul_op] = sym__mul_op,
  [sym__unary] = sym__unary,
  [sym__primary] = sym__primary,
  [sym_call_expr] = sym_call_expr,
  [sym_field_expr] = sym_field_expr,
  [sym_group_expr] = sym_group_expr,
  [sym_boolean] = sym_boolean,
  [aux_sym_source_file_repeat1] = aux_sym_source_file_repeat1,
  [aux_sym_module_declaration_repeat1] = aux_sym_module_declaration_repeat1,
  [aux_sym_component_body_repeat1] = aux_sym_component_body_repeat1,
  [aux_sym_parameter_block_repeat1] = aux_sym_parameter_block_repeat1,
  [aux_sym_array_literal_repeat1] = aux_sym_array_literal_repeat1,
  [aux_sym_event_param_repeat1] = aux_sym_event_param_repeat1,
  [aux_sym_parameter_binding_repeat1] = aux_sym_parameter_binding_repeat1,
  [aux_sym__logical_or_repeat1] = aux_sym__logical_or_repeat1,
  [aux_sym__logical_and_repeat1] = aux_sym__logical_and_repeat1,
  [aux_sym__comparison_repeat1] = aux_sym__comparison_repeat1,
  [aux_sym__addition_repeat1] = aux_sym__addition_repeat1,
  [aux_sym__multiplication_repeat1] = aux_sym__multiplication_repeat1,
  [aux_sym_call_expr_repeat1] = aux_sym_call_expr_repeat1,
  [aux_sym_field_expr_repeat1] = aux_sym_field_expr_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_domain] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_schema] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SEMI] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_view] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_container] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_component] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_action] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_module] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_input] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_output] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_params] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_label] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACK] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_on] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_select] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_submit] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_click] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_change] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_load] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_save] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_cancel] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_delete] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_confirm] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_back] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_navigate] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_refresh] = {
    .visible = true,
    .named = false,
  },
  [sym_stay_statement] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_Uuid] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_String] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_Int] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_Float] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_Boolean] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DateTime] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PIPE_PIPE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_AMP_AMP] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_EQ_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LT_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_GT_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_TILDE_EQ] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG_TILDE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PLUS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_STAR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PERCENT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_string] = {
    .visible = true,
    .named = true,
  },
  [sym_number] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_true] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_false] = {
    .visible = true,
    .named = false,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_source_file] = {
    .visible = true,
    .named = true,
  },
  [sym__definition] = {
    .visible = false,
    .named = true,
  },
  [sym_domain_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_view_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_container_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_component_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_action_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_module_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_view_body] = {
    .visible = true,
    .named = true,
  },
  [sym_component_body] = {
    .visible = true,
    .named = true,
  },
  [sym_action_body] = {
    .visible = true,
    .named = true,
  },
  [sym_params_block] = {
    .visible = true,
    .named = true,
  },
  [sym_parameter_block] = {
    .visible = true,
    .named = true,
  },
  [sym_parameter_decl] = {
    .visible = true,
    .named = true,
  },
  [sym_label_declaration] = {
    .visible = true,
    .named = true,
  },
  [sym_property_assignment] = {
    .visible = true,
    .named = true,
  },
  [sym_value_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_array_literal] = {
    .visible = true,
    .named = true,
  },
  [sym_event_handler] = {
    .visible = true,
    .named = true,
  },
  [sym_event_type] = {
    .visible = true,
    .named = true,
  },
  [sym_event_param] = {
    .visible = true,
    .named = true,
  },
  [sym_event_action] = {
    .visible = true,
    .named = true,
  },
  [sym_navigate_action] = {
    .visible = true,
    .named = true,
  },
  [sym_refresh_action] = {
    .visible = true,
    .named = true,
  },
  [sym_action_invocation] = {
    .visible = true,
    .named = true,
  },
  [sym_parameter_binding] = {
    .visible = true,
    .named = true,
  },
  [sym_binding_pair] = {
    .visible = true,
    .named = true,
  },
  [sym_type_ref] = {
    .visible = true,
    .named = true,
  },
  [sym_expression] = {
    .visible = true,
    .named = true,
  },
  [sym__logical_or] = {
    .visible = false,
    .named = true,
  },
  [sym__logical_and] = {
    .visible = false,
    .named = true,
  },
  [sym__comparison] = {
    .visible = false,
    .named = true,
  },
  [sym__comparison_op] = {
    .visible = false,
    .named = true,
  },
  [sym__addition] = {
    .visible = false,
    .named = true,
  },
  [sym__add_op] = {
    .visible = false,
    .named = true,
  },
  [sym__multiplication] = {
    .visible = false,
    .named = true,
  },
  [sym__mul_op] = {
    .visible = false,
    .named = true,
  },
  [sym__unary] = {
    .visible = false,
    .named = true,
  },
  [sym__primary] = {
    .visible = false,
    .named = true,
  },
  [sym_call_expr] = {
    .visible = true,
    .named = true,
  },
  [sym_field_expr] = {
    .visible = true,
    .named = true,
  },
  [sym_group_expr] = {
    .visible = true,
    .named = true,
  },
  [sym_boolean] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_file_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_module_declaration_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_component_body_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_parameter_block_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_array_literal_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_event_param_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_parameter_binding_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__logical_or_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__logical_and_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__comparison_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__addition_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym__multiplication_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_call_expr_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_field_expr_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum ts_field_identifiers {
  field_action = 1,
  field_key = 2,
  field_name = 3,
  field_type = 4,
  field_value = 5,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_action] = "action",
  [field_key] = "key",
  [field_name] = "name",
  [field_type] = "type",
  [field_value] = "value",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 2},
  [2] = {.index = 2, .length = 2},
  [3] = {.index = 4, .length = 2},
  [4] = {.index = 6, .length = 2},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_key, 0},
    {field_value, 2},
  [2] =
    {field_name, 0},
    {field_type, 2},
  [4] =
    {field_action, 3},
    {field_type, 1},
  [6] =
    {field_action, 4},
    {field_type, 1},
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
};

static const uint16_t ts_non_terminal_alias_map[] = {
  0,
};

static const TSStateId ts_primary_state_ids[STATE_COUNT] = {
  [0] = 0,
  [1] = 1,
  [2] = 2,
  [3] = 3,
  [4] = 3,
  [5] = 5,
  [6] = 6,
  [7] = 7,
  [8] = 8,
  [9] = 9,
  [10] = 10,
  [11] = 11,
  [12] = 12,
  [13] = 13,
  [14] = 14,
  [15] = 15,
  [16] = 16,
  [17] = 17,
  [18] = 18,
  [19] = 19,
  [20] = 20,
  [21] = 21,
  [22] = 22,
  [23] = 23,
  [24] = 24,
  [25] = 25,
  [26] = 26,
  [27] = 27,
  [28] = 28,
  [29] = 29,
  [30] = 30,
  [31] = 31,
  [32] = 32,
  [33] = 33,
  [34] = 34,
  [35] = 35,
  [36] = 36,
  [37] = 37,
  [38] = 37,
  [39] = 39,
  [40] = 40,
  [41] = 41,
  [42] = 42,
  [43] = 42,
  [44] = 41,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 50,
  [51] = 47,
  [52] = 48,
  [53] = 53,
  [54] = 54,
  [55] = 54,
  [56] = 56,
  [57] = 56,
  [58] = 50,
  [59] = 59,
  [60] = 60,
  [61] = 60,
  [62] = 62,
  [63] = 63,
  [64] = 64,
  [65] = 65,
  [66] = 66,
  [67] = 64,
  [68] = 68,
  [69] = 69,
  [70] = 70,
  [71] = 71,
  [72] = 72,
  [73] = 73,
  [74] = 74,
  [75] = 75,
  [76] = 76,
  [77] = 77,
  [78] = 78,
  [79] = 79,
  [80] = 80,
  [81] = 81,
  [82] = 82,
  [83] = 83,
  [84] = 84,
  [85] = 85,
  [86] = 86,
  [87] = 87,
  [88] = 88,
  [89] = 89,
  [90] = 90,
  [91] = 91,
  [92] = 92,
  [93] = 93,
  [94] = 94,
  [95] = 88,
  [96] = 96,
  [97] = 97,
  [98] = 98,
  [99] = 99,
  [100] = 100,
  [101] = 82,
  [102] = 102,
  [103] = 103,
  [104] = 81,
  [105] = 92,
  [106] = 106,
  [107] = 97,
  [108] = 108,
  [109] = 109,
  [110] = 110,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 114,
  [115] = 115,
  [116] = 116,
  [117] = 117,
  [118] = 118,
  [119] = 119,
  [120] = 120,
  [121] = 121,
  [122] = 122,
  [123] = 123,
  [124] = 98,
  [125] = 83,
  [126] = 108,
  [127] = 118,
  [128] = 128,
  [129] = 129,
  [130] = 112,
  [131] = 131,
  [132] = 109,
  [133] = 133,
  [134] = 134,
  [135] = 96,
  [136] = 136,
  [137] = 137,
  [138] = 138,
  [139] = 139,
  [140] = 140,
  [141] = 141,
  [142] = 142,
  [143] = 143,
  [144] = 144,
  [145] = 145,
  [146] = 146,
  [147] = 147,
  [148] = 148,
  [149] = 93,
  [150] = 150,
  [151] = 151,
  [152] = 152,
  [153] = 153,
  [154] = 154,
  [155] = 155,
  [156] = 156,
  [157] = 157,
  [158] = 158,
  [159] = 159,
  [160] = 94,
  [161] = 161,
  [162] = 162,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 166,
  [167] = 167,
  [168] = 168,
  [169] = 169,
  [170] = 170,
  [171] = 171,
  [172] = 172,
  [173] = 173,
  [174] = 174,
  [175] = 175,
  [176] = 176,
  [177] = 177,
  [178] = 178,
  [179] = 179,
  [180] = 180,
  [181] = 181,
  [182] = 182,
  [183] = 183,
  [184] = 184,
  [185] = 185,
  [186] = 186,
  [187] = 187,
  [188] = 188,
  [189] = 189,
  [190] = 190,
  [191] = 191,
  [192] = 192,
  [193] = 193,
  [194] = 194,
  [195] = 195,
  [196] = 196,
  [197] = 197,
  [198] = 198,
  [199] = 199,
  [200] = 200,
  [201] = 201,
  [202] = 202,
  [203] = 203,
  [204] = 204,
  [205] = 205,
  [206] = 177,
  [207] = 207,
  [208] = 208,
  [209] = 184,
  [210] = 194,
  [211] = 211,
  [212] = 212,
  [213] = 213,
  [214] = 197,
  [215] = 215,
  [216] = 180,
  [217] = 217,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(67);
      if (lookahead == '!') ADVANCE(136);
      if (lookahead == '"') ADVANCE(2);
      if (lookahead == '%') ADVANCE(134);
      if (lookahead == '&') ADVANCE(3);
      if (lookahead == '(') ADVANCE(105);
      if (lookahead == ')') ADVANCE(106);
      if (lookahead == '*') ADVANCE(132);
      if (lookahead == '+') ADVANCE(129);
      if (lookahead == ',') ADVANCE(88);
      if (lookahead == '-') ADVANCE(131);
      if (lookahead == '.') ADVANCE(137);
      if (lookahead == '/') ADVANCE(133);
      if (lookahead == ':') ADVANCE(89);
      if (lookahead == ';') ADVANCE(73);
      if (lookahead == '<') ADVANCE(123);
      if (lookahead == '=') ADVANCE(13);
      if (lookahead == '>') ADVANCE(125);
      if (lookahead == 'B') ADVANCE(245);
      if (lookahead == 'D') ADVANCE(144);
      if (lookahead == 'F') ADVANCE(216);
      if (lookahead == 'I') ADVANCE(237);
      if (lookahead == 'S') ADVANCE(270);
      if (lookahead == 'U') ADVANCE(277);
      if (lookahead == '[') ADVANCE(91);
      if (lookahead == ']') ADVANCE(92);
      if (lookahead == 'a') ADVANCE(164);
      if (lookahead == 'b') ADVANCE(145);
      if (lookahead == 'c') ADVANCE(149);
      if (lookahead == 'd') ADVANCE(181);
      if (lookahead == 'f') ADVANCE(148);
      if (lookahead == 'i') ADVANCE(229);
      if (lookahead == 'l') ADVANCE(139);
      if (lookahead == 'm') ADVANCE(243);
      if (lookahead == 'n') ADVANCE(141);
      if (lookahead == 'o') ADVANCE(230);
      if (lookahead == 'p') ADVANCE(151);
      if (lookahead == 'r') ADVANCE(170);
      if (lookahead == 's') ADVANCE(146);
      if (lookahead == 't') ADVANCE(255);
      if (lookahead == 'v') ADVANCE(200);
      if (lookahead == '{') ADVANCE(70);
      if (lookahead == '|') ADVANCE(63);
      if (lookahead == '}') ADVANCE(74);
      if (lookahead == '~') ADVANCE(14);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(288);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('e' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 1:
      if (lookahead == '!') ADVANCE(135);
      if (lookahead == '"') ADVANCE(2);
      if (lookahead == '(') ADVANCE(105);
      if (lookahead == ')') ADVANCE(106);
      if (lookahead == '-') ADVANCE(130);
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == '[') ADVANCE(91);
      if (lookahead == ']') ADVANCE(92);
      if (lookahead == 'f') ADVANCE(148);
      if (lookahead == 't') ADVANCE(255);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(1)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(288);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 2:
      if (lookahead == '"') ADVANCE(287);
      if (lookahead == '\\') ADVANCE(65);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(2);
      END_STATE();
    case 3:
      if (lookahead == '&') ADVANCE(120);
      END_STATE();
    case 4:
      if (lookahead == '(') ADVANCE(105);
      if (lookahead == '-') ADVANCE(15);
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'c') ADVANCE(244);
      if (lookahead == 'l') ADVANCE(140);
      if (lookahead == 'o') ADVANCE(231);
      if (lookahead == 'p') ADVANCE(151);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(4)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 5:
      if (lookahead == '/') ADVANCE(292);
      END_STATE();
    case 6:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'B') ADVANCE(245);
      if (lookahead == 'D') ADVANCE(144);
      if (lookahead == 'F') ADVANCE(216);
      if (lookahead == 'I') ADVANCE(237);
      if (lookahead == 'S') ADVANCE(270);
      if (lookahead == 'U') ADVANCE(277);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(6)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 7:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'b') ADVANCE(145);
      if (lookahead == 'c') ADVANCE(150);
      if (lookahead == 'd') ADVANCE(182);
      if (lookahead == 'l') ADVANCE(247);
      if (lookahead == 's') ADVANCE(147);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 8:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'c') ADVANCE(244);
      if (lookahead == 'l') ADVANCE(140);
      if (lookahead == 'o') ADVANCE(231);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 9:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'c') ADVANCE(244);
      if (lookahead == 'o') ADVANCE(231);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(9)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 10:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'o') ADVANCE(231);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(10)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 11:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 12:
      if (lookahead == '=') ADVANCE(122);
      if (lookahead == '~') ADVANCE(128);
      END_STATE();
    case 13:
      if (lookahead == '=') ADVANCE(121);
      END_STATE();
    case 14:
      if (lookahead == '=') ADVANCE(127);
      END_STATE();
    case 15:
      if (lookahead == '>') ADVANCE(94);
      END_STATE();
    case 16:
      if (lookahead == 'a') ADVANCE(60);
      END_STATE();
    case 17:
      if (lookahead == 'a') ADVANCE(62);
      END_STATE();
    case 18:
      if (lookahead == 'a') ADVANCE(71);
      END_STATE();
    case 19:
      if (lookahead == 'a') ADVANCE(37);
      END_STATE();
    case 20:
      if (lookahead == 'a') ADVANCE(54);
      END_STATE();
    case 21:
      if (lookahead == 'c') ADVANCE(53);
      END_STATE();
    case 22:
      if (lookahead == 'c') ADVANCE(33);
      if (lookahead == 't') ADVANCE(17);
      END_STATE();
    case 23:
      if (lookahead == 'd') ADVANCE(56);
      END_STATE();
    case 24:
      if (lookahead == 'e') ADVANCE(30);
      END_STATE();
    case 25:
      if (lookahead == 'e') ADVANCE(61);
      END_STATE();
    case 26:
      if (lookahead == 'e') ADVANCE(50);
      END_STATE();
    case 27:
      if (lookahead == 'e') ADVANCE(81);
      END_STATE();
    case 28:
      if (lookahead == 'e') ADVANCE(107);
      END_STATE();
    case 29:
      if (lookahead == 'e') ADVANCE(40);
      END_STATE();
    case 30:
      if (lookahead == 'f') ADVANCE(49);
      END_STATE();
    case 31:
      if (lookahead == 'g') ADVANCE(20);
      END_STATE();
    case 32:
      if (lookahead == 'h') ADVANCE(109);
      END_STATE();
    case 33:
      if (lookahead == 'h') ADVANCE(29);
      END_STATE();
    case 34:
      if (lookahead == 'i') ADVANCE(31);
      END_STATE();
    case 35:
      if (lookahead == 'i') ADVANCE(25);
      END_STATE();
    case 36:
      if (lookahead == 'i') ADVANCE(46);
      END_STATE();
    case 37:
      if (lookahead == 'i') ADVANCE(43);
      END_STATE();
    case 38:
      if (lookahead == 'l') ADVANCE(27);
      END_STATE();
    case 39:
      if (lookahead == 'm') ADVANCE(19);
      END_STATE();
    case 40:
      if (lookahead == 'm') ADVANCE(18);
      END_STATE();
    case 41:
      if (lookahead == 'n') ADVANCE(47);
      END_STATE();
    case 42:
      if (lookahead == 'n') ADVANCE(79);
      END_STATE();
    case 43:
      if (lookahead == 'n') ADVANCE(68);
      END_STATE();
    case 44:
      if (lookahead == 'o') ADVANCE(39);
      END_STATE();
    case 45:
      if (lookahead == 'o') ADVANCE(23);
      END_STATE();
    case 46:
      if (lookahead == 'o') ADVANCE(42);
      END_STATE();
    case 47:
      if (lookahead == 'p') ADVANCE(58);
      END_STATE();
    case 48:
      if (lookahead == 'p') ADVANCE(59);
      END_STATE();
    case 49:
      if (lookahead == 'r') ADVANCE(26);
      END_STATE();
    case 50:
      if (lookahead == 's') ADVANCE(32);
      END_STATE();
    case 51:
      if (lookahead == 't') ADVANCE(83);
      END_STATE();
    case 52:
      if (lookahead == 't') ADVANCE(85);
      END_STATE();
    case 53:
      if (lookahead == 't') ADVANCE(36);
      END_STATE();
    case 54:
      if (lookahead == 't') ADVANCE(28);
      END_STATE();
    case 55:
      if (lookahead == 't') ADVANCE(48);
      END_STATE();
    case 56:
      if (lookahead == 'u') ADVANCE(38);
      END_STATE();
    case 57:
      if (lookahead == 'u') ADVANCE(55);
      END_STATE();
    case 58:
      if (lookahead == 'u') ADVANCE(51);
      END_STATE();
    case 59:
      if (lookahead == 'u') ADVANCE(52);
      END_STATE();
    case 60:
      if (lookahead == 'v') ADVANCE(34);
      END_STATE();
    case 61:
      if (lookahead == 'w') ADVANCE(75);
      END_STATE();
    case 62:
      if (lookahead == 'y') ADVANCE(111);
      END_STATE();
    case 63:
      if (lookahead == '|') ADVANCE(119);
      END_STATE();
    case 64:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(289);
      END_STATE();
    case 65:
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(2);
      END_STATE();
    case 66:
      if (eof) ADVANCE(67);
      if (lookahead == '!') ADVANCE(12);
      if (lookahead == '%') ADVANCE(134);
      if (lookahead == '&') ADVANCE(3);
      if (lookahead == '(') ADVANCE(105);
      if (lookahead == ')') ADVANCE(106);
      if (lookahead == '*') ADVANCE(132);
      if (lookahead == '+') ADVANCE(129);
      if (lookahead == ',') ADVANCE(88);
      if (lookahead == '-') ADVANCE(130);
      if (lookahead == '.') ADVANCE(137);
      if (lookahead == '/') ADVANCE(133);
      if (lookahead == ';') ADVANCE(73);
      if (lookahead == '<') ADVANCE(123);
      if (lookahead == '=') ADVANCE(13);
      if (lookahead == '>') ADVANCE(125);
      if (lookahead == ']') ADVANCE(92);
      if (lookahead == 'a') ADVANCE(21);
      if (lookahead == 'd') ADVANCE(44);
      if (lookahead == 'i') ADVANCE(41);
      if (lookahead == 'm') ADVANCE(45);
      if (lookahead == 'n') ADVANCE(16);
      if (lookahead == 'o') ADVANCE(57);
      if (lookahead == 'r') ADVANCE(24);
      if (lookahead == 's') ADVANCE(22);
      if (lookahead == 'v') ADVANCE(35);
      if (lookahead == '|') ADVANCE(63);
      if (lookahead == '}') ADVANCE(74);
      if (lookahead == '~') ADVANCE(14);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(66)
      END_STATE();
    case 67:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 68:
      ACCEPT_TOKEN(anon_sym_domain);
      END_STATE();
    case 69:
      ACCEPT_TOKEN(anon_sym_domain);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 70:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 71:
      ACCEPT_TOKEN(anon_sym_schema);
      END_STATE();
    case 72:
      ACCEPT_TOKEN(anon_sym_schema);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 73:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 74:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 75:
      ACCEPT_TOKEN(anon_sym_view);
      END_STATE();
    case 76:
      ACCEPT_TOKEN(anon_sym_view);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 77:
      ACCEPT_TOKEN(anon_sym_container);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 78:
      ACCEPT_TOKEN(anon_sym_component);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 79:
      ACCEPT_TOKEN(anon_sym_action);
      END_STATE();
    case 80:
      ACCEPT_TOKEN(anon_sym_action);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 81:
      ACCEPT_TOKEN(anon_sym_module);
      END_STATE();
    case 82:
      ACCEPT_TOKEN(anon_sym_module);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 83:
      ACCEPT_TOKEN(anon_sym_input);
      END_STATE();
    case 84:
      ACCEPT_TOKEN(anon_sym_input);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 85:
      ACCEPT_TOKEN(anon_sym_output);
      END_STATE();
    case 86:
      ACCEPT_TOKEN(anon_sym_output);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 87:
      ACCEPT_TOKEN(anon_sym_params);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 88:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 89:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 90:
      ACCEPT_TOKEN(anon_sym_label);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 91:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 92:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 93:
      ACCEPT_TOKEN(anon_sym_on);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 94:
      ACCEPT_TOKEN(anon_sym_DASH_GT);
      END_STATE();
    case 95:
      ACCEPT_TOKEN(anon_sym_select);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 96:
      ACCEPT_TOKEN(anon_sym_submit);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 97:
      ACCEPT_TOKEN(anon_sym_click);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 98:
      ACCEPT_TOKEN(anon_sym_change);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 99:
      ACCEPT_TOKEN(anon_sym_load);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 100:
      ACCEPT_TOKEN(anon_sym_save);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 101:
      ACCEPT_TOKEN(anon_sym_cancel);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 102:
      ACCEPT_TOKEN(anon_sym_delete);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 103:
      ACCEPT_TOKEN(anon_sym_confirm);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 104:
      ACCEPT_TOKEN(anon_sym_back);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 105:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 106:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 107:
      ACCEPT_TOKEN(anon_sym_navigate);
      END_STATE();
    case 108:
      ACCEPT_TOKEN(anon_sym_navigate);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 109:
      ACCEPT_TOKEN(anon_sym_refresh);
      END_STATE();
    case 110:
      ACCEPT_TOKEN(anon_sym_refresh);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 111:
      ACCEPT_TOKEN(sym_stay_statement);
      END_STATE();
    case 112:
      ACCEPT_TOKEN(sym_stay_statement);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 113:
      ACCEPT_TOKEN(anon_sym_Uuid);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 114:
      ACCEPT_TOKEN(anon_sym_String);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 115:
      ACCEPT_TOKEN(anon_sym_Int);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 116:
      ACCEPT_TOKEN(anon_sym_Float);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 117:
      ACCEPT_TOKEN(anon_sym_Boolean);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 118:
      ACCEPT_TOKEN(anon_sym_DateTime);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 119:
      ACCEPT_TOKEN(anon_sym_PIPE_PIPE);
      END_STATE();
    case 120:
      ACCEPT_TOKEN(anon_sym_AMP_AMP);
      END_STATE();
    case 121:
      ACCEPT_TOKEN(anon_sym_EQ_EQ);
      END_STATE();
    case 122:
      ACCEPT_TOKEN(anon_sym_BANG_EQ);
      END_STATE();
    case 123:
      ACCEPT_TOKEN(anon_sym_LT);
      if (lookahead == '=') ADVANCE(124);
      END_STATE();
    case 124:
      ACCEPT_TOKEN(anon_sym_LT_EQ);
      END_STATE();
    case 125:
      ACCEPT_TOKEN(anon_sym_GT);
      if (lookahead == '=') ADVANCE(126);
      END_STATE();
    case 126:
      ACCEPT_TOKEN(anon_sym_GT_EQ);
      END_STATE();
    case 127:
      ACCEPT_TOKEN(anon_sym_TILDE_EQ);
      END_STATE();
    case 128:
      ACCEPT_TOKEN(anon_sym_BANG_TILDE);
      END_STATE();
    case 129:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 130:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 131:
      ACCEPT_TOKEN(anon_sym_DASH);
      if (lookahead == '>') ADVANCE(94);
      END_STATE();
    case 132:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 133:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '/') ADVANCE(292);
      END_STATE();
    case 134:
      ACCEPT_TOKEN(anon_sym_PERCENT);
      END_STATE();
    case 135:
      ACCEPT_TOKEN(anon_sym_BANG);
      END_STATE();
    case 136:
      ACCEPT_TOKEN(anon_sym_BANG);
      if (lookahead == '=') ADVANCE(122);
      if (lookahead == '~') ADVANCE(128);
      END_STATE();
    case 137:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 138:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(204);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 139:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(161);
      if (lookahead == 'o') ADVANCE(153);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 140:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(161);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 141:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(283);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 142:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(285);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 143:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(72);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 144:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(271);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 145:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(162);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 146:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(282);
      if (lookahead == 'c') ADVANCE(199);
      if (lookahead == 'e') ADVANCE(218);
      if (lookahead == 't') ADVANCE(142);
      if (lookahead == 'u') ADVANCE(160);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 147:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(282);
      if (lookahead == 'e') ADVANCE(218);
      if (lookahead == 'u') ADVANCE(160);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 148:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(213);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 149:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(238);
      if (lookahead == 'h') ADVANCE(154);
      if (lookahead == 'l') ADVANCE(203);
      if (lookahead == 'o') ADVANCE(222);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 150:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(238);
      if (lookahead == 'h') ADVANCE(154);
      if (lookahead == 'l') ADVANCE(203);
      if (lookahead == 'o') ADVANCE(236);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 151:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(258);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 152:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(224);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 153:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(168);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 154:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(232);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 155:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(264);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 156:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(235);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 157:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(208);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 158:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(274);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 159:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(210);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 160:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(225);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 161:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(184);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 162:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(211);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 163:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(212);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 164:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(272);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 165:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(267);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 166:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(185);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 167:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(113);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 168:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(99);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 169:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(281);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 170:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(192);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 171:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(284);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 172:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(138);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 173:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(100);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 174:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(290);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 175:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(291);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 176:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(98);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 177:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(102);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 178:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(82);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 179:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(118);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 180:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(108);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 181:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(217);
      if (lookahead == 'o') ADVANCE(226);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 182:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(217);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 183:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(261);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 184:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(214);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 185:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(215);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 186:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(254);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 187:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(156);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 188:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(240);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 189:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(165);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 190:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(227);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 191:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(273);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 192:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(259);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 193:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(207);
      if (lookahead == 't') ADVANCE(159);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 194:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(207);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 195:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(114);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 196:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(176);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 197:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(158);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 198:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(110);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 199:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(190);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 200:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(171);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 201:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(167);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 202:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(197);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 203:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(163);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 204:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(228);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 205:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(246);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 206:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(239);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 207:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(257);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 208:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(234);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 209:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(268);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 210:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(242);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 211:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(104);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 212:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(97);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 213:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(262);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 214:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(90);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 215:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(101);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 216:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(249);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 217:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(191);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 218:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(189);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 219:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(187);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 220:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(178);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 221:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(103);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 222:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(253);
      if (lookahead == 'n') ADVANCE(193);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 223:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(253);
      if (lookahead == 'n') ADVANCE(276);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 224:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(260);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 225:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(209);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 226:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(157);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 227:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(143);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 228:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(179);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 229:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(251);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 230:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(93);
      if (lookahead == 'u') ADVANCE(275);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 231:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(93);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 232:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(196);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 233:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(80);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 234:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(69);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 235:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(117);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 236:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(194);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 237:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(263);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 238:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(166);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 239:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(195);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 240:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(269);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 241:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(188);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 242:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(186);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 243:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(169);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 244:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(223);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 245:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(250);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 246:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(233);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 247:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(153);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 248:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(241);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 249:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(155);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 250:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(219);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 251:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(278);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 252:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(279);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 253:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(248);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 254:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(77);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 255:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(280);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 256:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(206);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 257:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(221);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 258:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(152);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 259:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(183);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 260:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(87);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 261:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(198);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 262:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(175);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 263:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(115);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 264:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(116);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 265:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(84);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 266:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(86);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 267:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(95);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 268:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(96);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 269:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(78);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 270:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(256);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 271:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(172);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 272:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(205);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 273:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(177);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 274:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(180);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 275:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(252);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 276:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(159);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 277:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(201);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 278:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(265);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 279:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(266);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 280:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(174);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 281:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(220);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 282:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(173);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 283:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(202);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 284:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'w') ADVANCE(76);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 285:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'y') ADVANCE(112);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 286:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 287:
      ACCEPT_TOKEN(sym_string);
      END_STATE();
    case 288:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '.') ADVANCE(64);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(288);
      END_STATE();
    case 289:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(289);
      END_STATE();
    case 290:
      ACCEPT_TOKEN(anon_sym_true);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 291:
      ACCEPT_TOKEN(anon_sym_false);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(292);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 66},
  [2] = {.lex_state = 1},
  [3] = {.lex_state = 1},
  [4] = {.lex_state = 1},
  [5] = {.lex_state = 1},
  [6] = {.lex_state = 66},
  [7] = {.lex_state = 66},
  [8] = {.lex_state = 66},
  [9] = {.lex_state = 66},
  [10] = {.lex_state = 66},
  [11] = {.lex_state = 66},
  [12] = {.lex_state = 66},
  [13] = {.lex_state = 1},
  [14] = {.lex_state = 66},
  [15] = {.lex_state = 66},
  [16] = {.lex_state = 66},
  [17] = {.lex_state = 66},
  [18] = {.lex_state = 66},
  [19] = {.lex_state = 66},
  [20] = {.lex_state = 1},
  [21] = {.lex_state = 1},
  [22] = {.lex_state = 1},
  [23] = {.lex_state = 66},
  [24] = {.lex_state = 66},
  [25] = {.lex_state = 66},
  [26] = {.lex_state = 66},
  [27] = {.lex_state = 66},
  [28] = {.lex_state = 1},
  [29] = {.lex_state = 66},
  [30] = {.lex_state = 66},
  [31] = {.lex_state = 66},
  [32] = {.lex_state = 66},
  [33] = {.lex_state = 1},
  [34] = {.lex_state = 1},
  [35] = {.lex_state = 66},
  [36] = {.lex_state = 1},
  [37] = {.lex_state = 4},
  [38] = {.lex_state = 4},
  [39] = {.lex_state = 1},
  [40] = {.lex_state = 1},
  [41] = {.lex_state = 8},
  [42] = {.lex_state = 7},
  [43] = {.lex_state = 7},
  [44] = {.lex_state = 8},
  [45] = {.lex_state = 66},
  [46] = {.lex_state = 66},
  [47] = {.lex_state = 9},
  [48] = {.lex_state = 9},
  [49] = {.lex_state = 9},
  [50] = {.lex_state = 9},
  [51] = {.lex_state = 9},
  [52] = {.lex_state = 9},
  [53] = {.lex_state = 9},
  [54] = {.lex_state = 9},
  [55] = {.lex_state = 9},
  [56] = {.lex_state = 9},
  [57] = {.lex_state = 9},
  [58] = {.lex_state = 9},
  [59] = {.lex_state = 9},
  [60] = {.lex_state = 66},
  [61] = {.lex_state = 66},
  [62] = {.lex_state = 6},
  [63] = {.lex_state = 0},
  [64] = {.lex_state = 66},
  [65] = {.lex_state = 0},
  [66] = {.lex_state = 0},
  [67] = {.lex_state = 66},
  [68] = {.lex_state = 0},
  [69] = {.lex_state = 0},
  [70] = {.lex_state = 0},
  [71] = {.lex_state = 0},
  [72] = {.lex_state = 8},
  [73] = {.lex_state = 66},
  [74] = {.lex_state = 10},
  [75] = {.lex_state = 10},
  [76] = {.lex_state = 10},
  [77] = {.lex_state = 0},
  [78] = {.lex_state = 10},
  [79] = {.lex_state = 66},
  [80] = {.lex_state = 10},
  [81] = {.lex_state = 9},
  [82] = {.lex_state = 66},
  [83] = {.lex_state = 9},
  [84] = {.lex_state = 9},
  [85] = {.lex_state = 66},
  [86] = {.lex_state = 0},
  [87] = {.lex_state = 66},
  [88] = {.lex_state = 66},
  [89] = {.lex_state = 66},
  [90] = {.lex_state = 66},
  [91] = {.lex_state = 66},
  [92] = {.lex_state = 66},
  [93] = {.lex_state = 9},
  [94] = {.lex_state = 9},
  [95] = {.lex_state = 9},
  [96] = {.lex_state = 9},
  [97] = {.lex_state = 9},
  [98] = {.lex_state = 9},
  [99] = {.lex_state = 9},
  [100] = {.lex_state = 9},
  [101] = {.lex_state = 9},
  [102] = {.lex_state = 9},
  [103] = {.lex_state = 9},
  [104] = {.lex_state = 66},
  [105] = {.lex_state = 9},
  [106] = {.lex_state = 11},
  [107] = {.lex_state = 10},
  [108] = {.lex_state = 4},
  [109] = {.lex_state = 0},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 0},
  [112] = {.lex_state = 0},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 0},
  [118] = {.lex_state = 11},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 0},
  [122] = {.lex_state = 0},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 10},
  [125] = {.lex_state = 10},
  [126] = {.lex_state = 4},
  [127] = {.lex_state = 11},
  [128] = {.lex_state = 0},
  [129] = {.lex_state = 0},
  [130] = {.lex_state = 0},
  [131] = {.lex_state = 0},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 0},
  [134] = {.lex_state = 0},
  [135] = {.lex_state = 66},
  [136] = {.lex_state = 0},
  [137] = {.lex_state = 0},
  [138] = {.lex_state = 0},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 0},
  [141] = {.lex_state = 0},
  [142] = {.lex_state = 0},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 0},
  [145] = {.lex_state = 0},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 0},
  [149] = {.lex_state = 66},
  [150] = {.lex_state = 0},
  [151] = {.lex_state = 0},
  [152] = {.lex_state = 4},
  [153] = {.lex_state = 0},
  [154] = {.lex_state = 0},
  [155] = {.lex_state = 11},
  [156] = {.lex_state = 0},
  [157] = {.lex_state = 11},
  [158] = {.lex_state = 0},
  [159] = {.lex_state = 0},
  [160] = {.lex_state = 66},
  [161] = {.lex_state = 0},
  [162] = {.lex_state = 0},
  [163] = {.lex_state = 66},
  [164] = {.lex_state = 0},
  [165] = {.lex_state = 0},
  [166] = {.lex_state = 0},
  [167] = {.lex_state = 0},
  [168] = {.lex_state = 0},
  [169] = {.lex_state = 0},
  [170] = {.lex_state = 0},
  [171] = {.lex_state = 0},
  [172] = {.lex_state = 0},
  [173] = {.lex_state = 0},
  [174] = {.lex_state = 0},
  [175] = {.lex_state = 0},
  [176] = {.lex_state = 0},
  [177] = {.lex_state = 0},
  [178] = {.lex_state = 0},
  [179] = {.lex_state = 0},
  [180] = {.lex_state = 4},
  [181] = {.lex_state = 0},
  [182] = {.lex_state = 0},
  [183] = {.lex_state = 11},
  [184] = {.lex_state = 0},
  [185] = {.lex_state = 0},
  [186] = {.lex_state = 0},
  [187] = {.lex_state = 0},
  [188] = {.lex_state = 0},
  [189] = {.lex_state = 11},
  [190] = {.lex_state = 11},
  [191] = {.lex_state = 0},
  [192] = {.lex_state = 0},
  [193] = {.lex_state = 0},
  [194] = {.lex_state = 0},
  [195] = {.lex_state = 0},
  [196] = {.lex_state = 4},
  [197] = {.lex_state = 0},
  [198] = {.lex_state = 0},
  [199] = {.lex_state = 4},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 0},
  [202] = {.lex_state = 0},
  [203] = {.lex_state = 66},
  [204] = {.lex_state = 66},
  [205] = {.lex_state = 0},
  [206] = {.lex_state = 0},
  [207] = {.lex_state = 11},
  [208] = {.lex_state = 0},
  [209] = {.lex_state = 0},
  [210] = {.lex_state = 0},
  [211] = {.lex_state = 0},
  [212] = {.lex_state = 0},
  [213] = {.lex_state = 0},
  [214] = {.lex_state = 0},
  [215] = {.lex_state = 0},
  [216] = {.lex_state = 4},
  [217] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_domain] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_schema] = ACTIONS(1),
    [anon_sym_SEMI] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_view] = ACTIONS(1),
    [anon_sym_container] = ACTIONS(1),
    [anon_sym_component] = ACTIONS(1),
    [anon_sym_action] = ACTIONS(1),
    [anon_sym_module] = ACTIONS(1),
    [anon_sym_input] = ACTIONS(1),
    [anon_sym_output] = ACTIONS(1),
    [anon_sym_params] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_label] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [anon_sym_on] = ACTIONS(1),
    [anon_sym_DASH_GT] = ACTIONS(1),
    [anon_sym_select] = ACTIONS(1),
    [anon_sym_submit] = ACTIONS(1),
    [anon_sym_click] = ACTIONS(1),
    [anon_sym_change] = ACTIONS(1),
    [anon_sym_load] = ACTIONS(1),
    [anon_sym_save] = ACTIONS(1),
    [anon_sym_cancel] = ACTIONS(1),
    [anon_sym_delete] = ACTIONS(1),
    [anon_sym_confirm] = ACTIONS(1),
    [anon_sym_back] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_navigate] = ACTIONS(1),
    [anon_sym_refresh] = ACTIONS(1),
    [sym_stay_statement] = ACTIONS(1),
    [anon_sym_Uuid] = ACTIONS(1),
    [anon_sym_String] = ACTIONS(1),
    [anon_sym_Int] = ACTIONS(1),
    [anon_sym_Float] = ACTIONS(1),
    [anon_sym_Boolean] = ACTIONS(1),
    [anon_sym_DateTime] = ACTIONS(1),
    [anon_sym_PIPE_PIPE] = ACTIONS(1),
    [anon_sym_AMP_AMP] = ACTIONS(1),
    [anon_sym_EQ_EQ] = ACTIONS(1),
    [anon_sym_BANG_EQ] = ACTIONS(1),
    [anon_sym_LT] = ACTIONS(1),
    [anon_sym_LT_EQ] = ACTIONS(1),
    [anon_sym_GT] = ACTIONS(1),
    [anon_sym_GT_EQ] = ACTIONS(1),
    [anon_sym_TILDE_EQ] = ACTIONS(1),
    [anon_sym_BANG_TILDE] = ACTIONS(1),
    [anon_sym_PLUS] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [anon_sym_STAR] = ACTIONS(1),
    [anon_sym_SLASH] = ACTIONS(1),
    [anon_sym_PERCENT] = ACTIONS(1),
    [anon_sym_BANG] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [sym_identifier] = ACTIONS(1),
    [sym_string] = ACTIONS(1),
    [sym_number] = ACTIONS(1),
    [anon_sym_true] = ACTIONS(1),
    [anon_sym_false] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source_file] = STATE(212),
    [sym__definition] = STATE(45),
    [sym_domain_declaration] = STATE(45),
    [sym_view_declaration] = STATE(45),
    [sym_action_declaration] = STATE(45),
    [sym_module_declaration] = STATE(45),
    [aux_sym_source_file_repeat1] = STATE(45),
    [ts_builtin_sym_end] = ACTIONS(5),
    [anon_sym_domain] = ACTIONS(7),
    [anon_sym_view] = ACTIONS(9),
    [anon_sym_action] = ACTIONS(11),
    [anon_sym_module] = ACTIONS(13),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACK,
    ACTIONS(17), 1,
      anon_sym_RBRACK,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(128), 1,
      sym_value_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(123), 2,
      sym_array_literal,
      sym_expression,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [58] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACK,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(206), 1,
      sym_value_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(123), 2,
      sym_array_literal,
      sym_expression,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [113] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACK,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(177), 1,
      sym_value_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(123), 2,
      sym_array_literal,
      sym_expression,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [168] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACK,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(150), 1,
      sym_value_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(123), 2,
      sym_array_literal,
      sym_expression,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [223] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_DOT,
    STATE(7), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(31), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(29), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [257] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_DOT,
    STATE(8), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(37), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(35), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [291] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 1,
      anon_sym_DOT,
    STATE(8), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(41), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(39), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [325] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(52), 1,
      anon_sym_SLASH,
    STATE(12), 1,
      aux_sym__multiplication_repeat1,
    STATE(40), 1,
      sym__mul_op,
    ACTIONS(48), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(50), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(46), 15,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
  [363] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(56), 1,
      anon_sym_LPAREN,
    ACTIONS(60), 1,
      anon_sym_DOT,
    ACTIONS(58), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(54), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [397] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(69), 1,
      anon_sym_SLASH,
    STATE(11), 1,
      aux_sym__multiplication_repeat1,
    STATE(40), 1,
      sym__mul_op,
    ACTIONS(64), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(66), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(62), 15,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
  [435] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(52), 1,
      anon_sym_SLASH,
    STATE(11), 1,
      aux_sym__multiplication_repeat1,
    STATE(40), 1,
      sym__mul_op,
    ACTIONS(50), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(74), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(72), 15,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
  [473] = 14,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    ACTIONS(76), 1,
      anon_sym_RPAREN,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(119), 1,
      sym_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [524] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(41), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(39), 18,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
      anon_sym_DOT,
  [553] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(80), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(78), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [581] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(84), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(82), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [609] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(64), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(62), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [637] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(88), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(86), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [665] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(92), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(90), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [693] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(154), 1,
      sym_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [741] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(185), 1,
      sym_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [789] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(69), 1,
      sym__logical_and,
    STATE(86), 1,
      sym__logical_or,
    STATE(158), 1,
      sym_expression,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [837] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(96), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(94), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [865] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(100), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(98), 17,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_STAR,
      anon_sym_PERCENT,
  [893] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(27), 1,
      aux_sym__addition_repeat1,
    STATE(36), 1,
      sym__add_op,
    ACTIONS(104), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(106), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(102), 13,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
  [926] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(25), 1,
      aux_sym__addition_repeat1,
    STATE(36), 1,
      sym__add_op,
    ACTIONS(106), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(110), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(108), 13,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
  [959] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(27), 1,
      aux_sym__addition_repeat1,
    STATE(36), 1,
      sym__add_op,
    ACTIONS(114), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(116), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(112), 13,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
  [992] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(65), 1,
      sym__comparison,
    STATE(77), 1,
      sym__logical_and,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1034] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(30), 1,
      aux_sym__comparison_repeat1,
    STATE(34), 1,
      sym__comparison_op,
    ACTIONS(123), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(121), 6,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
    ACTIONS(119), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1065] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(32), 1,
      aux_sym__comparison_repeat1,
    STATE(34), 1,
      sym__comparison_op,
    ACTIONS(123), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(121), 6,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
    ACTIONS(125), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1096] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(114), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(112), 15,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1121] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(32), 1,
      aux_sym__comparison_repeat1,
    STATE(34), 1,
      sym__comparison_op,
    ACTIONS(132), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(129), 6,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
    ACTIONS(127), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1152] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(29), 1,
      sym__addition,
    STATE(70), 1,
      sym__comparison,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1191] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(26), 1,
      sym__multiplication,
    STATE(35), 1,
      sym__addition,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1227] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(135), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(127), 13,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
  [1250] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    STATE(31), 1,
      sym__multiplication,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(25), 2,
      sym_string,
      sym_number,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(9), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1283] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(137), 1,
      anon_sym_RBRACE,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(143), 1,
      anon_sym_params,
    ACTIONS(145), 1,
      anon_sym_label,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    STATE(44), 1,
      sym_params_block,
    STATE(51), 1,
      sym_label_declaration,
    STATE(50), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1321] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(143), 1,
      anon_sym_params,
    ACTIONS(145), 1,
      anon_sym_label,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(151), 1,
      anon_sym_RBRACE,
    STATE(41), 1,
      sym_params_block,
    STATE(47), 1,
      sym_label_declaration,
    STATE(58), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1359] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(153), 2,
      sym_string,
      sym_number,
    STATE(23), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1389] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(19), 1,
      anon_sym_LPAREN,
    ACTIONS(23), 1,
      sym_identifier,
    ACTIONS(21), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(155), 2,
      sym_string,
      sym_number,
    STATE(17), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1419] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(145), 1,
      anon_sym_label,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(157), 1,
      anon_sym_RBRACE,
    STATE(57), 1,
      sym_label_declaration,
    STATE(54), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1451] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(126), 1,
      sym_event_type,
    ACTIONS(159), 11,
      anon_sym_select,
      anon_sym_submit,
      anon_sym_click,
      anon_sym_change,
      anon_sym_load,
      anon_sym_save,
      anon_sym_cancel,
      anon_sym_delete,
      anon_sym_confirm,
      anon_sym_back,
      sym_identifier,
  [1471] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(108), 1,
      sym_event_type,
    ACTIONS(159), 11,
      anon_sym_select,
      anon_sym_submit,
      anon_sym_click,
      anon_sym_change,
      anon_sym_load,
      anon_sym_save,
      anon_sym_cancel,
      anon_sym_delete,
      anon_sym_confirm,
      anon_sym_back,
      sym_identifier,
  [1491] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(145), 1,
      anon_sym_label,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(56), 1,
      sym_label_declaration,
    STATE(55), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1523] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(7), 1,
      anon_sym_domain,
    ACTIONS(9), 1,
      anon_sym_view,
    ACTIONS(11), 1,
      anon_sym_action,
    ACTIONS(13), 1,
      anon_sym_module,
    ACTIONS(163), 1,
      ts_builtin_sym_end,
    STATE(46), 6,
      sym__definition,
      sym_domain_declaration,
      sym_view_declaration,
      sym_action_declaration,
      sym_module_declaration,
      aux_sym_source_file_repeat1,
  [1550] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(165), 1,
      ts_builtin_sym_end,
    ACTIONS(167), 1,
      anon_sym_domain,
    ACTIONS(170), 1,
      anon_sym_view,
    ACTIONS(173), 1,
      anon_sym_action,
    ACTIONS(176), 1,
      anon_sym_module,
    STATE(46), 6,
      sym__definition,
      sym_domain_declaration,
      sym_view_declaration,
      sym_action_declaration,
      sym_module_declaration,
      aux_sym_source_file_repeat1,
  [1577] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(157), 1,
      anon_sym_RBRACE,
    STATE(54), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1603] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(179), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1629] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(181), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1655] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1681] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(55), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1707] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(183), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1733] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(185), 1,
      anon_sym_RBRACE,
    ACTIONS(187), 1,
      anon_sym_container,
    ACTIONS(190), 1,
      anon_sym_component,
    ACTIONS(193), 1,
      anon_sym_on,
    ACTIONS(196), 1,
      sym_identifier,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1759] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(199), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1785] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(201), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1811] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(201), 1,
      anon_sym_RBRACE,
    STATE(48), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1837] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(199), 1,
      anon_sym_RBRACE,
    STATE(52), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1863] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(157), 1,
      anon_sym_RBRACE,
    STATE(53), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1889] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_container,
    ACTIONS(141), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_on,
    ACTIONS(149), 1,
      sym_identifier,
    ACTIONS(203), 1,
      anon_sym_RBRACE,
    STATE(49), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1915] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(205), 1,
      anon_sym_action,
    ACTIONS(207), 1,
      anon_sym_navigate,
    ACTIONS(209), 1,
      anon_sym_refresh,
    ACTIONS(211), 1,
      sym_stay_statement,
    STATE(194), 1,
      sym_event_action,
    STATE(164), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [1939] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(205), 1,
      anon_sym_action,
    ACTIONS(207), 1,
      anon_sym_navigate,
    ACTIONS(209), 1,
      anon_sym_refresh,
    ACTIONS(211), 1,
      sym_stay_statement,
    STATE(210), 1,
      sym_event_action,
    STATE(164), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [1963] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(139), 1,
      sym_type_ref,
    ACTIONS(213), 7,
      anon_sym_Uuid,
      anon_sym_String,
      anon_sym_Int,
      anon_sym_Float,
      anon_sym_Boolean,
      anon_sym_DateTime,
      sym_identifier,
  [1979] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(217), 1,
      anon_sym_AMP_AMP,
    STATE(66), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(215), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [1997] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(205), 1,
      anon_sym_action,
    ACTIONS(207), 1,
      anon_sym_navigate,
    ACTIONS(209), 1,
      anon_sym_refresh,
    ACTIONS(211), 1,
      sym_stay_statement,
    STATE(184), 1,
      sym_event_action,
    STATE(164), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2021] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(217), 1,
      anon_sym_AMP_AMP,
    STATE(63), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(219), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2039] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(223), 1,
      anon_sym_AMP_AMP,
    STATE(66), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(221), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2057] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(205), 1,
      anon_sym_action,
    ACTIONS(207), 1,
      anon_sym_navigate,
    ACTIONS(209), 1,
      anon_sym_refresh,
    ACTIONS(211), 1,
      sym_stay_statement,
    STATE(209), 1,
      sym_event_action,
    STATE(164), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2081] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(228), 1,
      anon_sym_PIPE_PIPE,
    STATE(68), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(226), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2098] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(233), 1,
      anon_sym_PIPE_PIPE,
    STATE(71), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(231), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2115] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(221), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [2128] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(233), 1,
      anon_sym_PIPE_PIPE,
    STATE(68), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(235), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2145] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(237), 1,
      anon_sym_RBRACE,
    ACTIONS(239), 5,
      anon_sym_container,
      anon_sym_component,
      anon_sym_label,
      anon_sym_on,
      sym_identifier,
  [2159] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(241), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2171] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 1,
      anon_sym_RBRACE,
    ACTIONS(245), 1,
      anon_sym_on,
    ACTIONS(247), 1,
      sym_identifier,
    STATE(76), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2189] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(245), 1,
      anon_sym_on,
    ACTIONS(247), 1,
      sym_identifier,
    ACTIONS(249), 1,
      anon_sym_RBRACE,
    STATE(76), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2207] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(251), 1,
      anon_sym_RBRACE,
    ACTIONS(253), 1,
      anon_sym_on,
    ACTIONS(256), 1,
      sym_identifier,
    STATE(76), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2225] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(226), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2237] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(245), 1,
      anon_sym_on,
    ACTIONS(247), 1,
      sym_identifier,
    ACTIONS(259), 1,
      anon_sym_RBRACE,
    STATE(75), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2255] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(261), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2267] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(245), 1,
      anon_sym_on,
    ACTIONS(247), 1,
      sym_identifier,
    ACTIONS(263), 1,
      anon_sym_RBRACE,
    STATE(74), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2285] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(265), 1,
      anon_sym_RBRACE,
    ACTIONS(267), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2298] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(269), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2309] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(271), 1,
      anon_sym_RBRACE,
    ACTIONS(273), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2322] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 1,
      anon_sym_RBRACE,
    ACTIONS(277), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2335] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(279), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2346] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(281), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2357] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(283), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2368] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(285), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2379] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(287), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2390] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(289), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2401] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(291), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2412] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(293), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2423] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(295), 1,
      anon_sym_RBRACE,
    ACTIONS(297), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2436] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 1,
      anon_sym_RBRACE,
    ACTIONS(301), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2449] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(285), 1,
      anon_sym_RBRACE,
    ACTIONS(303), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2462] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 1,
      anon_sym_RBRACE,
    ACTIONS(307), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2475] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 1,
      anon_sym_RBRACE,
    ACTIONS(311), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2488] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(313), 1,
      anon_sym_RBRACE,
    ACTIONS(315), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2501] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(317), 1,
      anon_sym_RBRACE,
    ACTIONS(319), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2514] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(321), 1,
      anon_sym_RBRACE,
    ACTIONS(323), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2527] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(269), 1,
      anon_sym_RBRACE,
    ACTIONS(325), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2540] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(327), 1,
      anon_sym_RBRACE,
    ACTIONS(329), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2553] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(331), 1,
      anon_sym_RBRACE,
    ACTIONS(333), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2566] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(265), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2577] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(293), 1,
      anon_sym_RBRACE,
    ACTIONS(335), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2590] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(337), 1,
      anon_sym_RBRACE,
    ACTIONS(339), 1,
      sym_identifier,
    STATE(114), 1,
      sym_binding_pair,
  [2603] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 1,
      anon_sym_RBRACE,
    ACTIONS(311), 2,
      anon_sym_on,
      sym_identifier,
  [2614] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(341), 1,
      anon_sym_DASH_GT,
    ACTIONS(343), 1,
      anon_sym_LPAREN,
    STATE(180), 1,
      sym_event_param,
  [2627] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(345), 1,
      anon_sym_RBRACE,
    ACTIONS(347), 1,
      anon_sym_COMMA,
    STATE(111), 1,
      aux_sym_parameter_block_repeat1,
  [2640] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(349), 1,
      anon_sym_RBRACE,
    ACTIONS(351), 1,
      anon_sym_COMMA,
    STATE(121), 1,
      aux_sym_parameter_binding_repeat1,
  [2653] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(353), 1,
      anon_sym_RBRACE,
    ACTIONS(355), 1,
      anon_sym_COMMA,
    STATE(111), 1,
      aux_sym_parameter_block_repeat1,
  [2666] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      anon_sym_COMMA,
    ACTIONS(358), 1,
      anon_sym_RBRACE,
    STATE(109), 1,
      aux_sym_parameter_block_repeat1,
  [2679] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(360), 1,
      anon_sym_COMMA,
    ACTIONS(362), 1,
      anon_sym_RPAREN,
    STATE(133), 1,
      aux_sym_event_param_repeat1,
  [2692] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(351), 1,
      anon_sym_COMMA,
    ACTIONS(364), 1,
      anon_sym_RBRACE,
    STATE(110), 1,
      aux_sym_parameter_binding_repeat1,
  [2705] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(366), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2714] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 1,
      anon_sym_COMMA,
    ACTIONS(370), 1,
      anon_sym_RBRACK,
    STATE(117), 1,
      aux_sym_array_literal_repeat1,
  [2727] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(372), 1,
      anon_sym_COMMA,
    ACTIONS(375), 1,
      anon_sym_RBRACK,
    STATE(117), 1,
      aux_sym_array_literal_repeat1,
  [2740] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(377), 1,
      anon_sym_RBRACE,
    ACTIONS(379), 1,
      sym_identifier,
    STATE(112), 1,
      sym_parameter_decl,
  [2753] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(381), 1,
      anon_sym_COMMA,
    ACTIONS(383), 1,
      anon_sym_RPAREN,
    STATE(134), 1,
      aux_sym_call_expr_repeat1,
  [2766] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(385), 1,
      anon_sym_COMMA,
    ACTIONS(388), 1,
      anon_sym_RPAREN,
    STATE(120), 1,
      aux_sym_call_expr_repeat1,
  [2779] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(390), 1,
      anon_sym_RBRACE,
    ACTIONS(392), 1,
      anon_sym_COMMA,
    STATE(121), 1,
      aux_sym_parameter_binding_repeat1,
  [2792] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(395), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2801] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(397), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2810] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(313), 1,
      anon_sym_RBRACE,
    ACTIONS(315), 2,
      anon_sym_on,
      sym_identifier,
  [2821] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(271), 1,
      anon_sym_RBRACE,
    ACTIONS(273), 2,
      anon_sym_on,
      sym_identifier,
  [2832] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(343), 1,
      anon_sym_LPAREN,
    ACTIONS(399), 1,
      anon_sym_DASH_GT,
    STATE(216), 1,
      sym_event_param,
  [2845] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(379), 1,
      sym_identifier,
    ACTIONS(401), 1,
      anon_sym_RBRACE,
    STATE(130), 1,
      sym_parameter_decl,
  [2858] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 1,
      anon_sym_COMMA,
    ACTIONS(403), 1,
      anon_sym_RBRACK,
    STATE(116), 1,
      aux_sym_array_literal_repeat1,
  [2871] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(405), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2880] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      anon_sym_COMMA,
    ACTIONS(407), 1,
      anon_sym_RBRACE,
    STATE(132), 1,
      aux_sym_parameter_block_repeat1,
  [2893] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(360), 1,
      anon_sym_COMMA,
    ACTIONS(409), 1,
      anon_sym_RPAREN,
    STATE(113), 1,
      aux_sym_event_param_repeat1,
  [2906] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      anon_sym_COMMA,
    ACTIONS(411), 1,
      anon_sym_RBRACE,
    STATE(111), 1,
      aux_sym_parameter_block_repeat1,
  [2919] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(413), 1,
      anon_sym_COMMA,
    ACTIONS(416), 1,
      anon_sym_RPAREN,
    STATE(133), 1,
      aux_sym_event_param_repeat1,
  [2932] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(381), 1,
      anon_sym_COMMA,
    ACTIONS(418), 1,
      anon_sym_RPAREN,
    STATE(120), 1,
      aux_sym_call_expr_repeat1,
  [2945] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [2953] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(420), 1,
      anon_sym_LBRACE,
    STATE(59), 1,
      sym_parameter_block,
  [2963] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(422), 1,
      anon_sym_LBRACE,
    STATE(89), 1,
      sym_view_body,
  [2973] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      anon_sym_LBRACE,
    STATE(91), 1,
      sym_action_body,
  [2983] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(426), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [2991] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(428), 1,
      anon_sym_LBRACE,
    STATE(192), 1,
      sym_parameter_block,
  [3001] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(428), 1,
      anon_sym_LBRACE,
    STATE(163), 1,
      sym_parameter_block,
  [3011] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(353), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3019] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(430), 1,
      anon_sym_COMMA,
    ACTIONS(432), 1,
      anon_sym_RPAREN,
  [3029] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(434), 1,
      anon_sym_COMMA,
    ACTIONS(436), 1,
      anon_sym_RPAREN,
  [3039] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(438), 1,
      anon_sym_COMMA,
    ACTIONS(440), 1,
      anon_sym_RPAREN,
  [3049] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(416), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3057] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(442), 1,
      anon_sym_LBRACE,
    STATE(103), 1,
      sym_view_body,
  [3067] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(444), 1,
      anon_sym_LBRACE,
    STATE(102), 1,
      sym_component_body,
  [3077] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(295), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3085] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(375), 2,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3093] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(446), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3101] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(448), 2,
      anon_sym_DASH_GT,
      anon_sym_LPAREN,
  [3109] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(390), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3117] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(450), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3125] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(339), 1,
      sym_identifier,
    STATE(153), 1,
      sym_binding_pair,
  [3135] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(452), 1,
      anon_sym_LBRACE,
    STATE(173), 1,
      sym_parameter_binding,
  [3145] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(379), 1,
      sym_identifier,
    STATE(142), 1,
      sym_parameter_decl,
  [3155] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(388), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3163] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      anon_sym_LBRACE,
    STATE(171), 1,
      sym_action_body,
  [3173] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3181] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(452), 1,
      anon_sym_LBRACE,
    STATE(172), 1,
      sym_parameter_binding,
  [3191] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(454), 1,
      anon_sym_COLON,
  [3198] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(456), 1,
      anon_sym_output,
  [3205] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(458), 1,
      anon_sym_SEMI,
  [3212] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(460), 1,
      anon_sym_LPAREN,
  [3219] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(462), 1,
      anon_sym_SEMI,
  [3226] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(464), 1,
      anon_sym_SEMI,
  [3233] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(466), 1,
      anon_sym_SEMI,
  [3240] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(468), 1,
      anon_sym_LPAREN,
  [3247] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(470), 1,
      anon_sym_LPAREN,
  [3254] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(472), 1,
      anon_sym_RPAREN,
  [3261] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(474), 1,
      anon_sym_RPAREN,
  [3268] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(476), 1,
      anon_sym_RPAREN,
  [3275] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(478), 1,
      anon_sym_SEMI,
  [3282] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(480), 1,
      anon_sym_RPAREN,
  [3289] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(482), 1,
      sym_string,
  [3296] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(484), 1,
      anon_sym_SEMI,
  [3303] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(486), 1,
      anon_sym_SEMI,
  [3310] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(488), 1,
      anon_sym_SEMI,
  [3317] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(490), 1,
      anon_sym_DASH_GT,
  [3324] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(492), 1,
      anon_sym_RPAREN,
  [3331] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(494), 1,
      sym_string,
  [3338] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(496), 1,
      sym_identifier,
  [3345] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(498), 1,
      anon_sym_SEMI,
  [3352] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(500), 1,
      anon_sym_RPAREN,
  [3359] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(502), 1,
      anon_sym_RPAREN,
  [3366] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(504), 1,
      anon_sym_COLON,
  [3373] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(506), 1,
      anon_sym_RBRACE,
  [3380] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(508), 1,
      sym_identifier,
  [3387] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(510), 1,
      sym_identifier,
  [3394] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(512), 1,
      anon_sym_SEMI,
  [3401] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(514), 1,
      anon_sym_SEMI,
  [3408] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(516), 1,
      sym_string,
  [3415] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(518), 1,
      anon_sym_SEMI,
  [3422] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(520), 1,
      anon_sym_SEMI,
  [3429] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(522), 1,
      anon_sym_DASH_GT,
  [3436] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(524), 1,
      anon_sym_COLON,
  [3443] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(526), 1,
      sym_string,
  [3450] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(528), 1,
      anon_sym_DASH_GT,
  [3457] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(530), 1,
      sym_string,
  [3464] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(532), 1,
      sym_string,
  [3471] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(534), 1,
      sym_string,
  [3478] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(536), 1,
      anon_sym_input,
  [3485] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(538), 1,
      anon_sym_schema,
  [3492] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(540), 1,
      anon_sym_LBRACE,
  [3499] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(542), 1,
      anon_sym_SEMI,
  [3506] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(544), 1,
      sym_identifier,
  [3513] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(546), 1,
      sym_string,
  [3520] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(548), 1,
      anon_sym_SEMI,
  [3527] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(550), 1,
      anon_sym_SEMI,
  [3534] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(552), 1,
      anon_sym_LBRACE,
  [3541] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(554), 1,
      ts_builtin_sym_end,
  [3548] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(556), 1,
      sym_string,
  [3555] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(558), 1,
      anon_sym_COLON,
  [3562] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(560), 1,
      sym_string,
  [3569] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(562), 1,
      anon_sym_DASH_GT,
  [3576] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(564), 1,
      sym_string,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 58,
  [SMALL_STATE(4)] = 113,
  [SMALL_STATE(5)] = 168,
  [SMALL_STATE(6)] = 223,
  [SMALL_STATE(7)] = 257,
  [SMALL_STATE(8)] = 291,
  [SMALL_STATE(9)] = 325,
  [SMALL_STATE(10)] = 363,
  [SMALL_STATE(11)] = 397,
  [SMALL_STATE(12)] = 435,
  [SMALL_STATE(13)] = 473,
  [SMALL_STATE(14)] = 524,
  [SMALL_STATE(15)] = 553,
  [SMALL_STATE(16)] = 581,
  [SMALL_STATE(17)] = 609,
  [SMALL_STATE(18)] = 637,
  [SMALL_STATE(19)] = 665,
  [SMALL_STATE(20)] = 693,
  [SMALL_STATE(21)] = 741,
  [SMALL_STATE(22)] = 789,
  [SMALL_STATE(23)] = 837,
  [SMALL_STATE(24)] = 865,
  [SMALL_STATE(25)] = 893,
  [SMALL_STATE(26)] = 926,
  [SMALL_STATE(27)] = 959,
  [SMALL_STATE(28)] = 992,
  [SMALL_STATE(29)] = 1034,
  [SMALL_STATE(30)] = 1065,
  [SMALL_STATE(31)] = 1096,
  [SMALL_STATE(32)] = 1121,
  [SMALL_STATE(33)] = 1152,
  [SMALL_STATE(34)] = 1191,
  [SMALL_STATE(35)] = 1227,
  [SMALL_STATE(36)] = 1250,
  [SMALL_STATE(37)] = 1283,
  [SMALL_STATE(38)] = 1321,
  [SMALL_STATE(39)] = 1359,
  [SMALL_STATE(40)] = 1389,
  [SMALL_STATE(41)] = 1419,
  [SMALL_STATE(42)] = 1451,
  [SMALL_STATE(43)] = 1471,
  [SMALL_STATE(44)] = 1491,
  [SMALL_STATE(45)] = 1523,
  [SMALL_STATE(46)] = 1550,
  [SMALL_STATE(47)] = 1577,
  [SMALL_STATE(48)] = 1603,
  [SMALL_STATE(49)] = 1629,
  [SMALL_STATE(50)] = 1655,
  [SMALL_STATE(51)] = 1681,
  [SMALL_STATE(52)] = 1707,
  [SMALL_STATE(53)] = 1733,
  [SMALL_STATE(54)] = 1759,
  [SMALL_STATE(55)] = 1785,
  [SMALL_STATE(56)] = 1811,
  [SMALL_STATE(57)] = 1837,
  [SMALL_STATE(58)] = 1863,
  [SMALL_STATE(59)] = 1889,
  [SMALL_STATE(60)] = 1915,
  [SMALL_STATE(61)] = 1939,
  [SMALL_STATE(62)] = 1963,
  [SMALL_STATE(63)] = 1979,
  [SMALL_STATE(64)] = 1997,
  [SMALL_STATE(65)] = 2021,
  [SMALL_STATE(66)] = 2039,
  [SMALL_STATE(67)] = 2057,
  [SMALL_STATE(68)] = 2081,
  [SMALL_STATE(69)] = 2098,
  [SMALL_STATE(70)] = 2115,
  [SMALL_STATE(71)] = 2128,
  [SMALL_STATE(72)] = 2145,
  [SMALL_STATE(73)] = 2159,
  [SMALL_STATE(74)] = 2171,
  [SMALL_STATE(75)] = 2189,
  [SMALL_STATE(76)] = 2207,
  [SMALL_STATE(77)] = 2225,
  [SMALL_STATE(78)] = 2237,
  [SMALL_STATE(79)] = 2255,
  [SMALL_STATE(80)] = 2267,
  [SMALL_STATE(81)] = 2285,
  [SMALL_STATE(82)] = 2298,
  [SMALL_STATE(83)] = 2309,
  [SMALL_STATE(84)] = 2322,
  [SMALL_STATE(85)] = 2335,
  [SMALL_STATE(86)] = 2346,
  [SMALL_STATE(87)] = 2357,
  [SMALL_STATE(88)] = 2368,
  [SMALL_STATE(89)] = 2379,
  [SMALL_STATE(90)] = 2390,
  [SMALL_STATE(91)] = 2401,
  [SMALL_STATE(92)] = 2412,
  [SMALL_STATE(93)] = 2423,
  [SMALL_STATE(94)] = 2436,
  [SMALL_STATE(95)] = 2449,
  [SMALL_STATE(96)] = 2462,
  [SMALL_STATE(97)] = 2475,
  [SMALL_STATE(98)] = 2488,
  [SMALL_STATE(99)] = 2501,
  [SMALL_STATE(100)] = 2514,
  [SMALL_STATE(101)] = 2527,
  [SMALL_STATE(102)] = 2540,
  [SMALL_STATE(103)] = 2553,
  [SMALL_STATE(104)] = 2566,
  [SMALL_STATE(105)] = 2577,
  [SMALL_STATE(106)] = 2590,
  [SMALL_STATE(107)] = 2603,
  [SMALL_STATE(108)] = 2614,
  [SMALL_STATE(109)] = 2627,
  [SMALL_STATE(110)] = 2640,
  [SMALL_STATE(111)] = 2653,
  [SMALL_STATE(112)] = 2666,
  [SMALL_STATE(113)] = 2679,
  [SMALL_STATE(114)] = 2692,
  [SMALL_STATE(115)] = 2705,
  [SMALL_STATE(116)] = 2714,
  [SMALL_STATE(117)] = 2727,
  [SMALL_STATE(118)] = 2740,
  [SMALL_STATE(119)] = 2753,
  [SMALL_STATE(120)] = 2766,
  [SMALL_STATE(121)] = 2779,
  [SMALL_STATE(122)] = 2792,
  [SMALL_STATE(123)] = 2801,
  [SMALL_STATE(124)] = 2810,
  [SMALL_STATE(125)] = 2821,
  [SMALL_STATE(126)] = 2832,
  [SMALL_STATE(127)] = 2845,
  [SMALL_STATE(128)] = 2858,
  [SMALL_STATE(129)] = 2871,
  [SMALL_STATE(130)] = 2880,
  [SMALL_STATE(131)] = 2893,
  [SMALL_STATE(132)] = 2906,
  [SMALL_STATE(133)] = 2919,
  [SMALL_STATE(134)] = 2932,
  [SMALL_STATE(135)] = 2945,
  [SMALL_STATE(136)] = 2953,
  [SMALL_STATE(137)] = 2963,
  [SMALL_STATE(138)] = 2973,
  [SMALL_STATE(139)] = 2983,
  [SMALL_STATE(140)] = 2991,
  [SMALL_STATE(141)] = 3001,
  [SMALL_STATE(142)] = 3011,
  [SMALL_STATE(143)] = 3019,
  [SMALL_STATE(144)] = 3029,
  [SMALL_STATE(145)] = 3039,
  [SMALL_STATE(146)] = 3049,
  [SMALL_STATE(147)] = 3057,
  [SMALL_STATE(148)] = 3067,
  [SMALL_STATE(149)] = 3077,
  [SMALL_STATE(150)] = 3085,
  [SMALL_STATE(151)] = 3093,
  [SMALL_STATE(152)] = 3101,
  [SMALL_STATE(153)] = 3109,
  [SMALL_STATE(154)] = 3117,
  [SMALL_STATE(155)] = 3125,
  [SMALL_STATE(156)] = 3135,
  [SMALL_STATE(157)] = 3145,
  [SMALL_STATE(158)] = 3155,
  [SMALL_STATE(159)] = 3163,
  [SMALL_STATE(160)] = 3173,
  [SMALL_STATE(161)] = 3181,
  [SMALL_STATE(162)] = 3191,
  [SMALL_STATE(163)] = 3198,
  [SMALL_STATE(164)] = 3205,
  [SMALL_STATE(165)] = 3212,
  [SMALL_STATE(166)] = 3219,
  [SMALL_STATE(167)] = 3226,
  [SMALL_STATE(168)] = 3233,
  [SMALL_STATE(169)] = 3240,
  [SMALL_STATE(170)] = 3247,
  [SMALL_STATE(171)] = 3254,
  [SMALL_STATE(172)] = 3261,
  [SMALL_STATE(173)] = 3268,
  [SMALL_STATE(174)] = 3275,
  [SMALL_STATE(175)] = 3282,
  [SMALL_STATE(176)] = 3289,
  [SMALL_STATE(177)] = 3296,
  [SMALL_STATE(178)] = 3303,
  [SMALL_STATE(179)] = 3310,
  [SMALL_STATE(180)] = 3317,
  [SMALL_STATE(181)] = 3324,
  [SMALL_STATE(182)] = 3331,
  [SMALL_STATE(183)] = 3338,
  [SMALL_STATE(184)] = 3345,
  [SMALL_STATE(185)] = 3352,
  [SMALL_STATE(186)] = 3359,
  [SMALL_STATE(187)] = 3366,
  [SMALL_STATE(188)] = 3373,
  [SMALL_STATE(189)] = 3380,
  [SMALL_STATE(190)] = 3387,
  [SMALL_STATE(191)] = 3394,
  [SMALL_STATE(192)] = 3401,
  [SMALL_STATE(193)] = 3408,
  [SMALL_STATE(194)] = 3415,
  [SMALL_STATE(195)] = 3422,
  [SMALL_STATE(196)] = 3429,
  [SMALL_STATE(197)] = 3436,
  [SMALL_STATE(198)] = 3443,
  [SMALL_STATE(199)] = 3450,
  [SMALL_STATE(200)] = 3457,
  [SMALL_STATE(201)] = 3464,
  [SMALL_STATE(202)] = 3471,
  [SMALL_STATE(203)] = 3478,
  [SMALL_STATE(204)] = 3485,
  [SMALL_STATE(205)] = 3492,
  [SMALL_STATE(206)] = 3499,
  [SMALL_STATE(207)] = 3506,
  [SMALL_STATE(208)] = 3513,
  [SMALL_STATE(209)] = 3520,
  [SMALL_STATE(210)] = 3527,
  [SMALL_STATE(211)] = 3534,
  [SMALL_STATE(212)] = 3541,
  [SMALL_STATE(213)] = 3548,
  [SMALL_STATE(214)] = 3555,
  [SMALL_STATE(215)] = 3562,
  [SMALL_STATE(216)] = 3569,
  [SMALL_STATE(217)] = 3576,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(193),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(217),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(215),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(213),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [21] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [23] = {.entry = {.count = 1, .reusable = false}}, SHIFT(10),
  [25] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [27] = {.entry = {.count = 1, .reusable = false}}, SHIFT(16),
  [29] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 3),
  [31] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 3),
  [33] = {.entry = {.count = 1, .reusable = true}}, SHIFT(189),
  [35] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 4),
  [37] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 4),
  [39] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [41] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [43] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2), SHIFT_REPEAT(189),
  [46] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 1),
  [48] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 1),
  [50] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [52] = {.entry = {.count = 1, .reusable = false}}, SHIFT(40),
  [54] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__primary, 1),
  [56] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [58] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__primary, 1),
  [60] = {.entry = {.count = 1, .reusable = true}}, SHIFT(190),
  [62] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [64] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [66] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(40),
  [69] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(40),
  [72] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 2),
  [74] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 2),
  [76] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [78] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_group_expr, 3),
  [80] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_group_expr, 3),
  [82] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_boolean, 1),
  [84] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_boolean, 1),
  [86] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 3),
  [88] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 3),
  [90] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 5),
  [92] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 5),
  [94] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__unary, 2),
  [96] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__unary, 2),
  [98] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 4),
  [100] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 4),
  [102] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 2),
  [104] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 2),
  [106] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [108] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 1),
  [110] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 1),
  [112] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2),
  [114] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__addition_repeat1, 2),
  [116] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2), SHIFT_REPEAT(36),
  [119] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 1),
  [121] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [123] = {.entry = {.count = 1, .reusable = false}}, SHIFT(34),
  [125] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 2),
  [127] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [129] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(34),
  [132] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(34),
  [135] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [137] = {.entry = {.count = 1, .reusable = true}}, SHIFT(105),
  [139] = {.entry = {.count = 1, .reusable = false}}, SHIFT(201),
  [141] = {.entry = {.count = 1, .reusable = false}}, SHIFT(200),
  [143] = {.entry = {.count = 1, .reusable = false}}, SHIFT(140),
  [145] = {.entry = {.count = 1, .reusable = false}}, SHIFT(198),
  [147] = {.entry = {.count = 1, .reusable = false}}, SHIFT(43),
  [149] = {.entry = {.count = 1, .reusable = false}}, SHIFT(197),
  [151] = {.entry = {.count = 1, .reusable = true}}, SHIFT(92),
  [153] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [155] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [157] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [159] = {.entry = {.count = 1, .reusable = false}}, SHIFT(152),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(101),
  [163] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1),
  [165] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2),
  [167] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(193),
  [170] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(217),
  [173] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(215),
  [176] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(213),
  [179] = {.entry = {.count = 1, .reusable = true}}, SHIFT(81),
  [181] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [183] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
  [185] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_module_declaration_repeat1, 2),
  [187] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(201),
  [190] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(200),
  [193] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(43),
  [196] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(197),
  [199] = {.entry = {.count = 1, .reusable = true}}, SHIFT(88),
  [201] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [203] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(170),
  [207] = {.entry = {.count = 1, .reusable = true}}, SHIFT(169),
  [209] = {.entry = {.count = 1, .reusable = true}}, SHIFT(165),
  [211] = {.entry = {.count = 1, .reusable = true}}, SHIFT(164),
  [213] = {.entry = {.count = 1, .reusable = false}}, SHIFT(151),
  [215] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 2),
  [217] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [219] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 1),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2),
  [223] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2), SHIFT_REPEAT(33),
  [226] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2),
  [228] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2), SHIFT_REPEAT(28),
  [231] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 1),
  [233] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [235] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 2),
  [237] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_params_block, 3),
  [239] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_params_block, 3),
  [241] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 2),
  [243] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [245] = {.entry = {.count = 1, .reusable = false}}, SHIFT(42),
  [247] = {.entry = {.count = 1, .reusable = false}}, SHIFT(214),
  [249] = {.entry = {.count = 1, .reusable = true}}, SHIFT(100),
  [251] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_component_body_repeat1, 2),
  [253] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(42),
  [256] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(214),
  [259] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [261] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 3),
  [263] = {.entry = {.count = 1, .reusable = true}}, SHIFT(73),
  [265] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 5),
  [267] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 5),
  [269] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 3),
  [271] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [273] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [275] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 2),
  [277] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 2),
  [279] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 9),
  [281] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expression, 1),
  [283] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 8),
  [285] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 4),
  [287] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_declaration, 3),
  [289] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_domain_declaration, 7),
  [291] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_declaration, 3),
  [293] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 2),
  [295] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 4),
  [297] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 4),
  [299] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 3),
  [301] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 3),
  [303] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 4),
  [305] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 2),
  [307] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 2),
  [309] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [311] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [313] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [315] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [317] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_declaration, 3),
  [319] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_declaration, 3),
  [321] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 3),
  [323] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 3),
  [325] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 3),
  [327] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_declaration, 3),
  [329] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_declaration, 3),
  [331] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_container_declaration, 3),
  [333] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_container_declaration, 3),
  [335] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 2),
  [337] = {.entry = {.count = 1, .reusable = true}}, SHIFT(175),
  [339] = {.entry = {.count = 1, .reusable = true}}, SHIFT(162),
  [341] = {.entry = {.count = 1, .reusable = true}}, SHIFT(64),
  [343] = {.entry = {.count = 1, .reusable = true}}, SHIFT(183),
  [345] = {.entry = {.count = 1, .reusable = true}}, SHIFT(149),
  [347] = {.entry = {.count = 1, .reusable = true}}, SHIFT(157),
  [349] = {.entry = {.count = 1, .reusable = true}}, SHIFT(186),
  [351] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [353] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2),
  [355] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2), SHIFT_REPEAT(157),
  [358] = {.entry = {.count = 1, .reusable = true}}, SHIFT(160),
  [360] = {.entry = {.count = 1, .reusable = true}}, SHIFT(207),
  [362] = {.entry = {.count = 1, .reusable = true}}, SHIFT(196),
  [364] = {.entry = {.count = 1, .reusable = true}}, SHIFT(181),
  [366] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 3),
  [368] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [370] = {.entry = {.count = 1, .reusable = true}}, SHIFT(122),
  [372] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2), SHIFT_REPEAT(5),
  [375] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2),
  [377] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [379] = {.entry = {.count = 1, .reusable = true}}, SHIFT(187),
  [381] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [383] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [385] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2), SHIFT_REPEAT(22),
  [388] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2),
  [390] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2),
  [392] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2), SHIFT_REPEAT(155),
  [395] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 4),
  [397] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_value_expression, 1),
  [399] = {.entry = {.count = 1, .reusable = true}}, SHIFT(67),
  [401] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [403] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [405] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 2),
  [407] = {.entry = {.count = 1, .reusable = true}}, SHIFT(94),
  [409] = {.entry = {.count = 1, .reusable = true}}, SHIFT(199),
  [411] = {.entry = {.count = 1, .reusable = true}}, SHIFT(93),
  [413] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2), SHIFT_REPEAT(207),
  [416] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2),
  [418] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [420] = {.entry = {.count = 1, .reusable = true}}, SHIFT(127),
  [422] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [424] = {.entry = {.count = 1, .reusable = true}}, SHIFT(80),
  [426] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_decl, 3, .production_id = 2),
  [428] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [430] = {.entry = {.count = 1, .reusable = true}}, SHIFT(159),
  [432] = {.entry = {.count = 1, .reusable = true}}, SHIFT(166),
  [434] = {.entry = {.count = 1, .reusable = true}}, SHIFT(161),
  [436] = {.entry = {.count = 1, .reusable = true}}, SHIFT(167),
  [438] = {.entry = {.count = 1, .reusable = true}}, SHIFT(156),
  [440] = {.entry = {.count = 1, .reusable = true}}, SHIFT(168),
  [442] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [444] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [446] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_type_ref, 1),
  [448] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_type, 1),
  [450] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binding_pair, 3, .production_id = 1),
  [452] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [454] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [456] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [458] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_action, 1),
  [460] = {.entry = {.count = 1, .reusable = true}}, SHIFT(208),
  [462] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 4),
  [464] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 4),
  [466] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 4),
  [468] = {.entry = {.count = 1, .reusable = true}}, SHIFT(182),
  [470] = {.entry = {.count = 1, .reusable = true}}, SHIFT(176),
  [472] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [474] = {.entry = {.count = 1, .reusable = true}}, SHIFT(178),
  [476] = {.entry = {.count = 1, .reusable = true}}, SHIFT(179),
  [478] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 6),
  [480] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 2),
  [482] = {.entry = {.count = 1, .reusable = true}}, SHIFT(143),
  [484] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [486] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 6),
  [488] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 6),
  [490] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [492] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 3),
  [494] = {.entry = {.count = 1, .reusable = true}}, SHIFT(144),
  [496] = {.entry = {.count = 1, .reusable = true}}, SHIFT(131),
  [498] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [500] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [502] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 4),
  [504] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [506] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [508] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [510] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [512] = {.entry = {.count = 1, .reusable = true}}, SHIFT(99),
  [514] = {.entry = {.count = 1, .reusable = true}}, SHIFT(72),
  [516] = {.entry = {.count = 1, .reusable = true}}, SHIFT(211),
  [518] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [520] = {.entry = {.count = 1, .reusable = true}}, SHIFT(188),
  [522] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 4),
  [524] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [526] = {.entry = {.count = 1, .reusable = true}}, SHIFT(191),
  [528] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 3),
  [530] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [532] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [534] = {.entry = {.count = 1, .reusable = true}}, SHIFT(195),
  [536] = {.entry = {.count = 1, .reusable = true}}, SHIFT(141),
  [538] = {.entry = {.count = 1, .reusable = true}}, SHIFT(202),
  [540] = {.entry = {.count = 1, .reusable = true}}, SHIFT(203),
  [542] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [544] = {.entry = {.count = 1, .reusable = true}}, SHIFT(146),
  [546] = {.entry = {.count = 1, .reusable = true}}, SHIFT(145),
  [548] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [550] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [552] = {.entry = {.count = 1, .reusable = true}}, SHIFT(204),
  [554] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [556] = {.entry = {.count = 1, .reusable = true}}, SHIFT(205),
  [558] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [560] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [562] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [564] = {.entry = {.count = 1, .reusable = true}}, SHIFT(137),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef _WIN32
#define extern __declspec(dllexport)
#endif

extern const TSLanguage *tree_sitter_ifml(void) {
  static const TSLanguage language = {
    .version = LANGUAGE_VERSION,
    .symbol_count = SYMBOL_COUNT,
    .alias_count = ALIAS_COUNT,
    .token_count = TOKEN_COUNT,
    .external_token_count = EXTERNAL_TOKEN_COUNT,
    .state_count = STATE_COUNT,
    .large_state_count = LARGE_STATE_COUNT,
    .production_id_count = PRODUCTION_ID_COUNT,
    .field_count = FIELD_COUNT,
    .max_alias_sequence_length = MAX_ALIAS_SEQUENCE_LENGTH,
    .parse_table = &ts_parse_table[0][0],
    .small_parse_table = ts_small_parse_table,
    .small_parse_table_map = ts_small_parse_table_map,
    .parse_actions = ts_parse_actions,
    .symbol_names = ts_symbol_names,
    .field_names = ts_field_names,
    .field_map_slices = ts_field_map_slices,
    .field_map_entries = ts_field_map_entries,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
