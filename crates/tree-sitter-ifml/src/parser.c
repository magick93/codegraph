#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 235
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 126
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
  sym_object_literal = 83,
  sym_object_member = 84,
  sym_object_member_value = 85,
  sym_event_handler = 86,
  sym_event_type = 87,
  sym_event_param = 88,
  sym_event_action = 89,
  sym_navigate_action = 90,
  sym_refresh_action = 91,
  sym_action_invocation = 92,
  sym_parameter_binding = 93,
  sym_binding_pair = 94,
  sym_type_ref = 95,
  sym_expression = 96,
  sym__logical_or = 97,
  sym__logical_and = 98,
  sym__comparison = 99,
  sym__comparison_op = 100,
  sym__addition = 101,
  sym__add_op = 102,
  sym__multiplication = 103,
  sym__mul_op = 104,
  sym__unary = 105,
  sym__primary = 106,
  sym_call_expr = 107,
  sym_field_expr = 108,
  sym_group_expr = 109,
  sym_boolean = 110,
  aux_sym_source_file_repeat1 = 111,
  aux_sym_module_declaration_repeat1 = 112,
  aux_sym_component_body_repeat1 = 113,
  aux_sym_parameter_block_repeat1 = 114,
  aux_sym_array_literal_repeat1 = 115,
  aux_sym_object_literal_repeat1 = 116,
  aux_sym_event_param_repeat1 = 117,
  aux_sym_parameter_binding_repeat1 = 118,
  aux_sym__logical_or_repeat1 = 119,
  aux_sym__logical_and_repeat1 = 120,
  aux_sym__comparison_repeat1 = 121,
  aux_sym__addition_repeat1 = 122,
  aux_sym__multiplication_repeat1 = 123,
  aux_sym_call_expr_repeat1 = 124,
  aux_sym_field_expr_repeat1 = 125,
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
  [sym_object_literal] = "object_literal",
  [sym_object_member] = "object_member",
  [sym_object_member_value] = "object_member_value",
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
  [aux_sym_object_literal_repeat1] = "object_literal_repeat1",
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
  [sym_object_literal] = sym_object_literal,
  [sym_object_member] = sym_object_member,
  [sym_object_member_value] = sym_object_member_value,
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
  [aux_sym_object_literal_repeat1] = aux_sym_object_literal_repeat1,
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
  [sym_object_literal] = {
    .visible = true,
    .named = true,
  },
  [sym_object_member] = {
    .visible = true,
    .named = true,
  },
  [sym_object_member_value] = {
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
  [aux_sym_object_literal_repeat1] = {
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
  [38] = 38,
  [39] = 39,
  [40] = 40,
  [41] = 38,
  [42] = 42,
  [43] = 43,
  [44] = 43,
  [45] = 42,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 50,
  [51] = 51,
  [52] = 50,
  [53] = 49,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 57,
  [58] = 48,
  [59] = 57,
  [60] = 55,
  [61] = 61,
  [62] = 62,
  [63] = 63,
  [64] = 63,
  [65] = 61,
  [66] = 66,
  [67] = 67,
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
  [94] = 93,
  [95] = 95,
  [96] = 96,
  [97] = 85,
  [98] = 98,
  [99] = 99,
  [100] = 95,
  [101] = 101,
  [102] = 102,
  [103] = 89,
  [104] = 104,
  [105] = 105,
  [106] = 106,
  [107] = 107,
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
  [124] = 124,
  [125] = 125,
  [126] = 126,
  [127] = 127,
  [128] = 128,
  [129] = 114,
  [130] = 130,
  [131] = 106,
  [132] = 132,
  [133] = 133,
  [134] = 134,
  [135] = 135,
  [136] = 136,
  [137] = 137,
  [138] = 125,
  [139] = 139,
  [140] = 102,
  [141] = 133,
  [142] = 84,
  [143] = 143,
  [144] = 144,
  [145] = 137,
  [146] = 146,
  [147] = 147,
  [148] = 91,
  [149] = 149,
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
  [160] = 160,
  [161] = 161,
  [162] = 162,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 90,
  [167] = 167,
  [168] = 168,
  [169] = 169,
  [170] = 170,
  [171] = 171,
  [172] = 88,
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
  [206] = 206,
  [207] = 207,
  [208] = 208,
  [209] = 209,
  [210] = 210,
  [211] = 211,
  [212] = 212,
  [213] = 213,
  [214] = 214,
  [215] = 215,
  [216] = 216,
  [217] = 217,
  [218] = 218,
  [219] = 219,
  [220] = 220,
  [221] = 221,
  [222] = 222,
  [223] = 188,
  [224] = 224,
  [225] = 225,
  [226] = 206,
  [227] = 216,
  [228] = 228,
  [229] = 229,
  [230] = 230,
  [231] = 214,
  [232] = 232,
  [233] = 194,
  [234] = 234,
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
      if (lookahead == '{') ADVANCE(70);
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
      if (lookahead == ';') ADVANCE(73);
      if (lookahead == '}') ADVANCE(74);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(6)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 7:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'B') ADVANCE(245);
      if (lookahead == 'D') ADVANCE(144);
      if (lookahead == 'F') ADVANCE(216);
      if (lookahead == 'I') ADVANCE(237);
      if (lookahead == 'S') ADVANCE(270);
      if (lookahead == 'U') ADVANCE(277);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 8:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'b') ADVANCE(145);
      if (lookahead == 'c') ADVANCE(150);
      if (lookahead == 'd') ADVANCE(182);
      if (lookahead == 'l') ADVANCE(247);
      if (lookahead == 's') ADVANCE(147);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(286);
      END_STATE();
    case 9:
      if (lookahead == '/') ADVANCE(5);
      if (lookahead == 'c') ADVANCE(244);
      if (lookahead == 'l') ADVANCE(140);
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
      if (lookahead == 'c') ADVANCE(244);
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
      if (lookahead == 'o') ADVANCE(231);
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
  [6] = {.lex_state = 1},
  [7] = {.lex_state = 66},
  [8] = {.lex_state = 66},
  [9] = {.lex_state = 66},
  [10] = {.lex_state = 66},
  [11] = {.lex_state = 66},
  [12] = {.lex_state = 66},
  [13] = {.lex_state = 66},
  [14] = {.lex_state = 1},
  [15] = {.lex_state = 66},
  [16] = {.lex_state = 66},
  [17] = {.lex_state = 66},
  [18] = {.lex_state = 66},
  [19] = {.lex_state = 1},
  [20] = {.lex_state = 66},
  [21] = {.lex_state = 1},
  [22] = {.lex_state = 66},
  [23] = {.lex_state = 66},
  [24] = {.lex_state = 66},
  [25] = {.lex_state = 1},
  [26] = {.lex_state = 66},
  [27] = {.lex_state = 66},
  [28] = {.lex_state = 66},
  [29] = {.lex_state = 1},
  [30] = {.lex_state = 1},
  [31] = {.lex_state = 66},
  [32] = {.lex_state = 66},
  [33] = {.lex_state = 66},
  [34] = {.lex_state = 66},
  [35] = {.lex_state = 1},
  [36] = {.lex_state = 66},
  [37] = {.lex_state = 1},
  [38] = {.lex_state = 4},
  [39] = {.lex_state = 1},
  [40] = {.lex_state = 1},
  [41] = {.lex_state = 4},
  [42] = {.lex_state = 8},
  [43] = {.lex_state = 9},
  [44] = {.lex_state = 9},
  [45] = {.lex_state = 8},
  [46] = {.lex_state = 66},
  [47] = {.lex_state = 66},
  [48] = {.lex_state = 10},
  [49] = {.lex_state = 10},
  [50] = {.lex_state = 10},
  [51] = {.lex_state = 10},
  [52] = {.lex_state = 10},
  [53] = {.lex_state = 10},
  [54] = {.lex_state = 10},
  [55] = {.lex_state = 10},
  [56] = {.lex_state = 10},
  [57] = {.lex_state = 10},
  [58] = {.lex_state = 10},
  [59] = {.lex_state = 10},
  [60] = {.lex_state = 10},
  [61] = {.lex_state = 66},
  [62] = {.lex_state = 7},
  [63] = {.lex_state = 66},
  [64] = {.lex_state = 66},
  [65] = {.lex_state = 66},
  [66] = {.lex_state = 0},
  [67] = {.lex_state = 0},
  [68] = {.lex_state = 0},
  [69] = {.lex_state = 0},
  [70] = {.lex_state = 0},
  [71] = {.lex_state = 0},
  [72] = {.lex_state = 0},
  [73] = {.lex_state = 66},
  [74] = {.lex_state = 11},
  [75] = {.lex_state = 9},
  [76] = {.lex_state = 0},
  [77] = {.lex_state = 66},
  [78] = {.lex_state = 11},
  [79] = {.lex_state = 11},
  [80] = {.lex_state = 11},
  [81] = {.lex_state = 11},
  [82] = {.lex_state = 10},
  [83] = {.lex_state = 10},
  [84] = {.lex_state = 10},
  [85] = {.lex_state = 10},
  [86] = {.lex_state = 66},
  [87] = {.lex_state = 66},
  [88] = {.lex_state = 10},
  [89] = {.lex_state = 10},
  [90] = {.lex_state = 10},
  [91] = {.lex_state = 10},
  [92] = {.lex_state = 66},
  [93] = {.lex_state = 10},
  [94] = {.lex_state = 66},
  [95] = {.lex_state = 10},
  [96] = {.lex_state = 66},
  [97] = {.lex_state = 66},
  [98] = {.lex_state = 66},
  [99] = {.lex_state = 10},
  [100] = {.lex_state = 66},
  [101] = {.lex_state = 10},
  [102] = {.lex_state = 10},
  [103] = {.lex_state = 66},
  [104] = {.lex_state = 0},
  [105] = {.lex_state = 10},
  [106] = {.lex_state = 10},
  [107] = {.lex_state = 0},
  [108] = {.lex_state = 0},
  [109] = {.lex_state = 6},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 0},
  [112] = {.lex_state = 0},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 6},
  [118] = {.lex_state = 0},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 0},
  [122] = {.lex_state = 0},
  [123] = {.lex_state = 6},
  [124] = {.lex_state = 0},
  [125] = {.lex_state = 6},
  [126] = {.lex_state = 0},
  [127] = {.lex_state = 0},
  [128] = {.lex_state = 6},
  [129] = {.lex_state = 0},
  [130] = {.lex_state = 0},
  [131] = {.lex_state = 11},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 4},
  [134] = {.lex_state = 0},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 0},
  [137] = {.lex_state = 0},
  [138] = {.lex_state = 6},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 11},
  [141] = {.lex_state = 4},
  [142] = {.lex_state = 11},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 0},
  [145] = {.lex_state = 0},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 66},
  [149] = {.lex_state = 0},
  [150] = {.lex_state = 0},
  [151] = {.lex_state = 0},
  [152] = {.lex_state = 0},
  [153] = {.lex_state = 0},
  [154] = {.lex_state = 0},
  [155] = {.lex_state = 0},
  [156] = {.lex_state = 0},
  [157] = {.lex_state = 0},
  [158] = {.lex_state = 0},
  [159] = {.lex_state = 0},
  [160] = {.lex_state = 0},
  [161] = {.lex_state = 0},
  [162] = {.lex_state = 0},
  [163] = {.lex_state = 4},
  [164] = {.lex_state = 0},
  [165] = {.lex_state = 0},
  [166] = {.lex_state = 66},
  [167] = {.lex_state = 0},
  [168] = {.lex_state = 0},
  [169] = {.lex_state = 6},
  [170] = {.lex_state = 0},
  [171] = {.lex_state = 6},
  [172] = {.lex_state = 66},
  [173] = {.lex_state = 6},
  [174] = {.lex_state = 0},
  [175] = {.lex_state = 0},
  [176] = {.lex_state = 0},
  [177] = {.lex_state = 0},
  [178] = {.lex_state = 66},
  [179] = {.lex_state = 6},
  [180] = {.lex_state = 0},
  [181] = {.lex_state = 0},
  [182] = {.lex_state = 0},
  [183] = {.lex_state = 0},
  [184] = {.lex_state = 0},
  [185] = {.lex_state = 0},
  [186] = {.lex_state = 0},
  [187] = {.lex_state = 0},
  [188] = {.lex_state = 0},
  [189] = {.lex_state = 0},
  [190] = {.lex_state = 0},
  [191] = {.lex_state = 0},
  [192] = {.lex_state = 0},
  [193] = {.lex_state = 0},
  [194] = {.lex_state = 4},
  [195] = {.lex_state = 0},
  [196] = {.lex_state = 0},
  [197] = {.lex_state = 6},
  [198] = {.lex_state = 0},
  [199] = {.lex_state = 6},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 0},
  [202] = {.lex_state = 0},
  [203] = {.lex_state = 0},
  [204] = {.lex_state = 0},
  [205] = {.lex_state = 0},
  [206] = {.lex_state = 0},
  [207] = {.lex_state = 6},
  [208] = {.lex_state = 0},
  [209] = {.lex_state = 0},
  [210] = {.lex_state = 0},
  [211] = {.lex_state = 0},
  [212] = {.lex_state = 0},
  [213] = {.lex_state = 0},
  [214] = {.lex_state = 0},
  [215] = {.lex_state = 0},
  [216] = {.lex_state = 0},
  [217] = {.lex_state = 0},
  [218] = {.lex_state = 0},
  [219] = {.lex_state = 0},
  [220] = {.lex_state = 66},
  [221] = {.lex_state = 66},
  [222] = {.lex_state = 0},
  [223] = {.lex_state = 0},
  [224] = {.lex_state = 4},
  [225] = {.lex_state = 4},
  [226] = {.lex_state = 0},
  [227] = {.lex_state = 0},
  [228] = {.lex_state = 0},
  [229] = {.lex_state = 0},
  [230] = {.lex_state = 0},
  [231] = {.lex_state = 0},
  [232] = {.lex_state = 0},
  [233] = {.lex_state = 4},
  [234] = {.lex_state = 0},
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
    [sym_source_file] = STATE(229),
    [sym__definition] = STATE(47),
    [sym_domain_declaration] = STATE(47),
    [sym_view_declaration] = STATE(47),
    [sym_action_declaration] = STATE(47),
    [sym_module_declaration] = STATE(47),
    [aux_sym_source_file_repeat1] = STATE(47),
    [ts_builtin_sym_end] = ACTIONS(5),
    [anon_sym_domain] = ACTIONS(7),
    [anon_sym_view] = ACTIONS(9),
    [anon_sym_action] = ACTIONS(11),
    [anon_sym_module] = ACTIONS(13),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 17,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACE,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(19), 1,
      anon_sym_RBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(111), 1,
      sym_value_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(122), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [62] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACE,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(188), 1,
      sym_value_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(122), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [121] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACE,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(223), 1,
      sym_value_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(122), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [180] = 16,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(15), 1,
      anon_sym_LBRACE,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(165), 1,
      sym_value_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(122), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [239] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(160), 1,
      sym_object_member_value,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(159), 2,
      sym_array_literal,
      sym_expression,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [294] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(38), 1,
      anon_sym_SLASH,
    STATE(7), 1,
      aux_sym__multiplication_repeat1,
    STATE(40), 1,
      sym__mul_op,
    ACTIONS(33), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(35), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(31), 15,
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
  [332] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(45), 1,
      anon_sym_DOT,
    STATE(8), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(43), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(41), 17,
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
  [366] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(50), 1,
      anon_sym_LPAREN,
    ACTIONS(54), 1,
      anon_sym_DOT,
    ACTIONS(52), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(48), 17,
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
  [400] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(60), 1,
      anon_sym_DOT,
    STATE(13), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(58), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(56), 17,
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
  [434] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(68), 1,
      anon_sym_SLASH,
    STATE(12), 1,
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
  [472] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(68), 1,
      anon_sym_SLASH,
    STATE(7), 1,
      aux_sym__multiplication_repeat1,
    STATE(40), 1,
      sym__mul_op,
    ACTIONS(66), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(72), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(70), 15,
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
  [510] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(60), 1,
      anon_sym_DOT,
    STATE(8), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(76), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(74), 17,
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
  [544] = 14,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    ACTIONS(78), 1,
      anon_sym_RPAREN,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(130), 1,
      sym_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [595] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(43), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(41), 18,
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
  [624] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(82), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(80), 17,
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
  [652] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(31), 17,
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
  [680] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(86), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(84), 17,
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
  [708] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(175), 1,
      sym_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [756] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(90), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(88), 17,
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
  [784] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(168), 1,
      sym_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [832] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(94), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(92), 17,
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
  [860] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(98), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(96), 17,
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
  [888] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(102), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(100), 17,
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
  [916] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(70), 1,
      sym__logical_and,
    STATE(104), 1,
      sym__logical_or,
    STATE(204), 1,
      sym_expression,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [964] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(28), 1,
      aux_sym__addition_repeat1,
    STATE(37), 1,
      sym__add_op,
    ACTIONS(106), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(108), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(104), 13,
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
  [997] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(26), 1,
      aux_sym__addition_repeat1,
    STATE(37), 1,
      sym__add_op,
    ACTIONS(108), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(112), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(110), 13,
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
  [1030] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(28), 1,
      aux_sym__addition_repeat1,
    STATE(37), 1,
      sym__add_op,
    ACTIONS(116), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(118), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(114), 13,
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
  [1063] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(68), 1,
      sym__comparison,
    STATE(76), 1,
      sym__logical_and,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1105] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(32), 1,
      sym__addition,
    STATE(72), 1,
      sym__comparison,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1144] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(33), 1,
      aux_sym__comparison_repeat1,
    STATE(35), 1,
      sym__comparison_op,
    ACTIONS(125), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(123), 6,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
    ACTIONS(121), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1175] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(31), 1,
      aux_sym__comparison_repeat1,
    STATE(35), 1,
      sym__comparison_op,
    ACTIONS(125), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(123), 6,
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
  [1206] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(33), 1,
      aux_sym__comparison_repeat1,
    STATE(35), 1,
      sym__comparison_op,
    ACTIONS(134), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(131), 6,
      anon_sym_EQ_EQ,
      anon_sym_BANG_EQ,
      anon_sym_LT_EQ,
      anon_sym_GT_EQ,
      anon_sym_TILDE_EQ,
      anon_sym_BANG_TILDE,
    ACTIONS(129), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1237] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(116), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(114), 15,
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
  [1262] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(27), 1,
      sym__multiplication,
    STATE(36), 1,
      sym__addition,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1298] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(137), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(129), 13,
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
  [1321] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(34), 1,
      sym__multiplication,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(27), 2,
      sym_string,
      sym_number,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    STATE(11), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1354] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(139), 1,
      anon_sym_RBRACE,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(145), 1,
      anon_sym_params,
    ACTIONS(147), 1,
      anon_sym_label,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    STATE(43), 1,
      sym_params_block,
    STATE(57), 1,
      sym_label_declaration,
    STATE(48), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1392] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(29), 2,
      anon_sym_true,
      anon_sym_false,
    ACTIONS(153), 2,
      sym_string,
      sym_number,
    STATE(18), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1422] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    ACTIONS(23), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(29), 2,
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
  [1452] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(145), 1,
      anon_sym_params,
    ACTIONS(147), 1,
      anon_sym_label,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(157), 1,
      anon_sym_RBRACE,
    STATE(44), 1,
      sym_params_block,
    STATE(59), 1,
      sym_label_declaration,
    STATE(58), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1490] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(133), 1,
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
  [1510] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_label,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(53), 1,
      sym_label_declaration,
    STATE(52), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1542] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(147), 1,
      anon_sym_label,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(163), 1,
      anon_sym_RBRACE,
    STATE(49), 1,
      sym_label_declaration,
    STATE(50), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1574] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(141), 1,
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
  [1594] = 7,
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
  [1621] = 7,
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
    ACTIONS(179), 1,
      ts_builtin_sym_end,
    STATE(46), 6,
      sym__definition,
      sym_domain_declaration,
      sym_view_declaration,
      sym_action_declaration,
      sym_module_declaration,
      aux_sym_source_file_repeat1,
  [1648] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1674] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(181), 1,
      anon_sym_RBRACE,
    STATE(55), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1700] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(181), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1726] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(183), 1,
      anon_sym_RBRACE,
    ACTIONS(185), 1,
      anon_sym_container,
    ACTIONS(188), 1,
      anon_sym_component,
    ACTIONS(191), 1,
      anon_sym_on,
    ACTIONS(194), 1,
      sym_identifier,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1752] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(197), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1778] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(197), 1,
      anon_sym_RBRACE,
    STATE(60), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1804] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(199), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1830] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(201), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1856] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(203), 1,
      anon_sym_RBRACE,
    STATE(54), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1882] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(52), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1908] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(163), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1934] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(163), 1,
      anon_sym_RBRACE,
    STATE(50), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1960] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(141), 1,
      anon_sym_container,
    ACTIONS(143), 1,
      anon_sym_component,
    ACTIONS(149), 1,
      anon_sym_on,
    ACTIONS(151), 1,
      sym_identifier,
    ACTIONS(205), 1,
      anon_sym_RBRACE,
    STATE(51), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1986] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(207), 1,
      anon_sym_action,
    ACTIONS(209), 1,
      anon_sym_navigate,
    ACTIONS(211), 1,
      anon_sym_refresh,
    ACTIONS(213), 1,
      sym_stay_statement,
    STATE(226), 1,
      sym_event_action,
    STATE(180), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2010] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(164), 1,
      sym_type_ref,
    ACTIONS(215), 7,
      anon_sym_Uuid,
      anon_sym_String,
      anon_sym_Int,
      anon_sym_Float,
      anon_sym_Boolean,
      anon_sym_DateTime,
      sym_identifier,
  [2026] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(207), 1,
      anon_sym_action,
    ACTIONS(209), 1,
      anon_sym_navigate,
    ACTIONS(211), 1,
      anon_sym_refresh,
    ACTIONS(213), 1,
      sym_stay_statement,
    STATE(216), 1,
      sym_event_action,
    STATE(180), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2050] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(207), 1,
      anon_sym_action,
    ACTIONS(209), 1,
      anon_sym_navigate,
    ACTIONS(211), 1,
      anon_sym_refresh,
    ACTIONS(213), 1,
      sym_stay_statement,
    STATE(227), 1,
      sym_event_action,
    STATE(180), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2074] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(207), 1,
      anon_sym_action,
    ACTIONS(209), 1,
      anon_sym_navigate,
    ACTIONS(211), 1,
      anon_sym_refresh,
    ACTIONS(213), 1,
      sym_stay_statement,
    STATE(206), 1,
      sym_event_action,
    STATE(180), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2098] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(219), 1,
      anon_sym_AMP_AMP,
    STATE(67), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(217), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2116] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(223), 1,
      anon_sym_AMP_AMP,
    STATE(67), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(221), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2134] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(219), 1,
      anon_sym_AMP_AMP,
    STATE(66), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(226), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2152] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(230), 1,
      anon_sym_PIPE_PIPE,
    STATE(69), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(228), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2169] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(235), 1,
      anon_sym_PIPE_PIPE,
    STATE(71), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(233), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2186] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(235), 1,
      anon_sym_PIPE_PIPE,
    STATE(69), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(237), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2203] = 2,
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
  [2216] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(239), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2228] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(241), 1,
      anon_sym_RBRACE,
    ACTIONS(243), 1,
      anon_sym_on,
    ACTIONS(245), 1,
      sym_identifier,
    STATE(79), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2246] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(247), 1,
      anon_sym_RBRACE,
    ACTIONS(249), 5,
      anon_sym_container,
      anon_sym_component,
      anon_sym_label,
      anon_sym_on,
      sym_identifier,
  [2260] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(228), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2272] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(251), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2284] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 1,
      anon_sym_on,
    ACTIONS(245), 1,
      sym_identifier,
    ACTIONS(253), 1,
      anon_sym_RBRACE,
    STATE(80), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2302] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 1,
      anon_sym_on,
    ACTIONS(245), 1,
      sym_identifier,
    ACTIONS(255), 1,
      anon_sym_RBRACE,
    STATE(81), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2320] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 1,
      anon_sym_on,
    ACTIONS(245), 1,
      sym_identifier,
    ACTIONS(257), 1,
      anon_sym_RBRACE,
    STATE(81), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2338] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(259), 1,
      anon_sym_RBRACE,
    ACTIONS(261), 1,
      anon_sym_on,
    ACTIONS(264), 1,
      sym_identifier,
    STATE(81), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [2356] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(267), 1,
      anon_sym_RBRACE,
    ACTIONS(269), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2369] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(271), 1,
      anon_sym_RBRACE,
    ACTIONS(273), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2382] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 1,
      anon_sym_RBRACE,
    ACTIONS(277), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2395] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(279), 1,
      anon_sym_RBRACE,
    ACTIONS(281), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2408] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(283), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2419] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(285), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2430] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(287), 1,
      anon_sym_RBRACE,
    ACTIONS(289), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2443] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(291), 1,
      anon_sym_RBRACE,
    ACTIONS(293), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2456] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(295), 1,
      anon_sym_RBRACE,
    ACTIONS(297), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2469] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 1,
      anon_sym_RBRACE,
    ACTIONS(301), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2482] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(303), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2493] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 1,
      anon_sym_RBRACE,
    ACTIONS(307), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2506] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2517] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 1,
      anon_sym_RBRACE,
    ACTIONS(311), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2530] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(313), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2541] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(279), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2552] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(315), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2563] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(317), 1,
      anon_sym_RBRACE,
    ACTIONS(319), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2576] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2587] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(321), 1,
      anon_sym_RBRACE,
    ACTIONS(323), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2600] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      anon_sym_RBRACE,
    ACTIONS(327), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2613] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(291), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2624] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(329), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2635] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(331), 1,
      anon_sym_RBRACE,
    ACTIONS(333), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2648] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(335), 1,
      anon_sym_RBRACE,
    ACTIONS(337), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2661] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(339), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2671] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(341), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2681] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(343), 1,
      anon_sym_SEMI,
    ACTIONS(345), 1,
      anon_sym_RBRACE,
    ACTIONS(347), 1,
      sym_identifier,
    STATE(132), 1,
      sym_object_member,
  [2697] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(349), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2707] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(351), 1,
      anon_sym_COMMA,
    ACTIONS(353), 1,
      anon_sym_RBRACK,
    STATE(127), 1,
      aux_sym_array_literal_repeat1,
  [2720] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(355), 1,
      anon_sym_RBRACE,
    ACTIONS(357), 1,
      anon_sym_COMMA,
    STATE(112), 1,
      aux_sym_parameter_binding_repeat1,
  [2733] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(360), 1,
      anon_sym_RBRACE,
    ACTIONS(362), 1,
      anon_sym_COMMA,
    STATE(116), 1,
      aux_sym_parameter_binding_repeat1,
  [2746] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(364), 1,
      anon_sym_RBRACE,
    ACTIONS(366), 1,
      anon_sym_COMMA,
    STATE(145), 1,
      aux_sym_parameter_block_repeat1,
  [2759] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 1,
      anon_sym_COMMA,
    ACTIONS(371), 1,
      anon_sym_RPAREN,
    STATE(115), 1,
      aux_sym_call_expr_repeat1,
  [2772] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(362), 1,
      anon_sym_COMMA,
    ACTIONS(373), 1,
      anon_sym_RBRACE,
    STATE(112), 1,
      aux_sym_parameter_binding_repeat1,
  [2785] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      sym_identifier,
    ACTIONS(375), 1,
      anon_sym_RBRACE,
    STATE(162), 1,
      sym_object_member,
  [2798] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(377), 1,
      anon_sym_RBRACE,
    ACTIONS(379), 1,
      anon_sym_COMMA,
    STATE(118), 1,
      aux_sym_parameter_block_repeat1,
  [2811] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(382), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2820] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(384), 1,
      anon_sym_COMMA,
    ACTIONS(386), 1,
      anon_sym_RPAREN,
    STATE(144), 1,
      aux_sym_event_param_repeat1,
  [2833] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(388), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2842] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(390), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [2851] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      sym_identifier,
    ACTIONS(392), 1,
      anon_sym_RBRACE,
    STATE(162), 1,
      sym_object_member,
  [2864] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(392), 1,
      anon_sym_RBRACE,
    ACTIONS(394), 1,
      anon_sym_SEMI,
    STATE(126), 1,
      aux_sym_object_literal_repeat1,
  [2877] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(396), 1,
      anon_sym_RBRACE,
    ACTIONS(398), 1,
      sym_identifier,
    STATE(129), 1,
      sym_parameter_decl,
  [2890] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(400), 1,
      anon_sym_SEMI,
    ACTIONS(403), 1,
      anon_sym_RBRACE,
    STATE(126), 1,
      aux_sym_object_literal_repeat1,
  [2903] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(351), 1,
      anon_sym_COMMA,
    ACTIONS(405), 1,
      anon_sym_RBRACK,
    STATE(134), 1,
      aux_sym_array_literal_repeat1,
  [2916] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(407), 1,
      anon_sym_RBRACE,
    ACTIONS(409), 1,
      sym_identifier,
    STATE(113), 1,
      sym_binding_pair,
  [2929] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(366), 1,
      anon_sym_COMMA,
    ACTIONS(411), 1,
      anon_sym_RBRACE,
    STATE(137), 1,
      aux_sym_parameter_block_repeat1,
  [2942] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(413), 1,
      anon_sym_COMMA,
    ACTIONS(415), 1,
      anon_sym_RPAREN,
    STATE(139), 1,
      aux_sym_call_expr_repeat1,
  [2955] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(335), 1,
      anon_sym_RBRACE,
    ACTIONS(337), 2,
      anon_sym_on,
      sym_identifier,
  [2966] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(417), 1,
      anon_sym_SEMI,
    ACTIONS(419), 1,
      anon_sym_RBRACE,
    STATE(124), 1,
      aux_sym_object_literal_repeat1,
  [2979] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(421), 1,
      anon_sym_DASH_GT,
    ACTIONS(423), 1,
      anon_sym_LPAREN,
    STATE(194), 1,
      sym_event_param,
  [2992] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(425), 1,
      anon_sym_COMMA,
    ACTIONS(428), 1,
      anon_sym_RBRACK,
    STATE(134), 1,
      aux_sym_array_literal_repeat1,
  [3005] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(430), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3014] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(384), 1,
      anon_sym_COMMA,
    ACTIONS(432), 1,
      anon_sym_RPAREN,
    STATE(120), 1,
      aux_sym_event_param_repeat1,
  [3027] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(366), 1,
      anon_sym_COMMA,
    ACTIONS(434), 1,
      anon_sym_RBRACE,
    STATE(118), 1,
      aux_sym_parameter_block_repeat1,
  [3040] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(398), 1,
      sym_identifier,
    ACTIONS(436), 1,
      anon_sym_RBRACE,
    STATE(114), 1,
      sym_parameter_decl,
  [3053] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(413), 1,
      anon_sym_COMMA,
    ACTIONS(438), 1,
      anon_sym_RPAREN,
    STATE(115), 1,
      aux_sym_call_expr_repeat1,
  [3066] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      anon_sym_RBRACE,
    ACTIONS(327), 2,
      anon_sym_on,
      sym_identifier,
  [3077] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(423), 1,
      anon_sym_LPAREN,
    ACTIONS(440), 1,
      anon_sym_DASH_GT,
    STATE(233), 1,
      sym_event_param,
  [3090] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 1,
      anon_sym_RBRACE,
    ACTIONS(277), 2,
      anon_sym_on,
      sym_identifier,
  [3101] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(442), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3110] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(444), 1,
      anon_sym_COMMA,
    ACTIONS(447), 1,
      anon_sym_RPAREN,
    STATE(144), 1,
      aux_sym_event_param_repeat1,
  [3123] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(366), 1,
      anon_sym_COMMA,
    ACTIONS(449), 1,
      anon_sym_RBRACE,
    STATE(118), 1,
      aux_sym_parameter_block_repeat1,
  [3136] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(451), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3144] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(453), 1,
      anon_sym_LBRACE,
    STATE(56), 1,
      sym_parameter_block,
  [3154] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3162] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(455), 1,
      anon_sym_LBRACE,
    STATE(96), 1,
      sym_view_body,
  [3172] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(457), 1,
      anon_sym_LBRACE,
    STATE(98), 1,
      sym_action_body,
  [3182] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(377), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3190] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(459), 1,
      anon_sym_COMMA,
    ACTIONS(461), 1,
      anon_sym_RPAREN,
  [3200] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(463), 1,
      anon_sym_COMMA,
    ACTIONS(465), 1,
      anon_sym_RPAREN,
  [3210] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(467), 1,
      anon_sym_COMMA,
    ACTIONS(469), 1,
      anon_sym_RPAREN,
  [3220] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(447), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3228] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(471), 1,
      anon_sym_LBRACE,
    STATE(209), 1,
      sym_parameter_block,
  [3238] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(471), 1,
      anon_sym_LBRACE,
    STATE(178), 1,
      sym_parameter_block,
  [3248] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(473), 1,
      anon_sym_LBRACE,
    STATE(101), 1,
      sym_view_body,
  [3258] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(475), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3266] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(477), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3274] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(479), 1,
      anon_sym_LBRACE,
    STATE(105), 1,
      sym_component_body,
  [3284] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(403), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3292] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(481), 2,
      anon_sym_DASH_GT,
      anon_sym_LPAREN,
  [3300] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(483), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3308] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(428), 2,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3316] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(295), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3324] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(355), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3332] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(485), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3340] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(409), 1,
      sym_identifier,
    STATE(167), 1,
      sym_binding_pair,
  [3350] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(487), 1,
      anon_sym_LBRACE,
    STATE(190), 1,
      sym_parameter_binding,
  [3360] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(398), 1,
      sym_identifier,
    STATE(151), 1,
      sym_parameter_decl,
  [3370] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(287), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3378] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(347), 1,
      sym_identifier,
    STATE(162), 1,
      sym_object_member,
  [3388] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(457), 1,
      anon_sym_LBRACE,
    STATE(187), 1,
      sym_action_body,
  [3398] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(371), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3406] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(487), 1,
      anon_sym_LBRACE,
    STATE(189), 1,
      sym_parameter_binding,
  [3416] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(489), 1,
      anon_sym_SEMI,
  [3423] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(491), 1,
      anon_sym_output,
  [3430] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(493), 1,
      sym_identifier,
  [3437] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(495), 1,
      anon_sym_SEMI,
  [3444] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(497), 1,
      anon_sym_LPAREN,
  [3451] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(499), 1,
      anon_sym_SEMI,
  [3458] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(501), 1,
      anon_sym_SEMI,
  [3465] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(503), 1,
      anon_sym_SEMI,
  [3472] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(505), 1,
      anon_sym_LPAREN,
  [3479] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(507), 1,
      anon_sym_LPAREN,
  [3486] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(509), 1,
      anon_sym_RPAREN,
  [3493] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(511), 1,
      anon_sym_SEMI,
  [3500] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(513), 1,
      anon_sym_RPAREN,
  [3507] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(515), 1,
      anon_sym_RPAREN,
  [3514] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(517), 1,
      sym_string,
  [3521] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(519), 1,
      anon_sym_RPAREN,
  [3528] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(521), 1,
      anon_sym_COLON,
  [3535] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(523), 1,
      anon_sym_DASH_GT,
  [3542] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(525), 1,
      anon_sym_SEMI,
  [3549] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(527), 1,
      anon_sym_SEMI,
  [3556] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(529), 1,
      sym_identifier,
  [3563] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(531), 1,
      anon_sym_RPAREN,
  [3570] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(533), 1,
      sym_identifier,
  [3577] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(535), 1,
      anon_sym_COLON,
  [3584] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(419), 1,
      anon_sym_RBRACE,
  [3591] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(537), 1,
      anon_sym_COLON,
  [3598] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(539), 1,
      anon_sym_RPAREN,
  [3605] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(541), 1,
      anon_sym_RPAREN,
  [3612] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(543), 1,
      anon_sym_RBRACE,
  [3619] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(545), 1,
      anon_sym_SEMI,
  [3626] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(547), 1,
      sym_identifier,
  [3633] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(549), 1,
      anon_sym_SEMI,
  [3640] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(551), 1,
      anon_sym_SEMI,
  [3647] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(553), 1,
      sym_string,
  [3654] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(555), 1,
      sym_string,
  [3661] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(557), 1,
      anon_sym_SEMI,
  [3668] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(559), 1,
      sym_string,
  [3675] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(561), 1,
      anon_sym_COLON,
  [3682] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(563), 1,
      sym_string,
  [3689] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(565), 1,
      anon_sym_SEMI,
  [3696] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(567), 1,
      sym_string,
  [3703] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(569), 1,
      sym_string,
  [3710] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(571), 1,
      sym_string,
  [3717] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(573), 1,
      anon_sym_input,
  [3724] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(575), 1,
      anon_sym_schema,
  [3731] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(577), 1,
      anon_sym_LBRACE,
  [3738] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(579), 1,
      anon_sym_SEMI,
  [3745] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(581), 1,
      anon_sym_DASH_GT,
  [3752] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(583), 1,
      anon_sym_DASH_GT,
  [3759] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(585), 1,
      anon_sym_SEMI,
  [3766] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(587), 1,
      anon_sym_SEMI,
  [3773] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(589), 1,
      anon_sym_LBRACE,
  [3780] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(591), 1,
      ts_builtin_sym_end,
  [3787] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(593), 1,
      sym_string,
  [3794] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(595), 1,
      anon_sym_COLON,
  [3801] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(597), 1,
      sym_string,
  [3808] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(599), 1,
      anon_sym_DASH_GT,
  [3815] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(601), 1,
      sym_string,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 62,
  [SMALL_STATE(4)] = 121,
  [SMALL_STATE(5)] = 180,
  [SMALL_STATE(6)] = 239,
  [SMALL_STATE(7)] = 294,
  [SMALL_STATE(8)] = 332,
  [SMALL_STATE(9)] = 366,
  [SMALL_STATE(10)] = 400,
  [SMALL_STATE(11)] = 434,
  [SMALL_STATE(12)] = 472,
  [SMALL_STATE(13)] = 510,
  [SMALL_STATE(14)] = 544,
  [SMALL_STATE(15)] = 595,
  [SMALL_STATE(16)] = 624,
  [SMALL_STATE(17)] = 652,
  [SMALL_STATE(18)] = 680,
  [SMALL_STATE(19)] = 708,
  [SMALL_STATE(20)] = 756,
  [SMALL_STATE(21)] = 784,
  [SMALL_STATE(22)] = 832,
  [SMALL_STATE(23)] = 860,
  [SMALL_STATE(24)] = 888,
  [SMALL_STATE(25)] = 916,
  [SMALL_STATE(26)] = 964,
  [SMALL_STATE(27)] = 997,
  [SMALL_STATE(28)] = 1030,
  [SMALL_STATE(29)] = 1063,
  [SMALL_STATE(30)] = 1105,
  [SMALL_STATE(31)] = 1144,
  [SMALL_STATE(32)] = 1175,
  [SMALL_STATE(33)] = 1206,
  [SMALL_STATE(34)] = 1237,
  [SMALL_STATE(35)] = 1262,
  [SMALL_STATE(36)] = 1298,
  [SMALL_STATE(37)] = 1321,
  [SMALL_STATE(38)] = 1354,
  [SMALL_STATE(39)] = 1392,
  [SMALL_STATE(40)] = 1422,
  [SMALL_STATE(41)] = 1452,
  [SMALL_STATE(42)] = 1490,
  [SMALL_STATE(43)] = 1510,
  [SMALL_STATE(44)] = 1542,
  [SMALL_STATE(45)] = 1574,
  [SMALL_STATE(46)] = 1594,
  [SMALL_STATE(47)] = 1621,
  [SMALL_STATE(48)] = 1648,
  [SMALL_STATE(49)] = 1674,
  [SMALL_STATE(50)] = 1700,
  [SMALL_STATE(51)] = 1726,
  [SMALL_STATE(52)] = 1752,
  [SMALL_STATE(53)] = 1778,
  [SMALL_STATE(54)] = 1804,
  [SMALL_STATE(55)] = 1830,
  [SMALL_STATE(56)] = 1856,
  [SMALL_STATE(57)] = 1882,
  [SMALL_STATE(58)] = 1908,
  [SMALL_STATE(59)] = 1934,
  [SMALL_STATE(60)] = 1960,
  [SMALL_STATE(61)] = 1986,
  [SMALL_STATE(62)] = 2010,
  [SMALL_STATE(63)] = 2026,
  [SMALL_STATE(64)] = 2050,
  [SMALL_STATE(65)] = 2074,
  [SMALL_STATE(66)] = 2098,
  [SMALL_STATE(67)] = 2116,
  [SMALL_STATE(68)] = 2134,
  [SMALL_STATE(69)] = 2152,
  [SMALL_STATE(70)] = 2169,
  [SMALL_STATE(71)] = 2186,
  [SMALL_STATE(72)] = 2203,
  [SMALL_STATE(73)] = 2216,
  [SMALL_STATE(74)] = 2228,
  [SMALL_STATE(75)] = 2246,
  [SMALL_STATE(76)] = 2260,
  [SMALL_STATE(77)] = 2272,
  [SMALL_STATE(78)] = 2284,
  [SMALL_STATE(79)] = 2302,
  [SMALL_STATE(80)] = 2320,
  [SMALL_STATE(81)] = 2338,
  [SMALL_STATE(82)] = 2356,
  [SMALL_STATE(83)] = 2369,
  [SMALL_STATE(84)] = 2382,
  [SMALL_STATE(85)] = 2395,
  [SMALL_STATE(86)] = 2408,
  [SMALL_STATE(87)] = 2419,
  [SMALL_STATE(88)] = 2430,
  [SMALL_STATE(89)] = 2443,
  [SMALL_STATE(90)] = 2456,
  [SMALL_STATE(91)] = 2469,
  [SMALL_STATE(92)] = 2482,
  [SMALL_STATE(93)] = 2493,
  [SMALL_STATE(94)] = 2506,
  [SMALL_STATE(95)] = 2517,
  [SMALL_STATE(96)] = 2530,
  [SMALL_STATE(97)] = 2541,
  [SMALL_STATE(98)] = 2552,
  [SMALL_STATE(99)] = 2563,
  [SMALL_STATE(100)] = 2576,
  [SMALL_STATE(101)] = 2587,
  [SMALL_STATE(102)] = 2600,
  [SMALL_STATE(103)] = 2613,
  [SMALL_STATE(104)] = 2624,
  [SMALL_STATE(105)] = 2635,
  [SMALL_STATE(106)] = 2648,
  [SMALL_STATE(107)] = 2661,
  [SMALL_STATE(108)] = 2671,
  [SMALL_STATE(109)] = 2681,
  [SMALL_STATE(110)] = 2697,
  [SMALL_STATE(111)] = 2707,
  [SMALL_STATE(112)] = 2720,
  [SMALL_STATE(113)] = 2733,
  [SMALL_STATE(114)] = 2746,
  [SMALL_STATE(115)] = 2759,
  [SMALL_STATE(116)] = 2772,
  [SMALL_STATE(117)] = 2785,
  [SMALL_STATE(118)] = 2798,
  [SMALL_STATE(119)] = 2811,
  [SMALL_STATE(120)] = 2820,
  [SMALL_STATE(121)] = 2833,
  [SMALL_STATE(122)] = 2842,
  [SMALL_STATE(123)] = 2851,
  [SMALL_STATE(124)] = 2864,
  [SMALL_STATE(125)] = 2877,
  [SMALL_STATE(126)] = 2890,
  [SMALL_STATE(127)] = 2903,
  [SMALL_STATE(128)] = 2916,
  [SMALL_STATE(129)] = 2929,
  [SMALL_STATE(130)] = 2942,
  [SMALL_STATE(131)] = 2955,
  [SMALL_STATE(132)] = 2966,
  [SMALL_STATE(133)] = 2979,
  [SMALL_STATE(134)] = 2992,
  [SMALL_STATE(135)] = 3005,
  [SMALL_STATE(136)] = 3014,
  [SMALL_STATE(137)] = 3027,
  [SMALL_STATE(138)] = 3040,
  [SMALL_STATE(139)] = 3053,
  [SMALL_STATE(140)] = 3066,
  [SMALL_STATE(141)] = 3077,
  [SMALL_STATE(142)] = 3090,
  [SMALL_STATE(143)] = 3101,
  [SMALL_STATE(144)] = 3110,
  [SMALL_STATE(145)] = 3123,
  [SMALL_STATE(146)] = 3136,
  [SMALL_STATE(147)] = 3144,
  [SMALL_STATE(148)] = 3154,
  [SMALL_STATE(149)] = 3162,
  [SMALL_STATE(150)] = 3172,
  [SMALL_STATE(151)] = 3182,
  [SMALL_STATE(152)] = 3190,
  [SMALL_STATE(153)] = 3200,
  [SMALL_STATE(154)] = 3210,
  [SMALL_STATE(155)] = 3220,
  [SMALL_STATE(156)] = 3228,
  [SMALL_STATE(157)] = 3238,
  [SMALL_STATE(158)] = 3248,
  [SMALL_STATE(159)] = 3258,
  [SMALL_STATE(160)] = 3266,
  [SMALL_STATE(161)] = 3274,
  [SMALL_STATE(162)] = 3284,
  [SMALL_STATE(163)] = 3292,
  [SMALL_STATE(164)] = 3300,
  [SMALL_STATE(165)] = 3308,
  [SMALL_STATE(166)] = 3316,
  [SMALL_STATE(167)] = 3324,
  [SMALL_STATE(168)] = 3332,
  [SMALL_STATE(169)] = 3340,
  [SMALL_STATE(170)] = 3350,
  [SMALL_STATE(171)] = 3360,
  [SMALL_STATE(172)] = 3370,
  [SMALL_STATE(173)] = 3378,
  [SMALL_STATE(174)] = 3388,
  [SMALL_STATE(175)] = 3398,
  [SMALL_STATE(176)] = 3406,
  [SMALL_STATE(177)] = 3416,
  [SMALL_STATE(178)] = 3423,
  [SMALL_STATE(179)] = 3430,
  [SMALL_STATE(180)] = 3437,
  [SMALL_STATE(181)] = 3444,
  [SMALL_STATE(182)] = 3451,
  [SMALL_STATE(183)] = 3458,
  [SMALL_STATE(184)] = 3465,
  [SMALL_STATE(185)] = 3472,
  [SMALL_STATE(186)] = 3479,
  [SMALL_STATE(187)] = 3486,
  [SMALL_STATE(188)] = 3493,
  [SMALL_STATE(189)] = 3500,
  [SMALL_STATE(190)] = 3507,
  [SMALL_STATE(191)] = 3514,
  [SMALL_STATE(192)] = 3521,
  [SMALL_STATE(193)] = 3528,
  [SMALL_STATE(194)] = 3535,
  [SMALL_STATE(195)] = 3542,
  [SMALL_STATE(196)] = 3549,
  [SMALL_STATE(197)] = 3556,
  [SMALL_STATE(198)] = 3563,
  [SMALL_STATE(199)] = 3570,
  [SMALL_STATE(200)] = 3577,
  [SMALL_STATE(201)] = 3584,
  [SMALL_STATE(202)] = 3591,
  [SMALL_STATE(203)] = 3598,
  [SMALL_STATE(204)] = 3605,
  [SMALL_STATE(205)] = 3612,
  [SMALL_STATE(206)] = 3619,
  [SMALL_STATE(207)] = 3626,
  [SMALL_STATE(208)] = 3633,
  [SMALL_STATE(209)] = 3640,
  [SMALL_STATE(210)] = 3647,
  [SMALL_STATE(211)] = 3654,
  [SMALL_STATE(212)] = 3661,
  [SMALL_STATE(213)] = 3668,
  [SMALL_STATE(214)] = 3675,
  [SMALL_STATE(215)] = 3682,
  [SMALL_STATE(216)] = 3689,
  [SMALL_STATE(217)] = 3696,
  [SMALL_STATE(218)] = 3703,
  [SMALL_STATE(219)] = 3710,
  [SMALL_STATE(220)] = 3717,
  [SMALL_STATE(221)] = 3724,
  [SMALL_STATE(222)] = 3731,
  [SMALL_STATE(223)] = 3738,
  [SMALL_STATE(224)] = 3745,
  [SMALL_STATE(225)] = 3752,
  [SMALL_STATE(226)] = 3759,
  [SMALL_STATE(227)] = 3766,
  [SMALL_STATE(228)] = 3773,
  [SMALL_STATE(229)] = 3780,
  [SMALL_STATE(230)] = 3787,
  [SMALL_STATE(231)] = 3794,
  [SMALL_STATE(232)] = 3801,
  [SMALL_STATE(233)] = 3808,
  [SMALL_STATE(234)] = 3815,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(210),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(234),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(232),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(230),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [21] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [23] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(9),
  [27] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [29] = {.entry = {.count = 1, .reusable = false}}, SHIFT(24),
  [31] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [33] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [35] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(40),
  [38] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(40),
  [41] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [43] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [45] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2), SHIFT_REPEAT(179),
  [48] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__primary, 1),
  [50] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [52] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__primary, 1),
  [54] = {.entry = {.count = 1, .reusable = true}}, SHIFT(207),
  [56] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 3),
  [58] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 3),
  [60] = {.entry = {.count = 1, .reusable = true}}, SHIFT(179),
  [62] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 1),
  [64] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 1),
  [66] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [68] = {.entry = {.count = 1, .reusable = false}}, SHIFT(40),
  [70] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 2),
  [72] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 2),
  [74] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 4),
  [76] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 4),
  [78] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [80] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 5),
  [82] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 5),
  [84] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__unary, 2),
  [86] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__unary, 2),
  [88] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 4),
  [90] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 4),
  [92] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_group_expr, 3),
  [94] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_group_expr, 3),
  [96] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 3),
  [98] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 3),
  [100] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_boolean, 1),
  [102] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_boolean, 1),
  [104] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 2),
  [106] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 2),
  [108] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [110] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 1),
  [112] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 1),
  [114] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2),
  [116] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__addition_repeat1, 2),
  [118] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2), SHIFT_REPEAT(37),
  [121] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 2),
  [123] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [125] = {.entry = {.count = 1, .reusable = false}}, SHIFT(35),
  [127] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 1),
  [129] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [131] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(35),
  [134] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(35),
  [137] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [139] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [141] = {.entry = {.count = 1, .reusable = false}}, SHIFT(218),
  [143] = {.entry = {.count = 1, .reusable = false}}, SHIFT(217),
  [145] = {.entry = {.count = 1, .reusable = false}}, SHIFT(156),
  [147] = {.entry = {.count = 1, .reusable = false}}, SHIFT(215),
  [149] = {.entry = {.count = 1, .reusable = false}}, SHIFT(42),
  [151] = {.entry = {.count = 1, .reusable = false}}, SHIFT(214),
  [153] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [155] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [157] = {.entry = {.count = 1, .reusable = true}}, SHIFT(100),
  [159] = {.entry = {.count = 1, .reusable = false}}, SHIFT(163),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(93),
  [163] = {.entry = {.count = 1, .reusable = true}}, SHIFT(94),
  [165] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2),
  [167] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(210),
  [170] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(234),
  [173] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(232),
  [176] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(230),
  [179] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1),
  [181] = {.entry = {.count = 1, .reusable = true}}, SHIFT(103),
  [183] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_module_declaration_repeat1, 2),
  [185] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(218),
  [188] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(217),
  [191] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(42),
  [194] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(214),
  [197] = {.entry = {.count = 1, .reusable = true}}, SHIFT(89),
  [199] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [201] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [203] = {.entry = {.count = 1, .reusable = true}}, SHIFT(86),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [207] = {.entry = {.count = 1, .reusable = true}}, SHIFT(186),
  [209] = {.entry = {.count = 1, .reusable = true}}, SHIFT(185),
  [211] = {.entry = {.count = 1, .reusable = true}}, SHIFT(181),
  [213] = {.entry = {.count = 1, .reusable = true}}, SHIFT(180),
  [215] = {.entry = {.count = 1, .reusable = false}}, SHIFT(146),
  [217] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 2),
  [219] = {.entry = {.count = 1, .reusable = true}}, SHIFT(30),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2),
  [223] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2), SHIFT_REPEAT(30),
  [226] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 1),
  [228] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2),
  [230] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2), SHIFT_REPEAT(29),
  [233] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 1),
  [235] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [237] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 2),
  [239] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 3),
  [241] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [243] = {.entry = {.count = 1, .reusable = false}}, SHIFT(45),
  [245] = {.entry = {.count = 1, .reusable = false}}, SHIFT(231),
  [247] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_params_block, 3),
  [249] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_params_block, 3),
  [251] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 2),
  [253] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [255] = {.entry = {.count = 1, .reusable = true}}, SHIFT(73),
  [257] = {.entry = {.count = 1, .reusable = true}}, SHIFT(99),
  [259] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_component_body_repeat1, 2),
  [261] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(45),
  [264] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(231),
  [267] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_declaration, 3),
  [269] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_declaration, 3),
  [271] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 2),
  [273] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 2),
  [275] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [277] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [279] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 5),
  [281] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 5),
  [283] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 8),
  [285] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 9),
  [287] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 3),
  [289] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 3),
  [291] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 4),
  [293] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 4),
  [295] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 2),
  [297] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 2),
  [299] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 4),
  [301] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 4),
  [303] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_domain_declaration, 7),
  [305] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 3),
  [307] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 3),
  [309] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 2),
  [311] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 2),
  [313] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_declaration, 3),
  [315] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_declaration, 3),
  [317] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 3),
  [319] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 3),
  [321] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_container_declaration, 3),
  [323] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_container_declaration, 3),
  [325] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [327] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [329] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expression, 1),
  [331] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_declaration, 3),
  [333] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_declaration, 3),
  [335] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [337] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [339] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 3),
  [341] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 4),
  [343] = {.entry = {.count = 1, .reusable = true}}, SHIFT(201),
  [345] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [347] = {.entry = {.count = 1, .reusable = true}}, SHIFT(202),
  [349] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 2),
  [351] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [353] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [355] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2),
  [357] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2), SHIFT_REPEAT(169),
  [360] = {.entry = {.count = 1, .reusable = true}}, SHIFT(198),
  [362] = {.entry = {.count = 1, .reusable = true}}, SHIFT(169),
  [364] = {.entry = {.count = 1, .reusable = true}}, SHIFT(172),
  [366] = {.entry = {.count = 1, .reusable = true}}, SHIFT(171),
  [368] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2), SHIFT_REPEAT(19),
  [371] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2),
  [373] = {.entry = {.count = 1, .reusable = true}}, SHIFT(203),
  [375] = {.entry = {.count = 1, .reusable = true}}, SHIFT(143),
  [377] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2),
  [379] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2), SHIFT_REPEAT(171),
  [382] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 4),
  [384] = {.entry = {.count = 1, .reusable = true}}, SHIFT(199),
  [386] = {.entry = {.count = 1, .reusable = true}}, SHIFT(224),
  [388] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 3),
  [390] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_value_expression, 1),
  [392] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [394] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [396] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [398] = {.entry = {.count = 1, .reusable = true}}, SHIFT(200),
  [400] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_literal_repeat1, 2), SHIFT_REPEAT(173),
  [403] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_object_literal_repeat1, 2),
  [405] = {.entry = {.count = 1, .reusable = true}}, SHIFT(108),
  [407] = {.entry = {.count = 1, .reusable = true}}, SHIFT(192),
  [409] = {.entry = {.count = 1, .reusable = true}}, SHIFT(193),
  [411] = {.entry = {.count = 1, .reusable = true}}, SHIFT(88),
  [413] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [415] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [417] = {.entry = {.count = 1, .reusable = true}}, SHIFT(123),
  [419] = {.entry = {.count = 1, .reusable = true}}, SHIFT(121),
  [421] = {.entry = {.count = 1, .reusable = true}}, SHIFT(65),
  [423] = {.entry = {.count = 1, .reusable = true}}, SHIFT(197),
  [425] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2), SHIFT_REPEAT(5),
  [428] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2),
  [430] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 2),
  [432] = {.entry = {.count = 1, .reusable = true}}, SHIFT(225),
  [434] = {.entry = {.count = 1, .reusable = true}}, SHIFT(91),
  [436] = {.entry = {.count = 1, .reusable = true}}, SHIFT(166),
  [438] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [440] = {.entry = {.count = 1, .reusable = true}}, SHIFT(61),
  [442] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 5),
  [444] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2), SHIFT_REPEAT(199),
  [447] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2),
  [449] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [451] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_type_ref, 1),
  [453] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [455] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
  [457] = {.entry = {.count = 1, .reusable = true}}, SHIFT(74),
  [459] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [461] = {.entry = {.count = 1, .reusable = true}}, SHIFT(182),
  [463] = {.entry = {.count = 1, .reusable = true}}, SHIFT(176),
  [465] = {.entry = {.count = 1, .reusable = true}}, SHIFT(183),
  [467] = {.entry = {.count = 1, .reusable = true}}, SHIFT(170),
  [469] = {.entry = {.count = 1, .reusable = true}}, SHIFT(184),
  [471] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [473] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [475] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_member_value, 1),
  [477] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_member, 3, .production_id = 1),
  [479] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [481] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_type, 1),
  [483] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_decl, 3, .production_id = 2),
  [485] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binding_pair, 3, .production_id = 1),
  [487] = {.entry = {.count = 1, .reusable = true}}, SHIFT(128),
  [489] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 6),
  [491] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [493] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [495] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_action, 1),
  [497] = {.entry = {.count = 1, .reusable = true}}, SHIFT(191),
  [499] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 4),
  [501] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 4),
  [503] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 4),
  [505] = {.entry = {.count = 1, .reusable = true}}, SHIFT(211),
  [507] = {.entry = {.count = 1, .reusable = true}}, SHIFT(213),
  [509] = {.entry = {.count = 1, .reusable = true}}, SHIFT(177),
  [511] = {.entry = {.count = 1, .reusable = true}}, SHIFT(102),
  [513] = {.entry = {.count = 1, .reusable = true}}, SHIFT(195),
  [515] = {.entry = {.count = 1, .reusable = true}}, SHIFT(196),
  [517] = {.entry = {.count = 1, .reusable = true}}, SHIFT(154),
  [519] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 2),
  [521] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [523] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [525] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 6),
  [527] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 6),
  [529] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [531] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 3),
  [533] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [535] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [537] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [539] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 4),
  [541] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [543] = {.entry = {.count = 1, .reusable = true}}, SHIFT(92),
  [545] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [547] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [549] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [551] = {.entry = {.count = 1, .reusable = true}}, SHIFT(75),
  [553] = {.entry = {.count = 1, .reusable = true}}, SHIFT(228),
  [555] = {.entry = {.count = 1, .reusable = true}}, SHIFT(153),
  [557] = {.entry = {.count = 1, .reusable = true}}, SHIFT(205),
  [559] = {.entry = {.count = 1, .reusable = true}}, SHIFT(152),
  [561] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [563] = {.entry = {.count = 1, .reusable = true}}, SHIFT(208),
  [565] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [567] = {.entry = {.count = 1, .reusable = true}}, SHIFT(161),
  [569] = {.entry = {.count = 1, .reusable = true}}, SHIFT(158),
  [571] = {.entry = {.count = 1, .reusable = true}}, SHIFT(212),
  [573] = {.entry = {.count = 1, .reusable = true}}, SHIFT(157),
  [575] = {.entry = {.count = 1, .reusable = true}}, SHIFT(219),
  [577] = {.entry = {.count = 1, .reusable = true}}, SHIFT(220),
  [579] = {.entry = {.count = 1, .reusable = true}}, SHIFT(140),
  [581] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 4),
  [583] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 3),
  [585] = {.entry = {.count = 1, .reusable = true}}, SHIFT(131),
  [587] = {.entry = {.count = 1, .reusable = true}}, SHIFT(142),
  [589] = {.entry = {.count = 1, .reusable = true}}, SHIFT(221),
  [591] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [593] = {.entry = {.count = 1, .reusable = true}}, SHIFT(222),
  [595] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [597] = {.entry = {.count = 1, .reusable = true}}, SHIFT(150),
  [599] = {.entry = {.count = 1, .reusable = true}}, SHIFT(64),
  [601] = {.entry = {.count = 1, .reusable = true}}, SHIFT(149),
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
