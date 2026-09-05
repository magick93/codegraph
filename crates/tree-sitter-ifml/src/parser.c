#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#ifdef _MSC_VER
#pragma optimize("", off)
#elif defined(__clang__)
#pragma clang optimize off
#elif defined(__GNUC__)
#pragma GCC optimize ("O0")
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 294
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 163
#define ALIAS_COUNT 0
#define TOKEN_COUNT 90
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
  anon_sym_column = 13,
  anon_sym_DASH_GT = 14,
  anon_sym_field = 15,
  anon_sym_DOT = 16,
  anon_sym_lookup = 17,
  anon_sym_via = 18,
  anon_sym_expr = 19,
  anon_sym_text = 20,
  anon_sym_textarea = 21,
  anon_sym_password = 22,
  anon_sym_email = 23,
  anon_sym_number = 24,
  anon_sym_date = 25,
  anon_sym_time = 26,
  anon_sym_datetime = 27,
  anon_sym_dropdown = 28,
  anon_sym_radio = 29,
  anon_sym_checkbox = 30,
  anon_sym_toggle = 31,
  anon_sym_file = 32,
  anon_sym_hidden = 33,
  anon_sym_chart = 34,
  anon_sym_bar = 35,
  anon_sym_line = 36,
  anon_sym_pie = 37,
  anon_sym_radar = 38,
  anon_sym_metric = 39,
  anon_sym_params = 40,
  anon_sym_COMMA = 41,
  anon_sym_COLON = 42,
  anon_sym_label = 43,
  anon_sym_LBRACK = 44,
  anon_sym_RBRACK = 45,
  anon_sym_on = 46,
  anon_sym_select = 47,
  anon_sym_submit = 48,
  anon_sym_click = 49,
  anon_sym_change = 50,
  anon_sym_load = 51,
  anon_sym_save = 52,
  anon_sym_cancel = 53,
  anon_sym_delete = 54,
  anon_sym_confirm = 55,
  anon_sym_back = 56,
  anon_sym_LPAREN = 57,
  anon_sym_RPAREN = 58,
  anon_sym_navigate = 59,
  anon_sym_refresh = 60,
  sym_stay_statement = 61,
  anon_sym_Uuid = 62,
  anon_sym_String = 63,
  anon_sym_Int = 64,
  anon_sym_Float = 65,
  anon_sym_Boolean = 66,
  anon_sym_DateTime = 67,
  anon_sym_PIPE_PIPE = 68,
  anon_sym_AMP_AMP = 69,
  anon_sym_EQ_EQ = 70,
  anon_sym_BANG_EQ = 71,
  anon_sym_LT = 72,
  anon_sym_LT_EQ = 73,
  anon_sym_GT = 74,
  anon_sym_GT_EQ = 75,
  anon_sym_TILDE_EQ = 76,
  anon_sym_BANG_TILDE = 77,
  anon_sym_PLUS = 78,
  anon_sym_DASH = 79,
  anon_sym_STAR = 80,
  anon_sym_SLASH = 81,
  anon_sym_PERCENT = 82,
  anon_sym_BANG = 83,
  sym_identifier = 84,
  sym_string = 85,
  sym_number = 86,
  anon_sym_true = 87,
  anon_sym_false = 88,
  sym_comment = 89,
  sym_source_file = 90,
  sym__definition = 91,
  sym_domain_declaration = 92,
  sym_view_declaration = 93,
  sym_container_declaration = 94,
  sym_component_declaration = 95,
  sym_action_declaration = 96,
  sym_module_declaration = 97,
  sym_view_body = 98,
  sym_component_body = 99,
  sym_column_decl = 100,
  sym_field_ref = 101,
  sym_lookup_ref = 102,
  sym_expr_ref = 103,
  sym_field_decl = 104,
  sym_input_type = 105,
  sym_input_body = 106,
  sym_chart_decl = 107,
  sym_chart_kind = 108,
  sym_chart_body = 109,
  sym_action_body = 110,
  sym_params_block = 111,
  sym_parameter_block = 112,
  sym_parameter_decl = 113,
  sym_label_declaration = 114,
  sym_property_assignment = 115,
  sym_value_expression = 116,
  sym_array_literal = 117,
  sym_object_literal = 118,
  sym_object_member = 119,
  sym_object_member_value = 120,
  sym_event_handler = 121,
  sym_event_type = 122,
  sym_event_param = 123,
  sym_event_action = 124,
  sym_navigate_action = 125,
  sym_refresh_action = 126,
  sym_action_invocation = 127,
  sym_parameter_binding = 128,
  sym_binding_pair = 129,
  sym_type_ref = 130,
  sym_expression = 131,
  sym__logical_or = 132,
  sym__logical_and = 133,
  sym__comparison = 134,
  sym__comparison_op = 135,
  sym__addition = 136,
  sym__add_op = 137,
  sym__multiplication = 138,
  sym__mul_op = 139,
  sym__unary = 140,
  sym__primary = 141,
  sym_call_expr = 142,
  sym_field_expr = 143,
  sym_group_expr = 144,
  sym_boolean = 145,
  aux_sym_source_file_repeat1 = 146,
  aux_sym_module_declaration_repeat1 = 147,
  aux_sym_component_body_repeat1 = 148,
  aux_sym_input_body_repeat1 = 149,
  aux_sym_action_body_repeat1 = 150,
  aux_sym_parameter_block_repeat1 = 151,
  aux_sym_array_literal_repeat1 = 152,
  aux_sym_object_literal_repeat1 = 153,
  aux_sym_event_param_repeat1 = 154,
  aux_sym_parameter_binding_repeat1 = 155,
  aux_sym__logical_or_repeat1 = 156,
  aux_sym__logical_and_repeat1 = 157,
  aux_sym__comparison_repeat1 = 158,
  aux_sym__addition_repeat1 = 159,
  aux_sym__multiplication_repeat1 = 160,
  aux_sym_call_expr_repeat1 = 161,
  aux_sym_field_expr_repeat1 = 162,
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
  [anon_sym_column] = "column",
  [anon_sym_DASH_GT] = "->",
  [anon_sym_field] = "field",
  [anon_sym_DOT] = ".",
  [anon_sym_lookup] = "lookup",
  [anon_sym_via] = "via",
  [anon_sym_expr] = "expr",
  [anon_sym_text] = "text",
  [anon_sym_textarea] = "textarea",
  [anon_sym_password] = "password",
  [anon_sym_email] = "email",
  [anon_sym_number] = "number",
  [anon_sym_date] = "date",
  [anon_sym_time] = "time",
  [anon_sym_datetime] = "datetime",
  [anon_sym_dropdown] = "dropdown",
  [anon_sym_radio] = "radio",
  [anon_sym_checkbox] = "checkbox",
  [anon_sym_toggle] = "toggle",
  [anon_sym_file] = "file",
  [anon_sym_hidden] = "hidden",
  [anon_sym_chart] = "chart",
  [anon_sym_bar] = "bar",
  [anon_sym_line] = "line",
  [anon_sym_pie] = "pie",
  [anon_sym_radar] = "radar",
  [anon_sym_metric] = "metric",
  [anon_sym_params] = "params",
  [anon_sym_COMMA] = ",",
  [anon_sym_COLON] = ":",
  [anon_sym_label] = "label",
  [anon_sym_LBRACK] = "[",
  [anon_sym_RBRACK] = "]",
  [anon_sym_on] = "on",
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
  [sym_column_decl] = "column_decl",
  [sym_field_ref] = "field_ref",
  [sym_lookup_ref] = "lookup_ref",
  [sym_expr_ref] = "expr_ref",
  [sym_field_decl] = "field_decl",
  [sym_input_type] = "input_type",
  [sym_input_body] = "input_body",
  [sym_chart_decl] = "chart_decl",
  [sym_chart_kind] = "chart_kind",
  [sym_chart_body] = "chart_body",
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
  [aux_sym_input_body_repeat1] = "input_body_repeat1",
  [aux_sym_action_body_repeat1] = "action_body_repeat1",
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
  [anon_sym_column] = anon_sym_column,
  [anon_sym_DASH_GT] = anon_sym_DASH_GT,
  [anon_sym_field] = anon_sym_field,
  [anon_sym_DOT] = anon_sym_DOT,
  [anon_sym_lookup] = anon_sym_lookup,
  [anon_sym_via] = anon_sym_via,
  [anon_sym_expr] = anon_sym_expr,
  [anon_sym_text] = anon_sym_text,
  [anon_sym_textarea] = anon_sym_textarea,
  [anon_sym_password] = anon_sym_password,
  [anon_sym_email] = anon_sym_email,
  [anon_sym_number] = anon_sym_number,
  [anon_sym_date] = anon_sym_date,
  [anon_sym_time] = anon_sym_time,
  [anon_sym_datetime] = anon_sym_datetime,
  [anon_sym_dropdown] = anon_sym_dropdown,
  [anon_sym_radio] = anon_sym_radio,
  [anon_sym_checkbox] = anon_sym_checkbox,
  [anon_sym_toggle] = anon_sym_toggle,
  [anon_sym_file] = anon_sym_file,
  [anon_sym_hidden] = anon_sym_hidden,
  [anon_sym_chart] = anon_sym_chart,
  [anon_sym_bar] = anon_sym_bar,
  [anon_sym_line] = anon_sym_line,
  [anon_sym_pie] = anon_sym_pie,
  [anon_sym_radar] = anon_sym_radar,
  [anon_sym_metric] = anon_sym_metric,
  [anon_sym_params] = anon_sym_params,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_label] = anon_sym_label,
  [anon_sym_LBRACK] = anon_sym_LBRACK,
  [anon_sym_RBRACK] = anon_sym_RBRACK,
  [anon_sym_on] = anon_sym_on,
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
  [sym_column_decl] = sym_column_decl,
  [sym_field_ref] = sym_field_ref,
  [sym_lookup_ref] = sym_lookup_ref,
  [sym_expr_ref] = sym_expr_ref,
  [sym_field_decl] = sym_field_decl,
  [sym_input_type] = sym_input_type,
  [sym_input_body] = sym_input_body,
  [sym_chart_decl] = sym_chart_decl,
  [sym_chart_kind] = sym_chart_kind,
  [sym_chart_body] = sym_chart_body,
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
  [aux_sym_input_body_repeat1] = aux_sym_input_body_repeat1,
  [aux_sym_action_body_repeat1] = aux_sym_action_body_repeat1,
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
  [anon_sym_column] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH_GT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_field] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DOT] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_lookup] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_via] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_expr] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_text] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_textarea] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_password] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_email] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_number] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_date] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_time] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_datetime] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_dropdown] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_radio] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_checkbox] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_toggle] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_file] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_hidden] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_chart] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_bar] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_line] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_pie] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_radar] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_metric] = {
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
  [sym_column_decl] = {
    .visible = true,
    .named = true,
  },
  [sym_field_ref] = {
    .visible = true,
    .named = true,
  },
  [sym_lookup_ref] = {
    .visible = true,
    .named = true,
  },
  [sym_expr_ref] = {
    .visible = true,
    .named = true,
  },
  [sym_field_decl] = {
    .visible = true,
    .named = true,
  },
  [sym_input_type] = {
    .visible = true,
    .named = true,
  },
  [sym_input_body] = {
    .visible = true,
    .named = true,
  },
  [sym_chart_decl] = {
    .visible = true,
    .named = true,
  },
  [sym_chart_kind] = {
    .visible = true,
    .named = true,
  },
  [sym_chart_body] = {
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
  [aux_sym_input_body_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_action_body_repeat1] = {
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
  [4] = 4,
  [5] = 4,
  [6] = 4,
  [7] = 4,
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
  [41] = 41,
  [42] = 42,
  [43] = 43,
  [44] = 44,
  [45] = 44,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 46,
  [50] = 47,
  [51] = 46,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 56,
  [58] = 58,
  [59] = 59,
  [60] = 60,
  [61] = 61,
  [62] = 61,
  [63] = 63,
  [64] = 58,
  [65] = 65,
  [66] = 60,
  [67] = 67,
  [68] = 67,
  [69] = 69,
  [70] = 70,
  [71] = 71,
  [72] = 69,
  [73] = 73,
  [74] = 70,
  [75] = 75,
  [76] = 69,
  [77] = 77,
  [78] = 70,
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
  [95] = 95,
  [96] = 96,
  [97] = 97,
  [98] = 98,
  [99] = 99,
  [100] = 100,
  [101] = 101,
  [102] = 102,
  [103] = 103,
  [104] = 104,
  [105] = 105,
  [106] = 100,
  [107] = 107,
  [108] = 108,
  [109] = 104,
  [110] = 110,
  [111] = 111,
  [112] = 112,
  [113] = 98,
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
  [124] = 116,
  [125] = 125,
  [126] = 126,
  [127] = 117,
  [128] = 128,
  [129] = 123,
  [130] = 126,
  [131] = 131,
  [132] = 132,
  [133] = 133,
  [134] = 134,
  [135] = 135,
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
  [149] = 145,
  [150] = 150,
  [151] = 104,
  [152] = 152,
  [153] = 153,
  [154] = 154,
  [155] = 155,
  [156] = 100,
  [157] = 157,
  [158] = 98,
  [159] = 159,
  [160] = 160,
  [161] = 161,
  [162] = 145,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 166,
  [167] = 167,
  [168] = 168,
  [169] = 169,
  [170] = 166,
  [171] = 168,
  [172] = 172,
  [173] = 173,
  [174] = 155,
  [175] = 175,
  [176] = 176,
  [177] = 177,
  [178] = 178,
  [179] = 179,
  [180] = 125,
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
  [197] = 128,
  [198] = 198,
  [199] = 104,
  [200] = 200,
  [201] = 201,
  [202] = 202,
  [203] = 203,
  [204] = 204,
  [205] = 205,
  [206] = 206,
  [207] = 207,
  [208] = 208,
  [209] = 107,
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
  [223] = 223,
  [224] = 224,
  [225] = 225,
  [226] = 226,
  [227] = 227,
  [228] = 228,
  [229] = 229,
  [230] = 230,
  [231] = 231,
  [232] = 232,
  [233] = 233,
  [234] = 234,
  [235] = 235,
  [236] = 236,
  [237] = 237,
  [238] = 238,
  [239] = 239,
  [240] = 240,
  [241] = 241,
  [242] = 242,
  [243] = 243,
  [244] = 244,
  [245] = 245,
  [246] = 246,
  [247] = 247,
  [248] = 248,
  [249] = 249,
  [250] = 250,
  [251] = 251,
  [252] = 252,
  [253] = 253,
  [254] = 254,
  [255] = 255,
  [256] = 256,
  [257] = 257,
  [258] = 258,
  [259] = 259,
  [260] = 260,
  [261] = 261,
  [262] = 262,
  [263] = 263,
  [264] = 264,
  [265] = 265,
  [266] = 266,
  [267] = 267,
  [268] = 268,
  [269] = 268,
  [270] = 270,
  [271] = 271,
  [272] = 261,
  [273] = 249,
  [274] = 268,
  [275] = 261,
  [276] = 249,
  [277] = 268,
  [278] = 278,
  [279] = 279,
  [280] = 280,
  [281] = 281,
  [282] = 282,
  [283] = 283,
  [284] = 284,
  [285] = 285,
  [286] = 286,
  [287] = 270,
  [288] = 285,
  [289] = 289,
  [290] = 270,
  [291] = 285,
  [292] = 285,
  [293] = 293,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(226);
      if (lookahead == '!') ADVANCE(335);
      if (lookahead == '"') ADVANCE(3);
      if (lookahead == '%') ADVANCE(333);
      if (lookahead == '&') ADVANCE(4);
      if (lookahead == '(') ADVANCE(301);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(331);
      if (lookahead == '+') ADVANCE(328);
      if (lookahead == ',') ADVANCE(273);
      if (lookahead == '-') ADVANCE(330);
      if (lookahead == '.') ADVANCE(246);
      if (lookahead == '/') ADVANCE(332);
      if (lookahead == ':') ADVANCE(274);
      if (lookahead == ';') ADVANCE(230);
      if (lookahead == '<') ADVANCE(322);
      if (lookahead == '=') ADVANCE(14);
      if (lookahead == '>') ADVANCE(324);
      if (lookahead == 'B') ADVANCE(161);
      if (lookahead == 'D') ADVANCE(27);
      if (lookahead == 'F') ADVANCE(125);
      if (lookahead == 'I') ADVANCE(153);
      if (lookahead == 'S') ADVANCE(191);
      if (lookahead == 'U') ADVANCE(213);
      if (lookahead == '[') ADVANCE(277);
      if (lookahead == ']') ADVANCE(278);
      if (lookahead == 'a') ADVANCE(49);
      if (lookahead == 'b') ADVANCE(18);
      if (lookahead == 'c') ADVANCE(30);
      if (lookahead == 'd') ADVANCE(39);
      if (lookahead == 'e') ADVANCE(134);
      if (lookahead == 'f') ADVANCE(29);
      if (lookahead == 'h') ADVANCE(102);
      if (lookahead == 'i') ADVANCE(152);
      if (lookahead == 'l') ADVANCE(19);
      if (lookahead == 'm') ADVANCE(92);
      if (lookahead == 'n') ADVANCE(20);
      if (lookahead == 'o') ADVANCE(144);
      if (lookahead == 'p') ADVANCE(21);
      if (lookahead == 'r') ADVANCE(31);
      if (lookahead == 's') ADVANCE(28);
      if (lookahead == 't') ADVANCE(62);
      if (lookahead == 'v') ADVANCE(103);
      if (lookahead == '{') ADVANCE(228);
      if (lookahead == '|') ADVANCE(223);
      if (lookahead == '}') ADVANCE(231);
      if (lookahead == '~') ADVANCE(15);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(447);
      END_STATE();
    case 1:
      if (lookahead == '!') ADVANCE(334);
      if (lookahead == '"') ADVANCE(3);
      if (lookahead == '(') ADVANCE(301);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '-') ADVANCE(329);
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == '[') ADVANCE(277);
      if (lookahead == ']') ADVANCE(278);
      if (lookahead == 'f') ADVANCE(337);
      if (lookahead == 't') ADVANCE(423);
      if (lookahead == '{') ADVANCE(228);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(1)
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(447);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 2:
      if (lookahead == '!') ADVANCE(13);
      if (lookahead == '%') ADVANCE(333);
      if (lookahead == '&') ADVANCE(4);
      if (lookahead == '(') ADVANCE(301);
      if (lookahead == ')') ADVANCE(302);
      if (lookahead == '*') ADVANCE(331);
      if (lookahead == '+') ADVANCE(328);
      if (lookahead == ',') ADVANCE(273);
      if (lookahead == '-') ADVANCE(329);
      if (lookahead == '.') ADVANCE(246);
      if (lookahead == '/') ADVANCE(332);
      if (lookahead == ';') ADVANCE(230);
      if (lookahead == '<') ADVANCE(322);
      if (lookahead == '=') ADVANCE(14);
      if (lookahead == '>') ADVANCE(324);
      if (lookahead == ']') ADVANCE(278);
      if (lookahead == 'c') ADVANCE(416);
      if (lookahead == 'l') ADVANCE(338);
      if (lookahead == 'o') ADVANCE(405);
      if (lookahead == 'p') ADVANCE(344);
      if (lookahead == '|') ADVANCE(223);
      if (lookahead == '}') ADVANCE(231);
      if (lookahead == '~') ADVANCE(15);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(2)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(446);
      if (lookahead == '\\') ADVANCE(225);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(3);
      END_STATE();
    case 4:
      if (lookahead == '&') ADVANCE(319);
      END_STATE();
    case 5:
      if (lookahead == '(') ADVANCE(301);
      if (lookahead == '-') ADVANCE(16);
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'b') ADVANCE(341);
      if (lookahead == 'c') ADVANCE(345);
      if (lookahead == 'd') ADVANCE(369);
      if (lookahead == 'l') ADVANCE(419);
      if (lookahead == 's') ADVANCE(342);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(5)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 6:
      if (lookahead == '/') ADVANCE(453);
      END_STATE();
    case 7:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == ';') ADVANCE(230);
      if (lookahead == 'c') ADVANCE(379);
      if (lookahead == 'f') ADVANCE(385);
      if (lookahead == 'o') ADVANCE(405);
      if (lookahead == '}') ADVANCE(231);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(7)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 8:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == ';') ADVANCE(230);
      if (lookahead == '}') ADVANCE(231);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(8)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 9:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'B') ADVANCE(418);
      if (lookahead == 'D') ADVANCE(350);
      if (lookahead == 'F') ADVANCE(398);
      if (lookahead == 'I') ADVANCE(415);
      if (lookahead == 'S') ADVANCE(438);
      if (lookahead == 'U') ADVANCE(443);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(9)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 10:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'c') ADVANCE(416);
      if (lookahead == 'l') ADVANCE(338);
      if (lookahead == 'o') ADVANCE(405);
      if (lookahead == '}') ADVANCE(231);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(10)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 11:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'c') ADVANCE(416);
      if (lookahead == 'o') ADVANCE(405);
      if (lookahead == '}') ADVANCE(231);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(11)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 12:
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'o') ADVANCE(405);
      if (lookahead == '}') ADVANCE(231);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(12)
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 13:
      if (lookahead == '=') ADVANCE(321);
      if (lookahead == '~') ADVANCE(327);
      END_STATE();
    case 14:
      if (lookahead == '=') ADVANCE(320);
      END_STATE();
    case 15:
      if (lookahead == '=') ADVANCE(326);
      END_STATE();
    case 16:
      if (lookahead == '>') ADVANCE(243);
      END_STATE();
    case 17:
      if (lookahead == 'T') ADVANCE(114);
      END_STATE();
    case 18:
      if (lookahead == 'a') ADVANCE(46);
      END_STATE();
    case 19:
      if (lookahead == 'a') ADVANCE(43);
      if (lookahead == 'i') ADVANCE(156);
      if (lookahead == 'o') ADVANCE(35);
      END_STATE();
    case 20:
      if (lookahead == 'a') ADVANCE(215);
      if (lookahead == 'u') ADVANCE(142);
      END_STATE();
    case 21:
      if (lookahead == 'a') ADVANCE(181);
      if (lookahead == 'i') ADVANCE(63);
      END_STATE();
    case 22:
      if (lookahead == 'a') ADVANCE(154);
      if (lookahead == 'e') ADVANCE(48);
      END_STATE();
    case 23:
      if (lookahead == 'a') ADVANCE(222);
      END_STATE();
    case 24:
      if (lookahead == 'a') ADVANCE(248);
      if (lookahead == 'e') ADVANCE(217);
      END_STATE();
    case 25:
      if (lookahead == 'a') ADVANCE(229);
      END_STATE();
    case 26:
      if (lookahead == 'a') ADVANCE(251);
      END_STATE();
    case 27:
      if (lookahead == 'a') ADVANCE(201);
      END_STATE();
    case 28:
      if (lookahead == 'a') ADVANCE(216);
      if (lookahead == 'c') ADVANCE(101);
      if (lookahead == 'e') ADVANCE(129);
      if (lookahead == 't') ADVANCE(23);
      if (lookahead == 'u') ADVANCE(42);
      END_STATE();
    case 29:
      if (lookahead == 'a') ADVANCE(121);
      if (lookahead == 'i') ADVANCE(79);
      END_STATE();
    case 30:
      if (lookahead == 'a') ADVANCE(151);
      if (lookahead == 'h') ADVANCE(22);
      if (lookahead == 'l') ADVANCE(113);
      if (lookahead == 'o') ADVANCE(126);
      END_STATE();
    case 31:
      if (lookahead == 'a') ADVANCE(57);
      if (lookahead == 'e') ADVANCE(93);
      END_STATE();
    case 32:
      if (lookahead == 'a') ADVANCE(177);
      if (lookahead == 'i') ADVANCE(160);
      END_STATE();
    case 33:
      if (lookahead == 'a') ADVANCE(108);
      END_STATE();
    case 34:
      if (lookahead == 'a') ADVANCE(135);
      END_STATE();
    case 35:
      if (lookahead == 'a') ADVANCE(54);
      if (lookahead == 'o') ADVANCE(120);
      END_STATE();
    case 36:
      if (lookahead == 'a') ADVANCE(194);
      END_STATE();
    case 37:
      if (lookahead == 'a') ADVANCE(149);
      END_STATE();
    case 38:
      if (lookahead == 'a') ADVANCE(116);
      END_STATE();
    case 39:
      if (lookahead == 'a') ADVANCE(202);
      if (lookahead == 'e') ADVANCE(127);
      if (lookahead == 'o') ADVANCE(143);
      if (lookahead == 'r') ADVANCE(164);
      END_STATE();
    case 40:
      if (lookahead == 'a') ADVANCE(111);
      END_STATE();
    case 41:
      if (lookahead == 'a') ADVANCE(205);
      END_STATE();
    case 42:
      if (lookahead == 'b') ADVANCE(138);
      END_STATE();
    case 43:
      if (lookahead == 'b') ADVANCE(82);
      END_STATE();
    case 44:
      if (lookahead == 'b') ADVANCE(162);
      END_STATE();
    case 45:
      if (lookahead == 'b') ADVANCE(83);
      END_STATE();
    case 46:
      if (lookahead == 'c') ADVANCE(117);
      if (lookahead == 'r') ADVANCE(266);
      END_STATE();
    case 47:
      if (lookahead == 'c') ADVANCE(270);
      END_STATE();
    case 48:
      if (lookahead == 'c') ADVANCE(119);
      END_STATE();
    case 49:
      if (lookahead == 'c') ADVANCE(203);
      END_STATE();
    case 50:
      if (lookahead == 'c') ADVANCE(118);
      END_STATE();
    case 51:
      if (lookahead == 'c') ADVANCE(198);
      END_STATE();
    case 52:
      if (lookahead == 'c') ADVANCE(84);
      END_STATE();
    case 53:
      if (lookahead == 'd') ADVANCE(306);
      END_STATE();
    case 54:
      if (lookahead == 'd') ADVANCE(289);
      END_STATE();
    case 55:
      if (lookahead == 'd') ADVANCE(244);
      END_STATE();
    case 56:
      if (lookahead == 'd') ADVANCE(252);
      END_STATE();
    case 57:
      if (lookahead == 'd') ADVANCE(32);
      END_STATE();
    case 58:
      if (lookahead == 'd') ADVANCE(214);
      END_STATE();
    case 59:
      if (lookahead == 'd') ADVANCE(61);
      END_STATE();
    case 60:
      if (lookahead == 'd') ADVANCE(163);
      END_STATE();
    case 61:
      if (lookahead == 'd') ADVANCE(87);
      END_STATE();
    case 62:
      if (lookahead == 'e') ADVANCE(221);
      if (lookahead == 'i') ADVANCE(139);
      if (lookahead == 'o') ADVANCE(96);
      if (lookahead == 'r') ADVANCE(212);
      END_STATE();
    case 63:
      if (lookahead == 'e') ADVANCE(268);
      END_STATE();
    case 64:
      if (lookahead == 'e') ADVANCE(17);
      END_STATE();
    case 65:
      if (lookahead == 'e') ADVANCE(255);
      END_STATE();
    case 66:
      if (lookahead == 'e') ADVANCE(262);
      END_STATE();
    case 67:
      if (lookahead == 'e') ADVANCE(267);
      END_STATE();
    case 68:
      if (lookahead == 'e') ADVANCE(291);
      END_STATE();
    case 69:
      if (lookahead == 'e') ADVANCE(256);
      END_STATE();
    case 70:
      if (lookahead == 'e') ADVANCE(449);
      END_STATE();
    case 71:
      if (lookahead == 'e') ADVANCE(451);
      END_STATE();
    case 72:
      if (lookahead == 'e') ADVANCE(287);
      END_STATE();
    case 73:
      if (lookahead == 'e') ADVANCE(295);
      END_STATE();
    case 74:
      if (lookahead == 'e') ADVANCE(238);
      END_STATE();
    case 75:
      if (lookahead == 'e') ADVANCE(261);
      END_STATE();
    case 76:
      if (lookahead == 'e') ADVANCE(316);
      END_STATE();
    case 77:
      if (lookahead == 'e') ADVANCE(257);
      END_STATE();
    case 78:
      if (lookahead == 'e') ADVANCE(303);
      END_STATE();
    case 79:
      if (lookahead == 'e') ADVANCE(128);
      if (lookahead == 'l') ADVANCE(66);
      END_STATE();
    case 80:
      if (lookahead == 'e') ADVANCE(188);
      END_STATE();
    case 81:
      if (lookahead == 'e') ADVANCE(37);
      END_STATE();
    case 82:
      if (lookahead == 'e') ADVANCE(123);
      END_STATE();
    case 83:
      if (lookahead == 'e') ADVANCE(178);
      END_STATE();
    case 84:
      if (lookahead == 'e') ADVANCE(124);
      END_STATE();
    case 85:
      if (lookahead == 'e') ADVANCE(26);
      END_STATE();
    case 86:
      if (lookahead == 'e') ADVANCE(179);
      END_STATE();
    case 87:
      if (lookahead == 'e') ADVANCE(148);
      END_STATE();
    case 88:
      if (lookahead == 'e') ADVANCE(157);
      END_STATE();
    case 89:
      if (lookahead == 'e') ADVANCE(51);
      END_STATE();
    case 90:
      if (lookahead == 'e') ADVANCE(136);
      END_STATE();
    case 91:
      if (lookahead == 'e') ADVANCE(204);
      END_STATE();
    case 92:
      if (lookahead == 'e') ADVANCE(206);
      if (lookahead == 'o') ADVANCE(58);
      END_STATE();
    case 93:
      if (lookahead == 'f') ADVANCE(185);
      END_STATE();
    case 94:
      if (lookahead == 'f') ADVANCE(110);
      if (lookahead == 't') ADVANCE(38);
      END_STATE();
    case 95:
      if (lookahead == 'g') ADVANCE(308);
      END_STATE();
    case 96:
      if (lookahead == 'g') ADVANCE(98);
      END_STATE();
    case 97:
      if (lookahead == 'g') ADVANCE(72);
      END_STATE();
    case 98:
      if (lookahead == 'g') ADVANCE(132);
      END_STATE();
    case 99:
      if (lookahead == 'g') ADVANCE(41);
      END_STATE();
    case 100:
      if (lookahead == 'h') ADVANCE(304);
      END_STATE();
    case 101:
      if (lookahead == 'h') ADVANCE(90);
      END_STATE();
    case 102:
      if (lookahead == 'i') ADVANCE(59);
      END_STATE();
    case 103:
      if (lookahead == 'i') ADVANCE(24);
      END_STATE();
    case 104:
      if (lookahead == 'i') ADVANCE(47);
      END_STATE();
    case 105:
      if (lookahead == 'i') ADVANCE(53);
      END_STATE();
    case 106:
      if (lookahead == 'i') ADVANCE(99);
      END_STATE();
    case 107:
      if (lookahead == 'i') ADVANCE(165);
      END_STATE();
    case 108:
      if (lookahead == 'i') ADVANCE(122);
      END_STATE();
    case 109:
      if (lookahead == 'i') ADVANCE(155);
      END_STATE();
    case 110:
      if (lookahead == 'i') ADVANCE(182);
      END_STATE();
    case 111:
      if (lookahead == 'i') ADVANCE(147);
      END_STATE();
    case 112:
      if (lookahead == 'i') ADVANCE(199);
      END_STATE();
    case 113:
      if (lookahead == 'i') ADVANCE(50);
      END_STATE();
    case 114:
      if (lookahead == 'i') ADVANCE(140);
      END_STATE();
    case 115:
      if (lookahead == 'i') ADVANCE(141);
      END_STATE();
    case 116:
      if (lookahead == 'i') ADVANCE(159);
      END_STATE();
    case 117:
      if (lookahead == 'k') ADVANCE(299);
      END_STATE();
    case 118:
      if (lookahead == 'k') ADVANCE(285);
      END_STATE();
    case 119:
      if (lookahead == 'k') ADVANCE(44);
      END_STATE();
    case 120:
      if (lookahead == 'k') ADVANCE(209);
      END_STATE();
    case 121:
      if (lookahead == 'l') ADVANCE(190);
      END_STATE();
    case 122:
      if (lookahead == 'l') ADVANCE(253);
      END_STATE();
    case 123:
      if (lookahead == 'l') ADVANCE(275);
      END_STATE();
    case 124:
      if (lookahead == 'l') ADVANCE(293);
      END_STATE();
    case 125:
      if (lookahead == 'l') ADVANCE(167);
      END_STATE();
    case 126:
      if (lookahead == 'l') ADVANCE(208);
      if (lookahead == 'm') ADVANCE(173);
      if (lookahead == 'n') ADVANCE(94);
      END_STATE();
    case 127:
      if (lookahead == 'l') ADVANCE(91);
      END_STATE();
    case 128:
      if (lookahead == 'l') ADVANCE(55);
      END_STATE();
    case 129:
      if (lookahead == 'l') ADVANCE(89);
      END_STATE();
    case 130:
      if (lookahead == 'l') ADVANCE(81);
      END_STATE();
    case 131:
      if (lookahead == 'l') ADVANCE(74);
      END_STATE();
    case 132:
      if (lookahead == 'l') ADVANCE(75);
      END_STATE();
    case 133:
      if (lookahead == 'm') ADVANCE(297);
      END_STATE();
    case 134:
      if (lookahead == 'm') ADVANCE(33);
      if (lookahead == 'x') ADVANCE(171);
      END_STATE();
    case 135:
      if (lookahead == 'm') ADVANCE(187);
      END_STATE();
    case 136:
      if (lookahead == 'm') ADVANCE(25);
      END_STATE();
    case 137:
      if (lookahead == 'm') ADVANCE(146);
      END_STATE();
    case 138:
      if (lookahead == 'm') ADVANCE(112);
      END_STATE();
    case 139:
      if (lookahead == 'm') ADVANCE(69);
      END_STATE();
    case 140:
      if (lookahead == 'm') ADVANCE(76);
      END_STATE();
    case 141:
      if (lookahead == 'm') ADVANCE(77);
      END_STATE();
    case 142:
      if (lookahead == 'm') ADVANCE(45);
      END_STATE();
    case 143:
      if (lookahead == 'm') ADVANCE(40);
      END_STATE();
    case 144:
      if (lookahead == 'n') ADVANCE(279);
      if (lookahead == 'u') ADVANCE(207);
      END_STATE();
    case 145:
      if (lookahead == 'n') ADVANCE(237);
      END_STATE();
    case 146:
      if (lookahead == 'n') ADVANCE(241);
      END_STATE();
    case 147:
      if (lookahead == 'n') ADVANCE(227);
      END_STATE();
    case 148:
      if (lookahead == 'n') ADVANCE(263);
      END_STATE();
    case 149:
      if (lookahead == 'n') ADVANCE(314);
      END_STATE();
    case 150:
      if (lookahead == 'n') ADVANCE(258);
      END_STATE();
    case 151:
      if (lookahead == 'n') ADVANCE(52);
      END_STATE();
    case 152:
      if (lookahead == 'n') ADVANCE(172);
      END_STATE();
    case 153:
      if (lookahead == 'n') ADVANCE(192);
      END_STATE();
    case 154:
      if (lookahead == 'n') ADVANCE(97);
      if (lookahead == 'r') ADVANCE(195);
      END_STATE();
    case 155:
      if (lookahead == 'n') ADVANCE(95);
      END_STATE();
    case 156:
      if (lookahead == 'n') ADVANCE(67);
      END_STATE();
    case 157:
      if (lookahead == 'n') ADVANCE(200);
      END_STATE();
    case 158:
      if (lookahead == 'n') ADVANCE(88);
      END_STATE();
    case 159:
      if (lookahead == 'n') ADVANCE(86);
      END_STATE();
    case 160:
      if (lookahead == 'o') ADVANCE(259);
      END_STATE();
    case 161:
      if (lookahead == 'o') ADVANCE(169);
      END_STATE();
    case 162:
      if (lookahead == 'o') ADVANCE(220);
      END_STATE();
    case 163:
      if (lookahead == 'o') ADVANCE(219);
      END_STATE();
    case 164:
      if (lookahead == 'o') ADVANCE(174);
      END_STATE();
    case 165:
      if (lookahead == 'o') ADVANCE(145);
      END_STATE();
    case 166:
      if (lookahead == 'o') ADVANCE(184);
      END_STATE();
    case 167:
      if (lookahead == 'o') ADVANCE(36);
      END_STATE();
    case 168:
      if (lookahead == 'o') ADVANCE(158);
      END_STATE();
    case 169:
      if (lookahead == 'o') ADVANCE(130);
      END_STATE();
    case 170:
      if (lookahead == 'p') ADVANCE(247);
      END_STATE();
    case 171:
      if (lookahead == 'p') ADVANCE(176);
      END_STATE();
    case 172:
      if (lookahead == 'p') ADVANCE(210);
      END_STATE();
    case 173:
      if (lookahead == 'p') ADVANCE(168);
      END_STATE();
    case 174:
      if (lookahead == 'p') ADVANCE(60);
      END_STATE();
    case 175:
      if (lookahead == 'p') ADVANCE(211);
      END_STATE();
    case 176:
      if (lookahead == 'r') ADVANCE(249);
      END_STATE();
    case 177:
      if (lookahead == 'r') ADVANCE(269);
      END_STATE();
    case 178:
      if (lookahead == 'r') ADVANCE(254);
      END_STATE();
    case 179:
      if (lookahead == 'r') ADVANCE(233);
      END_STATE();
    case 180:
      if (lookahead == 'r') ADVANCE(109);
      END_STATE();
    case 181:
      if (lookahead == 'r') ADVANCE(34);
      if (lookahead == 's') ADVANCE(189);
      END_STATE();
    case 182:
      if (lookahead == 'r') ADVANCE(133);
      END_STATE();
    case 183:
      if (lookahead == 'r') ADVANCE(104);
      END_STATE();
    case 184:
      if (lookahead == 'r') ADVANCE(56);
      END_STATE();
    case 185:
      if (lookahead == 'r') ADVANCE(80);
      END_STATE();
    case 186:
      if (lookahead == 'r') ADVANCE(85);
      END_STATE();
    case 187:
      if (lookahead == 's') ADVANCE(271);
      END_STATE();
    case 188:
      if (lookahead == 's') ADVANCE(100);
      END_STATE();
    case 189:
      if (lookahead == 's') ADVANCE(218);
      END_STATE();
    case 190:
      if (lookahead == 's') ADVANCE(71);
      END_STATE();
    case 191:
      if (lookahead == 't') ADVANCE(180);
      END_STATE();
    case 192:
      if (lookahead == 't') ADVANCE(310);
      END_STATE();
    case 193:
      if (lookahead == 't') ADVANCE(250);
      END_STATE();
    case 194:
      if (lookahead == 't') ADVANCE(312);
      END_STATE();
    case 195:
      if (lookahead == 't') ADVANCE(264);
      END_STATE();
    case 196:
      if (lookahead == 't') ADVANCE(239);
      END_STATE();
    case 197:
      if (lookahead == 't') ADVANCE(240);
      END_STATE();
    case 198:
      if (lookahead == 't') ADVANCE(281);
      END_STATE();
    case 199:
      if (lookahead == 't') ADVANCE(283);
      END_STATE();
    case 200:
      if (lookahead == 't') ADVANCE(235);
      END_STATE();
    case 201:
      if (lookahead == 't') ADVANCE(64);
      END_STATE();
    case 202:
      if (lookahead == 't') ADVANCE(65);
      END_STATE();
    case 203:
      if (lookahead == 't') ADVANCE(107);
      END_STATE();
    case 204:
      if (lookahead == 't') ADVANCE(73);
      END_STATE();
    case 205:
      if (lookahead == 't') ADVANCE(78);
      END_STATE();
    case 206:
      if (lookahead == 't') ADVANCE(183);
      END_STATE();
    case 207:
      if (lookahead == 't') ADVANCE(175);
      END_STATE();
    case 208:
      if (lookahead == 'u') ADVANCE(137);
      END_STATE();
    case 209:
      if (lookahead == 'u') ADVANCE(170);
      END_STATE();
    case 210:
      if (lookahead == 'u') ADVANCE(196);
      END_STATE();
    case 211:
      if (lookahead == 'u') ADVANCE(197);
      END_STATE();
    case 212:
      if (lookahead == 'u') ADVANCE(70);
      END_STATE();
    case 213:
      if (lookahead == 'u') ADVANCE(105);
      END_STATE();
    case 214:
      if (lookahead == 'u') ADVANCE(131);
      END_STATE();
    case 215:
      if (lookahead == 'v') ADVANCE(106);
      END_STATE();
    case 216:
      if (lookahead == 'v') ADVANCE(68);
      END_STATE();
    case 217:
      if (lookahead == 'w') ADVANCE(232);
      END_STATE();
    case 218:
      if (lookahead == 'w') ADVANCE(166);
      END_STATE();
    case 219:
      if (lookahead == 'w') ADVANCE(150);
      END_STATE();
    case 220:
      if (lookahead == 'x') ADVANCE(260);
      END_STATE();
    case 221:
      if (lookahead == 'x') ADVANCE(193);
      END_STATE();
    case 222:
      if (lookahead == 'y') ADVANCE(305);
      END_STATE();
    case 223:
      if (lookahead == '|') ADVANCE(318);
      END_STATE();
    case 224:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(448);
      END_STATE();
    case 225:
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(3);
      END_STATE();
    case 226:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 227:
      ACCEPT_TOKEN(anon_sym_domain);
      END_STATE();
    case 228:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 229:
      ACCEPT_TOKEN(anon_sym_schema);
      END_STATE();
    case 230:
      ACCEPT_TOKEN(anon_sym_SEMI);
      END_STATE();
    case 231:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 232:
      ACCEPT_TOKEN(anon_sym_view);
      END_STATE();
    case 233:
      ACCEPT_TOKEN(anon_sym_container);
      END_STATE();
    case 234:
      ACCEPT_TOKEN(anon_sym_container);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 235:
      ACCEPT_TOKEN(anon_sym_component);
      END_STATE();
    case 236:
      ACCEPT_TOKEN(anon_sym_component);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 237:
      ACCEPT_TOKEN(anon_sym_action);
      END_STATE();
    case 238:
      ACCEPT_TOKEN(anon_sym_module);
      END_STATE();
    case 239:
      ACCEPT_TOKEN(anon_sym_input);
      END_STATE();
    case 240:
      ACCEPT_TOKEN(anon_sym_output);
      END_STATE();
    case 241:
      ACCEPT_TOKEN(anon_sym_column);
      END_STATE();
    case 242:
      ACCEPT_TOKEN(anon_sym_column);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 243:
      ACCEPT_TOKEN(anon_sym_DASH_GT);
      END_STATE();
    case 244:
      ACCEPT_TOKEN(anon_sym_field);
      END_STATE();
    case 245:
      ACCEPT_TOKEN(anon_sym_field);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 246:
      ACCEPT_TOKEN(anon_sym_DOT);
      END_STATE();
    case 247:
      ACCEPT_TOKEN(anon_sym_lookup);
      END_STATE();
    case 248:
      ACCEPT_TOKEN(anon_sym_via);
      END_STATE();
    case 249:
      ACCEPT_TOKEN(anon_sym_expr);
      END_STATE();
    case 250:
      ACCEPT_TOKEN(anon_sym_text);
      if (lookahead == 'a') ADVANCE(186);
      END_STATE();
    case 251:
      ACCEPT_TOKEN(anon_sym_textarea);
      END_STATE();
    case 252:
      ACCEPT_TOKEN(anon_sym_password);
      END_STATE();
    case 253:
      ACCEPT_TOKEN(anon_sym_email);
      END_STATE();
    case 254:
      ACCEPT_TOKEN(anon_sym_number);
      END_STATE();
    case 255:
      ACCEPT_TOKEN(anon_sym_date);
      if (lookahead == 't') ADVANCE(115);
      END_STATE();
    case 256:
      ACCEPT_TOKEN(anon_sym_time);
      END_STATE();
    case 257:
      ACCEPT_TOKEN(anon_sym_datetime);
      END_STATE();
    case 258:
      ACCEPT_TOKEN(anon_sym_dropdown);
      END_STATE();
    case 259:
      ACCEPT_TOKEN(anon_sym_radio);
      END_STATE();
    case 260:
      ACCEPT_TOKEN(anon_sym_checkbox);
      END_STATE();
    case 261:
      ACCEPT_TOKEN(anon_sym_toggle);
      END_STATE();
    case 262:
      ACCEPT_TOKEN(anon_sym_file);
      END_STATE();
    case 263:
      ACCEPT_TOKEN(anon_sym_hidden);
      END_STATE();
    case 264:
      ACCEPT_TOKEN(anon_sym_chart);
      END_STATE();
    case 265:
      ACCEPT_TOKEN(anon_sym_chart);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 266:
      ACCEPT_TOKEN(anon_sym_bar);
      END_STATE();
    case 267:
      ACCEPT_TOKEN(anon_sym_line);
      END_STATE();
    case 268:
      ACCEPT_TOKEN(anon_sym_pie);
      END_STATE();
    case 269:
      ACCEPT_TOKEN(anon_sym_radar);
      END_STATE();
    case 270:
      ACCEPT_TOKEN(anon_sym_metric);
      END_STATE();
    case 271:
      ACCEPT_TOKEN(anon_sym_params);
      END_STATE();
    case 272:
      ACCEPT_TOKEN(anon_sym_params);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 273:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 274:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 275:
      ACCEPT_TOKEN(anon_sym_label);
      END_STATE();
    case 276:
      ACCEPT_TOKEN(anon_sym_label);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 277:
      ACCEPT_TOKEN(anon_sym_LBRACK);
      END_STATE();
    case 278:
      ACCEPT_TOKEN(anon_sym_RBRACK);
      END_STATE();
    case 279:
      ACCEPT_TOKEN(anon_sym_on);
      END_STATE();
    case 280:
      ACCEPT_TOKEN(anon_sym_on);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 281:
      ACCEPT_TOKEN(anon_sym_select);
      END_STATE();
    case 282:
      ACCEPT_TOKEN(anon_sym_select);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 283:
      ACCEPT_TOKEN(anon_sym_submit);
      END_STATE();
    case 284:
      ACCEPT_TOKEN(anon_sym_submit);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 285:
      ACCEPT_TOKEN(anon_sym_click);
      END_STATE();
    case 286:
      ACCEPT_TOKEN(anon_sym_click);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 287:
      ACCEPT_TOKEN(anon_sym_change);
      END_STATE();
    case 288:
      ACCEPT_TOKEN(anon_sym_change);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 289:
      ACCEPT_TOKEN(anon_sym_load);
      END_STATE();
    case 290:
      ACCEPT_TOKEN(anon_sym_load);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 291:
      ACCEPT_TOKEN(anon_sym_save);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(anon_sym_save);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 293:
      ACCEPT_TOKEN(anon_sym_cancel);
      END_STATE();
    case 294:
      ACCEPT_TOKEN(anon_sym_cancel);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 295:
      ACCEPT_TOKEN(anon_sym_delete);
      END_STATE();
    case 296:
      ACCEPT_TOKEN(anon_sym_delete);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 297:
      ACCEPT_TOKEN(anon_sym_confirm);
      END_STATE();
    case 298:
      ACCEPT_TOKEN(anon_sym_confirm);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 299:
      ACCEPT_TOKEN(anon_sym_back);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(anon_sym_back);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 301:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 302:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 303:
      ACCEPT_TOKEN(anon_sym_navigate);
      END_STATE();
    case 304:
      ACCEPT_TOKEN(anon_sym_refresh);
      END_STATE();
    case 305:
      ACCEPT_TOKEN(sym_stay_statement);
      END_STATE();
    case 306:
      ACCEPT_TOKEN(anon_sym_Uuid);
      END_STATE();
    case 307:
      ACCEPT_TOKEN(anon_sym_Uuid);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 308:
      ACCEPT_TOKEN(anon_sym_String);
      END_STATE();
    case 309:
      ACCEPT_TOKEN(anon_sym_String);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 310:
      ACCEPT_TOKEN(anon_sym_Int);
      END_STATE();
    case 311:
      ACCEPT_TOKEN(anon_sym_Int);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 312:
      ACCEPT_TOKEN(anon_sym_Float);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(anon_sym_Float);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 314:
      ACCEPT_TOKEN(anon_sym_Boolean);
      END_STATE();
    case 315:
      ACCEPT_TOKEN(anon_sym_Boolean);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 316:
      ACCEPT_TOKEN(anon_sym_DateTime);
      END_STATE();
    case 317:
      ACCEPT_TOKEN(anon_sym_DateTime);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 318:
      ACCEPT_TOKEN(anon_sym_PIPE_PIPE);
      END_STATE();
    case 319:
      ACCEPT_TOKEN(anon_sym_AMP_AMP);
      END_STATE();
    case 320:
      ACCEPT_TOKEN(anon_sym_EQ_EQ);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(anon_sym_BANG_EQ);
      END_STATE();
    case 322:
      ACCEPT_TOKEN(anon_sym_LT);
      if (lookahead == '=') ADVANCE(323);
      END_STATE();
    case 323:
      ACCEPT_TOKEN(anon_sym_LT_EQ);
      END_STATE();
    case 324:
      ACCEPT_TOKEN(anon_sym_GT);
      if (lookahead == '=') ADVANCE(325);
      END_STATE();
    case 325:
      ACCEPT_TOKEN(anon_sym_GT_EQ);
      END_STATE();
    case 326:
      ACCEPT_TOKEN(anon_sym_TILDE_EQ);
      END_STATE();
    case 327:
      ACCEPT_TOKEN(anon_sym_BANG_TILDE);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 329:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 330:
      ACCEPT_TOKEN(anon_sym_DASH);
      if (lookahead == '>') ADVANCE(243);
      END_STATE();
    case 331:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 332:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '/') ADVANCE(453);
      END_STATE();
    case 333:
      ACCEPT_TOKEN(anon_sym_PERCENT);
      END_STATE();
    case 334:
      ACCEPT_TOKEN(anon_sym_BANG);
      END_STATE();
    case 335:
      ACCEPT_TOKEN(anon_sym_BANG);
      if (lookahead == '=') ADVANCE(321);
      if (lookahead == '~') ADVANCE(327);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'T') ADVANCE(383);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 337:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(390);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 338:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(352);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 339:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(401);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 340:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(387);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 341:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(353);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(444);
      if (lookahead == 'e') ADVANCE(396);
      if (lookahead == 'u') ADVANCE(351);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 343:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(357);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 344:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(426);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 345:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(411);
      if (lookahead == 'h') ADVANCE(347);
      if (lookahead == 'l') ADVANCE(386);
      if (lookahead == 'o') ADVANCE(406);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 346:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(427);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 347:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(407);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 348:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(436);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 349:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(409);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 350:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(440);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 351:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(402);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 352:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'b') ADVANCE(367);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 353:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(388);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 354:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(389);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 355:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(432);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 356:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'c') ADVANCE(373);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 357:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(290);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(245);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 359:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'd') ADVANCE(307);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 360:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(450);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 361:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(452);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 362:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(292);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 363:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(288);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 364:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(296);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 365:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(336);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 366:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(317);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 367:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(391);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 368:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(424);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 369:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(395);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 370:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(410);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 371:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(439);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 372:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(355);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 373:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(392);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 374:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(394);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 375:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'e') ADVANCE(349);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 376:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(381);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 377:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(309);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 378:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'g') ADVANCE(363);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 379:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'h') ADVANCE(346);
      if (lookahead == 'o') ADVANCE(393);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 380:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(359);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 381:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(425);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 382:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(433);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 383:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(404);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 384:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(412);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 385:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(374);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 386:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(354);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 387:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'i') ADVANCE(414);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 388:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(300);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 389:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'k') ADVANCE(286);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 390:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(430);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 391:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(276);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 392:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(294);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 393:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(442);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 394:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(358);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 395:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(371);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 396:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(372);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 397:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(375);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 398:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'l') ADVANCE(420);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 399:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(422);
      if (lookahead == 'n') ADVANCE(437);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 400:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(298);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 401:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(429);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 402:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(382);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 403:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(408);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 404:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(366);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 405:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(280);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 406:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(376);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 407:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(378);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 408:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(242);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 409:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(315);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 410:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(431);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 411:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(356);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 412:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(377);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 413:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(370);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 414:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(368);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 415:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(435);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 416:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(399);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 417:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(413);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 418:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(421);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 419:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(343);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 420:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(348);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 421:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'o') ADVANCE(397);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 422:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'p') ADVANCE(417);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 423:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(441);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 424:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(234);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 425:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(400);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 426:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(339);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 427:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(434);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 428:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'r') ADVANCE(384);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 429:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(272);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 430:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 's') ADVANCE(361);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 431:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(236);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 432:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(282);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 433:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(284);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 434:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(265);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 435:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(311);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 436:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(313);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 437:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(340);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 438:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(428);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 439:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(364);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 440:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 't') ADVANCE(365);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 441:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(360);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 442:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(403);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 443:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(380);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 444:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'v') ADVANCE(362);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 445:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 446:
      ACCEPT_TOKEN(sym_string);
      END_STATE();
    case 447:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '.') ADVANCE(224);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(447);
      END_STATE();
    case 448:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(448);
      END_STATE();
    case 449:
      ACCEPT_TOKEN(anon_sym_true);
      END_STATE();
    case 450:
      ACCEPT_TOKEN(anon_sym_true);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 451:
      ACCEPT_TOKEN(anon_sym_false);
      END_STATE();
    case 452:
      ACCEPT_TOKEN(anon_sym_false);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(445);
      END_STATE();
    case 453:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(453);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 0},
  [2] = {.lex_state = 1},
  [3] = {.lex_state = 1},
  [4] = {.lex_state = 1},
  [5] = {.lex_state = 1},
  [6] = {.lex_state = 1},
  [7] = {.lex_state = 1},
  [8] = {.lex_state = 1},
  [9] = {.lex_state = 2},
  [10] = {.lex_state = 2},
  [11] = {.lex_state = 2},
  [12] = {.lex_state = 2},
  [13] = {.lex_state = 2},
  [14] = {.lex_state = 2},
  [15] = {.lex_state = 2},
  [16] = {.lex_state = 2},
  [17] = {.lex_state = 1},
  [18] = {.lex_state = 2},
  [19] = {.lex_state = 1},
  [20] = {.lex_state = 2},
  [21] = {.lex_state = 2},
  [22] = {.lex_state = 2},
  [23] = {.lex_state = 1},
  [24] = {.lex_state = 1},
  [25] = {.lex_state = 2},
  [26] = {.lex_state = 2},
  [27] = {.lex_state = 1},
  [28] = {.lex_state = 2},
  [29] = {.lex_state = 2},
  [30] = {.lex_state = 2},
  [31] = {.lex_state = 2},
  [32] = {.lex_state = 1},
  [33] = {.lex_state = 2},
  [34] = {.lex_state = 2},
  [35] = {.lex_state = 2},
  [36] = {.lex_state = 1},
  [37] = {.lex_state = 2},
  [38] = {.lex_state = 1},
  [39] = {.lex_state = 2},
  [40] = {.lex_state = 1},
  [41] = {.lex_state = 0},
  [42] = {.lex_state = 1},
  [43] = {.lex_state = 1},
  [44] = {.lex_state = 2},
  [45] = {.lex_state = 2},
  [46] = {.lex_state = 5},
  [47] = {.lex_state = 10},
  [48] = {.lex_state = 7},
  [49] = {.lex_state = 5},
  [50] = {.lex_state = 10},
  [51] = {.lex_state = 5},
  [52] = {.lex_state = 7},
  [53] = {.lex_state = 7},
  [54] = {.lex_state = 0},
  [55] = {.lex_state = 0},
  [56] = {.lex_state = 11},
  [57] = {.lex_state = 11},
  [58] = {.lex_state = 11},
  [59] = {.lex_state = 11},
  [60] = {.lex_state = 11},
  [61] = {.lex_state = 11},
  [62] = {.lex_state = 11},
  [63] = {.lex_state = 11},
  [64] = {.lex_state = 11},
  [65] = {.lex_state = 11},
  [66] = {.lex_state = 11},
  [67] = {.lex_state = 11},
  [68] = {.lex_state = 11},
  [69] = {.lex_state = 0},
  [70] = {.lex_state = 0},
  [71] = {.lex_state = 0},
  [72] = {.lex_state = 0},
  [73] = {.lex_state = 0},
  [74] = {.lex_state = 0},
  [75] = {.lex_state = 0},
  [76] = {.lex_state = 0},
  [77] = {.lex_state = 9},
  [78] = {.lex_state = 0},
  [79] = {.lex_state = 0},
  [80] = {.lex_state = 0},
  [81] = {.lex_state = 7},
  [82] = {.lex_state = 7},
  [83] = {.lex_state = 7},
  [84] = {.lex_state = 7},
  [85] = {.lex_state = 0},
  [86] = {.lex_state = 0},
  [87] = {.lex_state = 7},
  [88] = {.lex_state = 7},
  [89] = {.lex_state = 12},
  [90] = {.lex_state = 7},
  [91] = {.lex_state = 10},
  [92] = {.lex_state = 0},
  [93] = {.lex_state = 12},
  [94] = {.lex_state = 12},
  [95] = {.lex_state = 0},
  [96] = {.lex_state = 7},
  [97] = {.lex_state = 0},
  [98] = {.lex_state = 7},
  [99] = {.lex_state = 0},
  [100] = {.lex_state = 7},
  [101] = {.lex_state = 7},
  [102] = {.lex_state = 7},
  [103] = {.lex_state = 7},
  [104] = {.lex_state = 7},
  [105] = {.lex_state = 0},
  [106] = {.lex_state = 11},
  [107] = {.lex_state = 11},
  [108] = {.lex_state = 0},
  [109] = {.lex_state = 11},
  [110] = {.lex_state = 11},
  [111] = {.lex_state = 0},
  [112] = {.lex_state = 0},
  [113] = {.lex_state = 11},
  [114] = {.lex_state = 11},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 0},
  [118] = {.lex_state = 0},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 11},
  [121] = {.lex_state = 11},
  [122] = {.lex_state = 11},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 11},
  [125] = {.lex_state = 11},
  [126] = {.lex_state = 0},
  [127] = {.lex_state = 11},
  [128] = {.lex_state = 11},
  [129] = {.lex_state = 11},
  [130] = {.lex_state = 11},
  [131] = {.lex_state = 0},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 8},
  [134] = {.lex_state = 8},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 8},
  [137] = {.lex_state = 8},
  [138] = {.lex_state = 8},
  [139] = {.lex_state = 8},
  [140] = {.lex_state = 0},
  [141] = {.lex_state = 0},
  [142] = {.lex_state = 0},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 0},
  [145] = {.lex_state = 5},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 0},
  [149] = {.lex_state = 5},
  [150] = {.lex_state = 0},
  [151] = {.lex_state = 12},
  [152] = {.lex_state = 8},
  [153] = {.lex_state = 0},
  [154] = {.lex_state = 0},
  [155] = {.lex_state = 8},
  [156] = {.lex_state = 12},
  [157] = {.lex_state = 8},
  [158] = {.lex_state = 12},
  [159] = {.lex_state = 0},
  [160] = {.lex_state = 0},
  [161] = {.lex_state = 0},
  [162] = {.lex_state = 5},
  [163] = {.lex_state = 0},
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
  [174] = {.lex_state = 8},
  [175] = {.lex_state = 8},
  [176] = {.lex_state = 0},
  [177] = {.lex_state = 0},
  [178] = {.lex_state = 8},
  [179] = {.lex_state = 0},
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
  [192] = {.lex_state = 8},
  [193] = {.lex_state = 0},
  [194] = {.lex_state = 0},
  [195] = {.lex_state = 0},
  [196] = {.lex_state = 0},
  [197] = {.lex_state = 0},
  [198] = {.lex_state = 0},
  [199] = {.lex_state = 8},
  [200] = {.lex_state = 0},
  [201] = {.lex_state = 5},
  [202] = {.lex_state = 0},
  [203] = {.lex_state = 0},
  [204] = {.lex_state = 0},
  [205] = {.lex_state = 0},
  [206] = {.lex_state = 0},
  [207] = {.lex_state = 0},
  [208] = {.lex_state = 0},
  [209] = {.lex_state = 0},
  [210] = {.lex_state = 8},
  [211] = {.lex_state = 0},
  [212] = {.lex_state = 0},
  [213] = {.lex_state = 0},
  [214] = {.lex_state = 0},
  [215] = {.lex_state = 0},
  [216] = {.lex_state = 0},
  [217] = {.lex_state = 8},
  [218] = {.lex_state = 0},
  [219] = {.lex_state = 0},
  [220] = {.lex_state = 8},
  [221] = {.lex_state = 8},
  [222] = {.lex_state = 0},
  [223] = {.lex_state = 0},
  [224] = {.lex_state = 8},
  [225] = {.lex_state = 0},
  [226] = {.lex_state = 0},
  [227] = {.lex_state = 0},
  [228] = {.lex_state = 8},
  [229] = {.lex_state = 0},
  [230] = {.lex_state = 0},
  [231] = {.lex_state = 0},
  [232] = {.lex_state = 0},
  [233] = {.lex_state = 0},
  [234] = {.lex_state = 0},
  [235] = {.lex_state = 0},
  [236] = {.lex_state = 0},
  [237] = {.lex_state = 0},
  [238] = {.lex_state = 0},
  [239] = {.lex_state = 5},
  [240] = {.lex_state = 8},
  [241] = {.lex_state = 0},
  [242] = {.lex_state = 0},
  [243] = {.lex_state = 0},
  [244] = {.lex_state = 0},
  [245] = {.lex_state = 0},
  [246] = {.lex_state = 0},
  [247] = {.lex_state = 0},
  [248] = {.lex_state = 0},
  [249] = {.lex_state = 0},
  [250] = {.lex_state = 5},
  [251] = {.lex_state = 8},
  [252] = {.lex_state = 0},
  [253] = {.lex_state = 0},
  [254] = {.lex_state = 0},
  [255] = {.lex_state = 5},
  [256] = {.lex_state = 5},
  [257] = {.lex_state = 8},
  [258] = {.lex_state = 0},
  [259] = {.lex_state = 0},
  [260] = {.lex_state = 0},
  [261] = {.lex_state = 0},
  [262] = {.lex_state = 0},
  [263] = {.lex_state = 0},
  [264] = {.lex_state = 0},
  [265] = {.lex_state = 0},
  [266] = {.lex_state = 8},
  [267] = {.lex_state = 0},
  [268] = {.lex_state = 0},
  [269] = {.lex_state = 0},
  [270] = {.lex_state = 5},
  [271] = {.lex_state = 8},
  [272] = {.lex_state = 0},
  [273] = {.lex_state = 0},
  [274] = {.lex_state = 0},
  [275] = {.lex_state = 0},
  [276] = {.lex_state = 0},
  [277] = {.lex_state = 0},
  [278] = {.lex_state = 0},
  [279] = {.lex_state = 0},
  [280] = {.lex_state = 0},
  [281] = {.lex_state = 0},
  [282] = {.lex_state = 0},
  [283] = {.lex_state = 0},
  [284] = {.lex_state = 0},
  [285] = {.lex_state = 0},
  [286] = {.lex_state = 0},
  [287] = {.lex_state = 5},
  [288] = {.lex_state = 0},
  [289] = {.lex_state = 0},
  [290] = {.lex_state = 5},
  [291] = {.lex_state = 0},
  [292] = {.lex_state = 0},
  [293] = {.lex_state = 0},
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
    [anon_sym_column] = ACTIONS(1),
    [anon_sym_DASH_GT] = ACTIONS(1),
    [anon_sym_field] = ACTIONS(1),
    [anon_sym_DOT] = ACTIONS(1),
    [anon_sym_lookup] = ACTIONS(1),
    [anon_sym_via] = ACTIONS(1),
    [anon_sym_expr] = ACTIONS(1),
    [anon_sym_text] = ACTIONS(1),
    [anon_sym_textarea] = ACTIONS(1),
    [anon_sym_password] = ACTIONS(1),
    [anon_sym_email] = ACTIONS(1),
    [anon_sym_number] = ACTIONS(1),
    [anon_sym_date] = ACTIONS(1),
    [anon_sym_time] = ACTIONS(1),
    [anon_sym_datetime] = ACTIONS(1),
    [anon_sym_dropdown] = ACTIONS(1),
    [anon_sym_radio] = ACTIONS(1),
    [anon_sym_checkbox] = ACTIONS(1),
    [anon_sym_toggle] = ACTIONS(1),
    [anon_sym_file] = ACTIONS(1),
    [anon_sym_hidden] = ACTIONS(1),
    [anon_sym_chart] = ACTIONS(1),
    [anon_sym_bar] = ACTIONS(1),
    [anon_sym_line] = ACTIONS(1),
    [anon_sym_pie] = ACTIONS(1),
    [anon_sym_radar] = ACTIONS(1),
    [anon_sym_metric] = ACTIONS(1),
    [anon_sym_params] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_label] = ACTIONS(1),
    [anon_sym_LBRACK] = ACTIONS(1),
    [anon_sym_RBRACK] = ACTIONS(1),
    [anon_sym_on] = ACTIONS(1),
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
    [sym_string] = ACTIONS(1),
    [sym_number] = ACTIONS(1),
    [anon_sym_true] = ACTIONS(1),
    [anon_sym_false] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source_file] = STATE(236),
    [sym__definition] = STATE(55),
    [sym_domain_declaration] = STATE(55),
    [sym_view_declaration] = STATE(55),
    [sym_action_declaration] = STATE(55),
    [sym_module_declaration] = STATE(55),
    [aux_sym_source_file_repeat1] = STATE(55),
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(159), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(179), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(277), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(274), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [239] = 16,
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(269), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [298] = 16,
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
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(268), 1,
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
    STATE(172), 3,
      sym_array_literal,
      sym_object_literal,
      sym_expression,
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [357] = 15,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(17), 1,
      anon_sym_LBRACK,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(185), 1,
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
    STATE(186), 2,
      sym_array_literal,
      sym_expression,
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [412] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_DOT,
    STATE(9), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(36), 3,
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
  [446] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(40), 1,
      anon_sym_DOT,
    STATE(9), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(42), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(38), 17,
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
  [480] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_SLASH,
    STATE(11), 1,
      aux_sym__multiplication_repeat1,
    STATE(43), 1,
      sym__mul_op,
    ACTIONS(46), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(48), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(44), 15,
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
  [518] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(40), 1,
      anon_sym_DOT,
    STATE(10), 1,
      aux_sym_field_expr_repeat1,
    ACTIONS(56), 3,
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
  [552] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(64), 1,
      anon_sym_SLASH,
    STATE(14), 1,
      aux_sym__multiplication_repeat1,
    STATE(43), 1,
      sym__mul_op,
    ACTIONS(60), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(62), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(58), 15,
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
  [590] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(64), 1,
      anon_sym_SLASH,
    STATE(11), 1,
      aux_sym__multiplication_repeat1,
    STATE(43), 1,
      sym__mul_op,
    ACTIONS(62), 2,
      anon_sym_STAR,
      anon_sym_PERCENT,
    ACTIONS(68), 2,
      anon_sym_LT,
      anon_sym_GT,
    ACTIONS(66), 15,
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
  [628] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(72), 1,
      anon_sym_DOT,
    ACTIONS(74), 1,
      anon_sym_LPAREN,
    ACTIONS(76), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(70), 17,
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
  [662] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(36), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(31), 18,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_DOT,
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
  [691] = 14,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    ACTIONS(78), 1,
      anon_sym_RPAREN,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(147), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [742] = 3,
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
  [770] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(194), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [818] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(46), 3,
      anon_sym_LT,
      anon_sym_GT,
      anon_sym_SLASH,
    ACTIONS(44), 17,
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
  [846] = 3,
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
  [874] = 3,
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
  [902] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(200), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [950] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(238), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [998] = 3,
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
  [1026] = 3,
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
  [1054] = 13,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(85), 1,
      sym__logical_and,
    STATE(118), 1,
      sym__logical_or,
    STATE(258), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1102] = 3,
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
  [1130] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(30), 1,
      aux_sym__addition_repeat1,
    STATE(40), 1,
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
  [1163] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(31), 1,
      aux_sym__addition_repeat1,
    STATE(40), 1,
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
  [1196] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(31), 1,
      aux_sym__addition_repeat1,
    STATE(40), 1,
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
  [1229] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(73), 1,
      sym__comparison,
    STATE(92), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1271] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(35), 1,
      aux_sym__comparison_repeat1,
    STATE(38), 1,
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
  [1302] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(34), 1,
      aux_sym__comparison_repeat1,
    STATE(38), 1,
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
  [1333] = 6,
    ACTIONS(3), 1,
      sym_comment,
    STATE(34), 1,
      aux_sym__comparison_repeat1,
    STATE(38), 1,
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
    ACTIONS(135), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [1364] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(33), 1,
      sym__addition,
    STATE(79), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1403] = 3,
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
  [1428] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(29), 1,
      sym__multiplication,
    STATE(39), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1464] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(137), 2,
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
  [1487] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_LPAREN,
    ACTIONS(25), 1,
      sym_identifier,
    STATE(37), 1,
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
    STATE(13), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1520] = 4,
    ACTIONS(3), 1,
      sym_comment,
    STATE(164), 1,
      sym_input_type,
    ACTIONS(139), 2,
      anon_sym_text,
      anon_sym_date,
    ACTIONS(141), 12,
      anon_sym_textarea,
      anon_sym_password,
      anon_sym_email,
      anon_sym_number,
      anon_sym_time,
      anon_sym_datetime,
      anon_sym_dropdown,
      anon_sym_radio,
      anon_sym_checkbox,
      anon_sym_toggle,
      anon_sym_file,
      anon_sym_hidden,
  [1545] = 7,
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
    ACTIONS(143), 2,
      sym_string,
      sym_number,
    STATE(25), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1575] = 7,
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
    ACTIONS(145), 2,
      sym_string,
      sym_number,
    STATE(20), 6,
      sym__unary,
      sym__primary,
      sym_call_expr,
      sym_field_expr,
      sym_group_expr,
      sym_boolean,
  [1605] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(147), 1,
      anon_sym_RBRACE,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(153), 1,
      anon_sym_params,
    ACTIONS(155), 1,
      anon_sym_label,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    STATE(47), 1,
      sym_params_block,
    STATE(62), 1,
      sym_label_declaration,
    STATE(56), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1643] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(153), 1,
      anon_sym_params,
    ACTIONS(155), 1,
      anon_sym_label,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(161), 1,
      anon_sym_RBRACE,
    STATE(50), 1,
      sym_params_block,
    STATE(61), 1,
      sym_label_declaration,
    STATE(57), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1681] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(145), 1,
      sym_event_type,
    ACTIONS(163), 11,
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
  [1701] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(155), 1,
      anon_sym_label,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(165), 1,
      anon_sym_RBRACE,
    STATE(60), 1,
      sym_label_declaration,
    STATE(58), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1733] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(167), 1,
      anon_sym_RBRACE,
    ACTIONS(169), 1,
      anon_sym_column,
    ACTIONS(172), 1,
      anon_sym_field,
    ACTIONS(175), 1,
      anon_sym_chart,
    ACTIONS(178), 1,
      anon_sym_on,
    ACTIONS(181), 1,
      sym_identifier,
    STATE(48), 6,
      sym_column_decl,
      sym_field_decl,
      sym_chart_decl,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [1763] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(162), 1,
      sym_event_type,
    ACTIONS(163), 11,
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
  [1783] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(155), 1,
      anon_sym_label,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(184), 1,
      anon_sym_RBRACE,
    STATE(66), 1,
      sym_label_declaration,
    STATE(64), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1815] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(149), 1,
      sym_event_type,
    ACTIONS(163), 11,
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
  [1835] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(186), 1,
      anon_sym_RBRACE,
    ACTIONS(188), 1,
      anon_sym_column,
    ACTIONS(190), 1,
      anon_sym_field,
    ACTIONS(192), 1,
      anon_sym_chart,
    ACTIONS(194), 1,
      anon_sym_on,
    ACTIONS(196), 1,
      sym_identifier,
    STATE(48), 6,
      sym_column_decl,
      sym_field_decl,
      sym_chart_decl,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [1865] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(188), 1,
      anon_sym_column,
    ACTIONS(190), 1,
      anon_sym_field,
    ACTIONS(192), 1,
      anon_sym_chart,
    ACTIONS(194), 1,
      anon_sym_on,
    ACTIONS(196), 1,
      sym_identifier,
    ACTIONS(198), 1,
      anon_sym_RBRACE,
    STATE(52), 6,
      sym_column_decl,
      sym_field_decl,
      sym_chart_decl,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_component_body_repeat1,
  [1895] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(200), 1,
      ts_builtin_sym_end,
    ACTIONS(202), 1,
      anon_sym_domain,
    ACTIONS(205), 1,
      anon_sym_view,
    ACTIONS(208), 1,
      anon_sym_action,
    ACTIONS(211), 1,
      anon_sym_module,
    STATE(54), 6,
      sym__definition,
      sym_domain_declaration,
      sym_view_declaration,
      sym_action_declaration,
      sym_module_declaration,
      aux_sym_source_file_repeat1,
  [1922] = 7,
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
    ACTIONS(214), 1,
      ts_builtin_sym_end,
    STATE(54), 6,
      sym__definition,
      sym_domain_declaration,
      sym_view_declaration,
      sym_action_declaration,
      sym_module_declaration,
      aux_sym_source_file_repeat1,
  [1949] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(165), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [1975] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(184), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2001] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(216), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2027] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(218), 1,
      anon_sym_RBRACE,
    ACTIONS(220), 1,
      anon_sym_container,
    ACTIONS(223), 1,
      anon_sym_component,
    ACTIONS(226), 1,
      anon_sym_on,
    ACTIONS(229), 1,
      sym_identifier,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2053] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(216), 1,
      anon_sym_RBRACE,
    STATE(67), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2079] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(184), 1,
      anon_sym_RBRACE,
    STATE(64), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2105] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(165), 1,
      anon_sym_RBRACE,
    STATE(58), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2131] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(232), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2157] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(234), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2183] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(236), 1,
      anon_sym_RBRACE,
    STATE(63), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2209] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(234), 1,
      anon_sym_RBRACE,
    STATE(68), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2235] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(238), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2261] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(149), 1,
      anon_sym_container,
    ACTIONS(151), 1,
      anon_sym_component,
    ACTIONS(157), 1,
      anon_sym_on,
    ACTIONS(159), 1,
      sym_identifier,
    ACTIONS(240), 1,
      anon_sym_RBRACE,
    STATE(59), 5,
      sym_container_declaration,
      sym_component_declaration,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_module_declaration_repeat1,
  [2287] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(272), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2311] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(273), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2335] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(252), 1,
      anon_sym_AMP_AMP,
    STATE(71), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(250), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2353] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(261), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2377] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(257), 1,
      anon_sym_AMP_AMP,
    STATE(75), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(255), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2395] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(276), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2419] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(257), 1,
      anon_sym_AMP_AMP,
    STATE(71), 1,
      aux_sym__logical_and_repeat1,
    ACTIONS(259), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2437] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(275), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2461] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(181), 1,
      sym_type_ref,
    ACTIONS(261), 7,
      anon_sym_Uuid,
      anon_sym_String,
      anon_sym_Int,
      anon_sym_Float,
      anon_sym_Boolean,
      anon_sym_DateTime,
      sym_identifier,
  [2477] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(242), 1,
      anon_sym_action,
    ACTIONS(244), 1,
      anon_sym_navigate,
    ACTIONS(246), 1,
      anon_sym_refresh,
    ACTIONS(248), 1,
      sym_stay_statement,
    STATE(249), 1,
      sym_event_action,
    STATE(262), 3,
      sym_navigate_action,
      sym_refresh_action,
      sym_action_invocation,
  [2501] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(250), 7,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
      anon_sym_AMP_AMP,
  [2514] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(265), 1,
      anon_sym_PIPE_PIPE,
    STATE(80), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(263), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2531] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(268), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
    ACTIONS(270), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2546] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(272), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
    ACTIONS(274), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2561] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(276), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
    ACTIONS(278), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2576] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(280), 1,
      anon_sym_SEMI,
    ACTIONS(282), 1,
      anon_sym_RBRACE,
    ACTIONS(284), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2593] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(288), 1,
      anon_sym_PIPE_PIPE,
    STATE(86), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(286), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2610] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(288), 1,
      anon_sym_PIPE_PIPE,
    STATE(80), 1,
      aux_sym__logical_or_repeat1,
    ACTIONS(290), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [2627] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(292), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
    ACTIONS(294), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2642] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(296), 1,
      anon_sym_SEMI,
    ACTIONS(298), 1,
      anon_sym_RBRACE,
    ACTIONS(300), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2659] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(302), 1,
      anon_sym_RBRACE,
    ACTIONS(304), 1,
      anon_sym_on,
    ACTIONS(306), 1,
      sym_identifier,
    STATE(94), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_action_body_repeat1,
  [2677] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(298), 1,
      anon_sym_RBRACE,
    ACTIONS(300), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2691] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(308), 1,
      anon_sym_RBRACE,
    ACTIONS(310), 5,
      anon_sym_container,
      anon_sym_component,
      anon_sym_label,
      anon_sym_on,
      sym_identifier,
  [2705] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(263), 6,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
      anon_sym_PIPE_PIPE,
  [2717] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(304), 1,
      anon_sym_on,
    ACTIONS(306), 1,
      sym_identifier,
    ACTIONS(312), 1,
      anon_sym_RBRACE,
    STATE(89), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_action_body_repeat1,
  [2735] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(314), 1,
      anon_sym_RBRACE,
    ACTIONS(316), 1,
      anon_sym_on,
    ACTIONS(319), 1,
      sym_identifier,
    STATE(94), 3,
      sym_property_assignment,
      sym_event_handler,
      aux_sym_action_body_repeat1,
  [2753] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(322), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2765] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(324), 1,
      anon_sym_RBRACE,
    ACTIONS(326), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2779] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(328), 6,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
      anon_sym_RPAREN,
  [2791] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_RBRACE,
    ACTIONS(332), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2805] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(334), 1,
      anon_sym_field,
    ACTIONS(336), 1,
      anon_sym_lookup,
    ACTIONS(338), 1,
      anon_sym_expr,
    STATE(213), 3,
      sym_field_ref,
      sym_lookup_ref,
      sym_expr_ref,
  [2823] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(340), 1,
      anon_sym_RBRACE,
    ACTIONS(342), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2837] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(344), 1,
      anon_sym_RBRACE,
    ACTIONS(346), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2851] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(282), 1,
      anon_sym_RBRACE,
    ACTIONS(284), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2865] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(348), 1,
      anon_sym_RBRACE,
    ACTIONS(350), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2879] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 1,
      anon_sym_RBRACE,
    ACTIONS(354), 5,
      anon_sym_column,
      anon_sym_field,
      anon_sym_chart,
      anon_sym_on,
      sym_identifier,
  [2893] = 3,
    ACTIONS(3), 1,
      sym_comment,
    STATE(140), 1,
      sym_chart_kind,
    ACTIONS(356), 5,
      anon_sym_bar,
      anon_sym_line,
      anon_sym_pie,
      anon_sym_radar,
      anon_sym_metric,
  [2907] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(340), 1,
      anon_sym_RBRACE,
    ACTIONS(342), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2920] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(358), 1,
      anon_sym_RBRACE,
    ACTIONS(360), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2933] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(362), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2944] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 1,
      anon_sym_RBRACE,
    ACTIONS(354), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2957] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(364), 1,
      anon_sym_RBRACE,
    ACTIONS(366), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [2970] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2981] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(370), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [2992] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_RBRACE,
    ACTIONS(332), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3005] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(372), 1,
      anon_sym_RBRACE,
    ACTIONS(374), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3018] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(376), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3029] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(378), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3040] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3051] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(382), 5,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
      anon_sym_RPAREN,
  [3062] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(384), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3073] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(386), 1,
      anon_sym_RBRACE,
    ACTIONS(388), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3086] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(390), 1,
      anon_sym_RBRACE,
    ACTIONS(392), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3099] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(394), 1,
      anon_sym_RBRACE,
    ACTIONS(396), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3112] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(398), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3123] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(378), 1,
      anon_sym_RBRACE,
    ACTIONS(400), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3136] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 1,
      anon_sym_RBRACE,
    ACTIONS(404), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3149] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(406), 5,
      ts_builtin_sym_end,
      anon_sym_domain,
      anon_sym_view,
      anon_sym_action,
      anon_sym_module,
  [3160] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_RBRACE,
    ACTIONS(408), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3173] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(410), 1,
      anon_sym_RBRACE,
    ACTIONS(412), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3186] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(398), 1,
      anon_sym_RBRACE,
    ACTIONS(414), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3199] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(406), 1,
      anon_sym_RBRACE,
    ACTIONS(416), 4,
      anon_sym_container,
      anon_sym_component,
      anon_sym_on,
      sym_identifier,
  [3212] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(418), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3222] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(420), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3232] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(422), 1,
      anon_sym_RBRACE,
    ACTIONS(424), 1,
      sym_identifier,
    STATE(134), 2,
      sym_property_assignment,
      aux_sym_input_body_repeat1,
  [3246] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(426), 1,
      anon_sym_RBRACE,
    ACTIONS(428), 1,
      sym_identifier,
    STATE(134), 2,
      sym_property_assignment,
      aux_sym_input_body_repeat1,
  [3260] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(431), 4,
      anon_sym_SEMI,
      anon_sym_RBRACE,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3270] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(433), 1,
      anon_sym_SEMI,
    ACTIONS(435), 1,
      anon_sym_RBRACE,
    ACTIONS(437), 1,
      sym_identifier,
    STATE(160), 1,
      sym_object_member,
  [3286] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      sym_identifier,
    ACTIONS(439), 1,
      anon_sym_RBRACE,
    STATE(133), 2,
      sym_property_assignment,
      aux_sym_input_body_repeat1,
  [3300] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      sym_identifier,
    ACTIONS(441), 1,
      anon_sym_RBRACE,
    STATE(134), 2,
      sym_property_assignment,
      aux_sym_input_body_repeat1,
  [3314] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      sym_identifier,
    ACTIONS(443), 1,
      anon_sym_RBRACE,
    STATE(138), 2,
      sym_property_assignment,
      aux_sym_input_body_repeat1,
  [3328] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(445), 1,
      anon_sym_LBRACE,
    ACTIONS(447), 1,
      anon_sym_SEMI,
    STATE(88), 1,
      sym_chart_body,
  [3341] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(449), 1,
      anon_sym_SEMI,
    ACTIONS(452), 1,
      anon_sym_RBRACE,
    STATE(141), 1,
      aux_sym_object_literal_repeat1,
  [3354] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(454), 1,
      anon_sym_RBRACE,
    ACTIONS(456), 1,
      anon_sym_COMMA,
    STATE(144), 1,
      aux_sym_parameter_binding_repeat1,
  [3367] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(458), 1,
      anon_sym_COMMA,
    ACTIONS(460), 1,
      anon_sym_RBRACK,
    STATE(177), 1,
      aux_sym_array_literal_repeat1,
  [3380] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(462), 1,
      anon_sym_RBRACE,
    ACTIONS(464), 1,
      anon_sym_COMMA,
    STATE(144), 1,
      aux_sym_parameter_binding_repeat1,
  [3393] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(467), 1,
      anon_sym_DASH_GT,
    ACTIONS(469), 1,
      anon_sym_LPAREN,
    STATE(290), 1,
      sym_event_param,
  [3406] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(471), 1,
      anon_sym_RBRACE,
    ACTIONS(473), 1,
      anon_sym_COMMA,
    STATE(146), 1,
      aux_sym_parameter_block_repeat1,
  [3419] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(476), 1,
      anon_sym_COMMA,
    ACTIONS(478), 1,
      anon_sym_RPAREN,
    STATE(176), 1,
      aux_sym_call_expr_repeat1,
  [3432] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(480), 1,
      anon_sym_COMMA,
    ACTIONS(482), 1,
      anon_sym_RPAREN,
    STATE(169), 1,
      aux_sym_event_param_repeat1,
  [3445] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(469), 1,
      anon_sym_LPAREN,
    ACTIONS(484), 1,
      anon_sym_DASH_GT,
    STATE(287), 1,
      sym_event_param,
  [3458] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(486), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3467] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 1,
      anon_sym_RBRACE,
    ACTIONS(354), 2,
      anon_sym_on,
      sym_identifier,
  [3478] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(437), 1,
      sym_identifier,
    ACTIONS(488), 1,
      anon_sym_RBRACE,
    STATE(182), 1,
      sym_object_member,
  [3491] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(488), 1,
      anon_sym_RBRACE,
    ACTIONS(490), 1,
      anon_sym_SEMI,
    STATE(141), 1,
      aux_sym_object_literal_repeat1,
  [3504] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(456), 1,
      anon_sym_COMMA,
    ACTIONS(492), 1,
      anon_sym_RBRACE,
    STATE(142), 1,
      aux_sym_parameter_binding_repeat1,
  [3517] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(494), 1,
      anon_sym_RBRACE,
    ACTIONS(496), 1,
      sym_identifier,
    STATE(168), 1,
      sym_parameter_decl,
  [3530] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(340), 1,
      anon_sym_RBRACE,
    ACTIONS(342), 2,
      anon_sym_on,
      sym_identifier,
  [3541] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(498), 1,
      anon_sym_RBRACE,
    ACTIONS(500), 1,
      sym_identifier,
    STATE(154), 1,
      sym_binding_pair,
  [3554] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_RBRACE,
    ACTIONS(332), 2,
      anon_sym_on,
      sym_identifier,
  [3565] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(458), 1,
      anon_sym_COMMA,
    ACTIONS(502), 1,
      anon_sym_RBRACK,
    STATE(143), 1,
      aux_sym_array_literal_repeat1,
  [3578] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(504), 1,
      anon_sym_SEMI,
    ACTIONS(506), 1,
      anon_sym_RBRACE,
    STATE(153), 1,
      aux_sym_object_literal_repeat1,
  [3591] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(508), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3600] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(469), 1,
      anon_sym_LPAREN,
    ACTIONS(510), 1,
      anon_sym_DASH_GT,
    STATE(270), 1,
      sym_event_param,
  [3613] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(480), 1,
      anon_sym_COMMA,
    ACTIONS(512), 1,
      anon_sym_RPAREN,
    STATE(148), 1,
      aux_sym_event_param_repeat1,
  [3626] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(514), 1,
      anon_sym_LBRACE,
    ACTIONS(516), 1,
      anon_sym_SEMI,
    STATE(84), 1,
      sym_input_body,
  [3639] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(518), 1,
      anon_sym_COMMA,
    ACTIONS(521), 1,
      anon_sym_RPAREN,
    STATE(165), 1,
      aux_sym_call_expr_repeat1,
  [3652] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(523), 1,
      anon_sym_RBRACE,
    ACTIONS(525), 1,
      anon_sym_COMMA,
    STATE(146), 1,
      aux_sym_parameter_block_repeat1,
  [3665] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(527), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3674] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(525), 1,
      anon_sym_COMMA,
    ACTIONS(529), 1,
      anon_sym_RBRACE,
    STATE(170), 1,
      aux_sym_parameter_block_repeat1,
  [3687] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(531), 1,
      anon_sym_COMMA,
    ACTIONS(534), 1,
      anon_sym_RPAREN,
    STATE(169), 1,
      aux_sym_event_param_repeat1,
  [3700] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(525), 1,
      anon_sym_COMMA,
    ACTIONS(536), 1,
      anon_sym_RBRACE,
    STATE(146), 1,
      aux_sym_parameter_block_repeat1,
  [3713] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(525), 1,
      anon_sym_COMMA,
    ACTIONS(538), 1,
      anon_sym_RBRACE,
    STATE(166), 1,
      aux_sym_parameter_block_repeat1,
  [3726] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(540), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3735] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(542), 3,
      anon_sym_SEMI,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3744] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(496), 1,
      sym_identifier,
    ACTIONS(544), 1,
      anon_sym_RBRACE,
    STATE(171), 1,
      sym_parameter_decl,
  [3757] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(437), 1,
      sym_identifier,
    ACTIONS(546), 1,
      anon_sym_RBRACE,
    STATE(182), 1,
      sym_object_member,
  [3770] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(476), 1,
      anon_sym_COMMA,
    ACTIONS(548), 1,
      anon_sym_RPAREN,
    STATE(165), 1,
      aux_sym_call_expr_repeat1,
  [3783] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(550), 1,
      anon_sym_COMMA,
    ACTIONS(553), 1,
      anon_sym_RBRACK,
    STATE(177), 1,
      aux_sym_array_literal_repeat1,
  [3796] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(496), 1,
      sym_identifier,
    STATE(211), 1,
      sym_parameter_decl,
  [3806] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(553), 2,
      anon_sym_COMMA,
      anon_sym_RBRACK,
  [3814] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3822] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(555), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [3830] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(452), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3838] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(557), 1,
      anon_sym_LBRACE,
    STATE(119), 1,
      sym_action_body,
  [3848] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(559), 1,
      anon_sym_LBRACE,
    STATE(65), 1,
      sym_parameter_block,
  [3858] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(561), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3866] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(563), 2,
      anon_sym_SEMI,
      anon_sym_RBRACE,
  [3874] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(534), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3882] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(557), 1,
      anon_sym_LBRACE,
    STATE(216), 1,
      sym_action_body,
  [3892] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(565), 1,
      anon_sym_LBRACE,
    STATE(218), 1,
      sym_parameter_binding,
  [3902] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(565), 1,
      anon_sym_LBRACE,
    STATE(219), 1,
      sym_parameter_binding,
  [3912] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(567), 1,
      anon_sym_COMMA,
    ACTIONS(569), 1,
      anon_sym_RPAREN,
  [3922] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(437), 1,
      sym_identifier,
    STATE(182), 1,
      sym_object_member,
  [3932] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(571), 1,
      anon_sym_LBRACE,
    STATE(108), 1,
      sym_view_body,
  [3942] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(521), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [3950] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(573), 1,
      anon_sym_COMMA,
    ACTIONS(575), 1,
      anon_sym_RPAREN,
  [3960] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(577), 1,
      anon_sym_COMMA,
    ACTIONS(579), 1,
      anon_sym_RPAREN,
  [3970] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(410), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [3978] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(581), 2,
      anon_sym_LBRACE,
      anon_sym_SEMI,
  [3986] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 2,
      anon_sym_RBRACE,
      sym_identifier,
  [3994] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(583), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [4002] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(585), 2,
      anon_sym_DASH_GT,
      anon_sym_LPAREN,
  [4010] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(462), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [4018] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(587), 2,
      anon_sym_LBRACE,
      anon_sym_SEMI,
  [4026] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(589), 1,
      anon_sym_LBRACE,
    STATE(120), 1,
      sym_component_body,
  [4036] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(591), 1,
      anon_sym_LBRACE,
    STATE(121), 1,
      sym_view_body,
  [4046] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(593), 1,
      anon_sym_LBRACE,
    STATE(286), 1,
      sym_parameter_block,
  [4056] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(595), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [4064] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(593), 1,
      anon_sym_LBRACE,
    STATE(282), 1,
      sym_parameter_block,
  [4074] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(358), 2,
      anon_sym_SEMI,
      anon_sym_output,
  [4082] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(500), 1,
      sym_identifier,
    STATE(202), 1,
      sym_binding_pair,
  [4092] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(471), 2,
      anon_sym_RBRACE,
      anon_sym_COMMA,
  [4100] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(597), 1,
      sym_string,
  [4107] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(599), 1,
      anon_sym_SEMI,
  [4114] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(601), 1,
      anon_sym_SEMI,
  [4121] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(603), 1,
      anon_sym_LBRACE,
  [4128] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(605), 1,
      anon_sym_RPAREN,
  [4135] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(607), 1,
      sym_identifier,
  [4142] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(609), 1,
      anon_sym_RPAREN,
  [4149] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(611), 1,
      anon_sym_RPAREN,
  [4156] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(613), 1,
      sym_identifier,
  [4163] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(615), 1,
      sym_identifier,
  [4170] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(617), 1,
      anon_sym_schema,
  [4177] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(619), 1,
      anon_sym_LBRACE,
  [4184] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(621), 1,
      sym_identifier,
  [4191] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(623), 1,
      anon_sym_SEMI,
  [4198] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(625), 1,
      anon_sym_RPAREN,
  [4205] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(627), 1,
      anon_sym_COLON,
  [4212] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(629), 1,
      sym_identifier,
  [4219] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(631), 1,
      anon_sym_SEMI,
  [4226] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(633), 1,
      anon_sym_SEMI,
  [4233] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(635), 1,
      anon_sym_SEMI,
  [4240] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(637), 1,
      anon_sym_via,
  [4247] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(639), 1,
      anon_sym_SEMI,
  [4254] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(641), 1,
      anon_sym_DOT,
  [4261] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(643), 1,
      anon_sym_DOT,
  [4268] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(645), 1,
      ts_builtin_sym_end,
  [4275] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(647), 1,
      anon_sym_RPAREN,
  [4282] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(649), 1,
      anon_sym_SEMI,
  [4289] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(651), 1,
      anon_sym_DASH_GT,
  [4296] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(653), 1,
      sym_identifier,
  [4303] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(655), 1,
      anon_sym_SEMI,
  [4310] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(657), 1,
      sym_string,
  [4317] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(659), 1,
      sym_string,
  [4324] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(661), 1,
      anon_sym_RPAREN,
  [4331] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(663), 1,
      sym_string,
  [4338] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(665), 1,
      anon_sym_SEMI,
  [4345] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(667), 1,
      anon_sym_input,
  [4352] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(669), 1,
      sym_string,
  [4359] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(671), 1,
      anon_sym_SEMI,
  [4366] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(673), 1,
      anon_sym_DASH_GT,
  [4373] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(675), 1,
      sym_identifier,
  [4380] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(677), 1,
      sym_string,
  [4387] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(679), 1,
      sym_string,
  [4394] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(681), 1,
      sym_string,
  [4401] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(683), 1,
      anon_sym_DASH_GT,
  [4408] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(685), 1,
      anon_sym_DASH_GT,
  [4415] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(687), 1,
      sym_identifier,
  [4422] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(689), 1,
      anon_sym_RPAREN,
  [4429] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(691), 1,
      anon_sym_COLON,
  [4436] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(506), 1,
      anon_sym_RBRACE,
  [4443] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(693), 1,
      anon_sym_SEMI,
  [4450] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(695), 1,
      anon_sym_SEMI,
  [4457] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(697), 1,
      anon_sym_LPAREN,
  [4464] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(699), 1,
      anon_sym_LPAREN,
  [4471] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(701), 1,
      anon_sym_LPAREN,
  [4478] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(703), 1,
      sym_identifier,
  [4485] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(705), 1,
      sym_string,
  [4492] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(707), 1,
      anon_sym_SEMI,
  [4499] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(709), 1,
      anon_sym_SEMI,
  [4506] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(711), 1,
      anon_sym_DASH_GT,
  [4513] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(713), 1,
      sym_identifier,
  [4520] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(715), 1,
      anon_sym_SEMI,
  [4527] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(717), 1,
      anon_sym_SEMI,
  [4534] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(719), 1,
      anon_sym_SEMI,
  [4541] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(721), 1,
      anon_sym_SEMI,
  [4548] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(723), 1,
      anon_sym_SEMI,
  [4555] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(725), 1,
      anon_sym_SEMI,
  [4562] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(727), 1,
      sym_string,
  [4569] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(729), 1,
      anon_sym_COLON,
  [4576] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(731), 1,
      anon_sym_RBRACE,
  [4583] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(733), 1,
      sym_string,
  [4590] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(735), 1,
      anon_sym_output,
  [4597] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(737), 1,
      anon_sym_SEMI,
  [4604] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(739), 1,
      sym_string,
  [4611] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(741), 1,
      anon_sym_COLON,
  [4618] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(743), 1,
      anon_sym_SEMI,
  [4625] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(745), 1,
      anon_sym_DASH_GT,
  [4632] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(747), 1,
      anon_sym_COLON,
  [4639] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(749), 1,
      anon_sym_SEMI,
  [4646] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(751), 1,
      anon_sym_DASH_GT,
  [4653] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(753), 1,
      anon_sym_COLON,
  [4660] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(755), 1,
      anon_sym_COLON,
  [4667] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(757), 1,
      anon_sym_input,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 62,
  [SMALL_STATE(4)] = 121,
  [SMALL_STATE(5)] = 180,
  [SMALL_STATE(6)] = 239,
  [SMALL_STATE(7)] = 298,
  [SMALL_STATE(8)] = 357,
  [SMALL_STATE(9)] = 412,
  [SMALL_STATE(10)] = 446,
  [SMALL_STATE(11)] = 480,
  [SMALL_STATE(12)] = 518,
  [SMALL_STATE(13)] = 552,
  [SMALL_STATE(14)] = 590,
  [SMALL_STATE(15)] = 628,
  [SMALL_STATE(16)] = 662,
  [SMALL_STATE(17)] = 691,
  [SMALL_STATE(18)] = 742,
  [SMALL_STATE(19)] = 770,
  [SMALL_STATE(20)] = 818,
  [SMALL_STATE(21)] = 846,
  [SMALL_STATE(22)] = 874,
  [SMALL_STATE(23)] = 902,
  [SMALL_STATE(24)] = 950,
  [SMALL_STATE(25)] = 998,
  [SMALL_STATE(26)] = 1026,
  [SMALL_STATE(27)] = 1054,
  [SMALL_STATE(28)] = 1102,
  [SMALL_STATE(29)] = 1130,
  [SMALL_STATE(30)] = 1163,
  [SMALL_STATE(31)] = 1196,
  [SMALL_STATE(32)] = 1229,
  [SMALL_STATE(33)] = 1271,
  [SMALL_STATE(34)] = 1302,
  [SMALL_STATE(35)] = 1333,
  [SMALL_STATE(36)] = 1364,
  [SMALL_STATE(37)] = 1403,
  [SMALL_STATE(38)] = 1428,
  [SMALL_STATE(39)] = 1464,
  [SMALL_STATE(40)] = 1487,
  [SMALL_STATE(41)] = 1520,
  [SMALL_STATE(42)] = 1545,
  [SMALL_STATE(43)] = 1575,
  [SMALL_STATE(44)] = 1605,
  [SMALL_STATE(45)] = 1643,
  [SMALL_STATE(46)] = 1681,
  [SMALL_STATE(47)] = 1701,
  [SMALL_STATE(48)] = 1733,
  [SMALL_STATE(49)] = 1763,
  [SMALL_STATE(50)] = 1783,
  [SMALL_STATE(51)] = 1815,
  [SMALL_STATE(52)] = 1835,
  [SMALL_STATE(53)] = 1865,
  [SMALL_STATE(54)] = 1895,
  [SMALL_STATE(55)] = 1922,
  [SMALL_STATE(56)] = 1949,
  [SMALL_STATE(57)] = 1975,
  [SMALL_STATE(58)] = 2001,
  [SMALL_STATE(59)] = 2027,
  [SMALL_STATE(60)] = 2053,
  [SMALL_STATE(61)] = 2079,
  [SMALL_STATE(62)] = 2105,
  [SMALL_STATE(63)] = 2131,
  [SMALL_STATE(64)] = 2157,
  [SMALL_STATE(65)] = 2183,
  [SMALL_STATE(66)] = 2209,
  [SMALL_STATE(67)] = 2235,
  [SMALL_STATE(68)] = 2261,
  [SMALL_STATE(69)] = 2287,
  [SMALL_STATE(70)] = 2311,
  [SMALL_STATE(71)] = 2335,
  [SMALL_STATE(72)] = 2353,
  [SMALL_STATE(73)] = 2377,
  [SMALL_STATE(74)] = 2395,
  [SMALL_STATE(75)] = 2419,
  [SMALL_STATE(76)] = 2437,
  [SMALL_STATE(77)] = 2461,
  [SMALL_STATE(78)] = 2477,
  [SMALL_STATE(79)] = 2501,
  [SMALL_STATE(80)] = 2514,
  [SMALL_STATE(81)] = 2531,
  [SMALL_STATE(82)] = 2546,
  [SMALL_STATE(83)] = 2561,
  [SMALL_STATE(84)] = 2576,
  [SMALL_STATE(85)] = 2593,
  [SMALL_STATE(86)] = 2610,
  [SMALL_STATE(87)] = 2627,
  [SMALL_STATE(88)] = 2642,
  [SMALL_STATE(89)] = 2659,
  [SMALL_STATE(90)] = 2677,
  [SMALL_STATE(91)] = 2691,
  [SMALL_STATE(92)] = 2705,
  [SMALL_STATE(93)] = 2717,
  [SMALL_STATE(94)] = 2735,
  [SMALL_STATE(95)] = 2753,
  [SMALL_STATE(96)] = 2765,
  [SMALL_STATE(97)] = 2779,
  [SMALL_STATE(98)] = 2791,
  [SMALL_STATE(99)] = 2805,
  [SMALL_STATE(100)] = 2823,
  [SMALL_STATE(101)] = 2837,
  [SMALL_STATE(102)] = 2851,
  [SMALL_STATE(103)] = 2865,
  [SMALL_STATE(104)] = 2879,
  [SMALL_STATE(105)] = 2893,
  [SMALL_STATE(106)] = 2907,
  [SMALL_STATE(107)] = 2920,
  [SMALL_STATE(108)] = 2933,
  [SMALL_STATE(109)] = 2944,
  [SMALL_STATE(110)] = 2957,
  [SMALL_STATE(111)] = 2970,
  [SMALL_STATE(112)] = 2981,
  [SMALL_STATE(113)] = 2992,
  [SMALL_STATE(114)] = 3005,
  [SMALL_STATE(115)] = 3018,
  [SMALL_STATE(116)] = 3029,
  [SMALL_STATE(117)] = 3040,
  [SMALL_STATE(118)] = 3051,
  [SMALL_STATE(119)] = 3062,
  [SMALL_STATE(120)] = 3073,
  [SMALL_STATE(121)] = 3086,
  [SMALL_STATE(122)] = 3099,
  [SMALL_STATE(123)] = 3112,
  [SMALL_STATE(124)] = 3123,
  [SMALL_STATE(125)] = 3136,
  [SMALL_STATE(126)] = 3149,
  [SMALL_STATE(127)] = 3160,
  [SMALL_STATE(128)] = 3173,
  [SMALL_STATE(129)] = 3186,
  [SMALL_STATE(130)] = 3199,
  [SMALL_STATE(131)] = 3212,
  [SMALL_STATE(132)] = 3222,
  [SMALL_STATE(133)] = 3232,
  [SMALL_STATE(134)] = 3246,
  [SMALL_STATE(135)] = 3260,
  [SMALL_STATE(136)] = 3270,
  [SMALL_STATE(137)] = 3286,
  [SMALL_STATE(138)] = 3300,
  [SMALL_STATE(139)] = 3314,
  [SMALL_STATE(140)] = 3328,
  [SMALL_STATE(141)] = 3341,
  [SMALL_STATE(142)] = 3354,
  [SMALL_STATE(143)] = 3367,
  [SMALL_STATE(144)] = 3380,
  [SMALL_STATE(145)] = 3393,
  [SMALL_STATE(146)] = 3406,
  [SMALL_STATE(147)] = 3419,
  [SMALL_STATE(148)] = 3432,
  [SMALL_STATE(149)] = 3445,
  [SMALL_STATE(150)] = 3458,
  [SMALL_STATE(151)] = 3467,
  [SMALL_STATE(152)] = 3478,
  [SMALL_STATE(153)] = 3491,
  [SMALL_STATE(154)] = 3504,
  [SMALL_STATE(155)] = 3517,
  [SMALL_STATE(156)] = 3530,
  [SMALL_STATE(157)] = 3541,
  [SMALL_STATE(158)] = 3554,
  [SMALL_STATE(159)] = 3565,
  [SMALL_STATE(160)] = 3578,
  [SMALL_STATE(161)] = 3591,
  [SMALL_STATE(162)] = 3600,
  [SMALL_STATE(163)] = 3613,
  [SMALL_STATE(164)] = 3626,
  [SMALL_STATE(165)] = 3639,
  [SMALL_STATE(166)] = 3652,
  [SMALL_STATE(167)] = 3665,
  [SMALL_STATE(168)] = 3674,
  [SMALL_STATE(169)] = 3687,
  [SMALL_STATE(170)] = 3700,
  [SMALL_STATE(171)] = 3713,
  [SMALL_STATE(172)] = 3726,
  [SMALL_STATE(173)] = 3735,
  [SMALL_STATE(174)] = 3744,
  [SMALL_STATE(175)] = 3757,
  [SMALL_STATE(176)] = 3770,
  [SMALL_STATE(177)] = 3783,
  [SMALL_STATE(178)] = 3796,
  [SMALL_STATE(179)] = 3806,
  [SMALL_STATE(180)] = 3814,
  [SMALL_STATE(181)] = 3822,
  [SMALL_STATE(182)] = 3830,
  [SMALL_STATE(183)] = 3838,
  [SMALL_STATE(184)] = 3848,
  [SMALL_STATE(185)] = 3858,
  [SMALL_STATE(186)] = 3866,
  [SMALL_STATE(187)] = 3874,
  [SMALL_STATE(188)] = 3882,
  [SMALL_STATE(189)] = 3892,
  [SMALL_STATE(190)] = 3902,
  [SMALL_STATE(191)] = 3912,
  [SMALL_STATE(192)] = 3922,
  [SMALL_STATE(193)] = 3932,
  [SMALL_STATE(194)] = 3942,
  [SMALL_STATE(195)] = 3950,
  [SMALL_STATE(196)] = 3960,
  [SMALL_STATE(197)] = 3970,
  [SMALL_STATE(198)] = 3978,
  [SMALL_STATE(199)] = 3986,
  [SMALL_STATE(200)] = 3994,
  [SMALL_STATE(201)] = 4002,
  [SMALL_STATE(202)] = 4010,
  [SMALL_STATE(203)] = 4018,
  [SMALL_STATE(204)] = 4026,
  [SMALL_STATE(205)] = 4036,
  [SMALL_STATE(206)] = 4046,
  [SMALL_STATE(207)] = 4056,
  [SMALL_STATE(208)] = 4064,
  [SMALL_STATE(209)] = 4074,
  [SMALL_STATE(210)] = 4082,
  [SMALL_STATE(211)] = 4092,
  [SMALL_STATE(212)] = 4100,
  [SMALL_STATE(213)] = 4107,
  [SMALL_STATE(214)] = 4114,
  [SMALL_STATE(215)] = 4121,
  [SMALL_STATE(216)] = 4128,
  [SMALL_STATE(217)] = 4135,
  [SMALL_STATE(218)] = 4142,
  [SMALL_STATE(219)] = 4149,
  [SMALL_STATE(220)] = 4156,
  [SMALL_STATE(221)] = 4163,
  [SMALL_STATE(222)] = 4170,
  [SMALL_STATE(223)] = 4177,
  [SMALL_STATE(224)] = 4184,
  [SMALL_STATE(225)] = 4191,
  [SMALL_STATE(226)] = 4198,
  [SMALL_STATE(227)] = 4205,
  [SMALL_STATE(228)] = 4212,
  [SMALL_STATE(229)] = 4219,
  [SMALL_STATE(230)] = 4226,
  [SMALL_STATE(231)] = 4233,
  [SMALL_STATE(232)] = 4240,
  [SMALL_STATE(233)] = 4247,
  [SMALL_STATE(234)] = 4254,
  [SMALL_STATE(235)] = 4261,
  [SMALL_STATE(236)] = 4268,
  [SMALL_STATE(237)] = 4275,
  [SMALL_STATE(238)] = 4282,
  [SMALL_STATE(239)] = 4289,
  [SMALL_STATE(240)] = 4296,
  [SMALL_STATE(241)] = 4303,
  [SMALL_STATE(242)] = 4310,
  [SMALL_STATE(243)] = 4317,
  [SMALL_STATE(244)] = 4324,
  [SMALL_STATE(245)] = 4331,
  [SMALL_STATE(246)] = 4338,
  [SMALL_STATE(247)] = 4345,
  [SMALL_STATE(248)] = 4352,
  [SMALL_STATE(249)] = 4359,
  [SMALL_STATE(250)] = 4366,
  [SMALL_STATE(251)] = 4373,
  [SMALL_STATE(252)] = 4380,
  [SMALL_STATE(253)] = 4387,
  [SMALL_STATE(254)] = 4394,
  [SMALL_STATE(255)] = 4401,
  [SMALL_STATE(256)] = 4408,
  [SMALL_STATE(257)] = 4415,
  [SMALL_STATE(258)] = 4422,
  [SMALL_STATE(259)] = 4429,
  [SMALL_STATE(260)] = 4436,
  [SMALL_STATE(261)] = 4443,
  [SMALL_STATE(262)] = 4450,
  [SMALL_STATE(263)] = 4457,
  [SMALL_STATE(264)] = 4464,
  [SMALL_STATE(265)] = 4471,
  [SMALL_STATE(266)] = 4478,
  [SMALL_STATE(267)] = 4485,
  [SMALL_STATE(268)] = 4492,
  [SMALL_STATE(269)] = 4499,
  [SMALL_STATE(270)] = 4506,
  [SMALL_STATE(271)] = 4513,
  [SMALL_STATE(272)] = 4520,
  [SMALL_STATE(273)] = 4527,
  [SMALL_STATE(274)] = 4534,
  [SMALL_STATE(275)] = 4541,
  [SMALL_STATE(276)] = 4548,
  [SMALL_STATE(277)] = 4555,
  [SMALL_STATE(278)] = 4562,
  [SMALL_STATE(279)] = 4569,
  [SMALL_STATE(280)] = 4576,
  [SMALL_STATE(281)] = 4583,
  [SMALL_STATE(282)] = 4590,
  [SMALL_STATE(283)] = 4597,
  [SMALL_STATE(284)] = 4604,
  [SMALL_STATE(285)] = 4611,
  [SMALL_STATE(286)] = 4618,
  [SMALL_STATE(287)] = 4625,
  [SMALL_STATE(288)] = 4632,
  [SMALL_STATE(289)] = 4639,
  [SMALL_STATE(290)] = 4646,
  [SMALL_STATE(291)] = 4653,
  [SMALL_STATE(292)] = 4660,
  [SMALL_STATE(293)] = 4667,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(245),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(284),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(281),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(278),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [19] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [21] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [23] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [25] = {.entry = {.count = 1, .reusable = false}}, SHIFT(15),
  [27] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [29] = {.entry = {.count = 1, .reusable = false}}, SHIFT(28),
  [31] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [33] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_field_expr_repeat1, 2), SHIFT_REPEAT(217),
  [36] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_field_expr_repeat1, 2),
  [38] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 4),
  [40] = {.entry = {.count = 1, .reusable = true}}, SHIFT(217),
  [42] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 4),
  [44] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [46] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2),
  [48] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(43),
  [51] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__multiplication_repeat1, 2), SHIFT_REPEAT(43),
  [54] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_expr, 3),
  [56] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_expr, 3),
  [58] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 1),
  [60] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 1),
  [62] = {.entry = {.count = 1, .reusable = true}}, SHIFT(43),
  [64] = {.entry = {.count = 1, .reusable = false}}, SHIFT(43),
  [66] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__multiplication, 2),
  [68] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__multiplication, 2),
  [70] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__primary, 1),
  [72] = {.entry = {.count = 1, .reusable = true}}, SHIFT(257),
  [74] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [76] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__primary, 1),
  [78] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [80] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 4),
  [82] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 4),
  [84] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 3),
  [86] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 3),
  [88] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_group_expr, 3),
  [90] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_group_expr, 3),
  [92] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__unary, 2),
  [94] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__unary, 2),
  [96] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_call_expr, 5),
  [98] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_call_expr, 5),
  [100] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_boolean, 1),
  [102] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_boolean, 1),
  [104] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 1),
  [106] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 1),
  [108] = {.entry = {.count = 1, .reusable = true}}, SHIFT(40),
  [110] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__addition, 2),
  [112] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym__addition, 2),
  [114] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2),
  [116] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__addition_repeat1, 2),
  [118] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__addition_repeat1, 2), SHIFT_REPEAT(40),
  [121] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 1),
  [123] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [125] = {.entry = {.count = 1, .reusable = false}}, SHIFT(38),
  [127] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [129] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(38),
  [132] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2), SHIFT_REPEAT(38),
  [135] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__comparison, 2),
  [137] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym__comparison_repeat1, 2),
  [139] = {.entry = {.count = 1, .reusable = false}}, SHIFT(198),
  [141] = {.entry = {.count = 1, .reusable = true}}, SHIFT(198),
  [143] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [145] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [147] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [149] = {.entry = {.count = 1, .reusable = false}}, SHIFT(243),
  [151] = {.entry = {.count = 1, .reusable = false}}, SHIFT(212),
  [153] = {.entry = {.count = 1, .reusable = false}}, SHIFT(206),
  [155] = {.entry = {.count = 1, .reusable = false}}, SHIFT(242),
  [157] = {.entry = {.count = 1, .reusable = false}}, SHIFT(49),
  [159] = {.entry = {.count = 1, .reusable = false}}, SHIFT(292),
  [161] = {.entry = {.count = 1, .reusable = true}}, SHIFT(130),
  [163] = {.entry = {.count = 1, .reusable = false}}, SHIFT(201),
  [165] = {.entry = {.count = 1, .reusable = true}}, SHIFT(123),
  [167] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_component_body_repeat1, 2),
  [169] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(267),
  [172] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(266),
  [175] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(105),
  [178] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(46),
  [181] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_component_body_repeat1, 2), SHIFT_REPEAT(288),
  [184] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [186] = {.entry = {.count = 1, .reusable = true}}, SHIFT(122),
  [188] = {.entry = {.count = 1, .reusable = false}}, SHIFT(267),
  [190] = {.entry = {.count = 1, .reusable = false}}, SHIFT(266),
  [192] = {.entry = {.count = 1, .reusable = false}}, SHIFT(105),
  [194] = {.entry = {.count = 1, .reusable = false}}, SHIFT(46),
  [196] = {.entry = {.count = 1, .reusable = false}}, SHIFT(288),
  [198] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [200] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2),
  [202] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(245),
  [205] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(284),
  [208] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(281),
  [211] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2), SHIFT_REPEAT(278),
  [214] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1),
  [216] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [218] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_module_declaration_repeat1, 2),
  [220] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(243),
  [223] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(212),
  [226] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(49),
  [229] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_module_declaration_repeat1, 2), SHIFT_REPEAT(292),
  [232] = {.entry = {.count = 1, .reusable = true}}, SHIFT(112),
  [234] = {.entry = {.count = 1, .reusable = true}}, SHIFT(127),
  [236] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [238] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [240] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [242] = {.entry = {.count = 1, .reusable = true}}, SHIFT(265),
  [244] = {.entry = {.count = 1, .reusable = true}}, SHIFT(264),
  [246] = {.entry = {.count = 1, .reusable = true}}, SHIFT(263),
  [248] = {.entry = {.count = 1, .reusable = true}}, SHIFT(262),
  [250] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2),
  [252] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_and_repeat1, 2), SHIFT_REPEAT(36),
  [255] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 1),
  [257] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [259] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_and, 2),
  [261] = {.entry = {.count = 1, .reusable = false}}, SHIFT(207),
  [263] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2),
  [265] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym__logical_or_repeat1, 2), SHIFT_REPEAT(32),
  [268] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_input_body, 3),
  [270] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_input_body, 3),
  [272] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chart_body, 2),
  [274] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chart_body, 2),
  [276] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_input_body, 2),
  [278] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_input_body, 2),
  [280] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [282] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_decl, 6),
  [284] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_decl, 6),
  [286] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 1),
  [288] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [290] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym__logical_or, 2),
  [292] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chart_body, 3),
  [294] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chart_body, 3),
  [296] = {.entry = {.count = 1, .reusable = true}}, SHIFT(103),
  [298] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chart_decl, 3),
  [300] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chart_decl, 3),
  [302] = {.entry = {.count = 1, .reusable = true}}, SHIFT(95),
  [304] = {.entry = {.count = 1, .reusable = false}}, SHIFT(51),
  [306] = {.entry = {.count = 1, .reusable = false}}, SHIFT(285),
  [308] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_params_block, 3),
  [310] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_params_block, 3),
  [312] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [314] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_action_body_repeat1, 2),
  [316] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_action_body_repeat1, 2), SHIFT_REPEAT(51),
  [319] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_action_body_repeat1, 2), SHIFT_REPEAT(285),
  [322] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 3),
  [324] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_decl, 7),
  [326] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_field_decl, 7),
  [328] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_body, 2),
  [330] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [332] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 6, .production_id = 4),
  [334] = {.entry = {.count = 1, .reusable = true}}, SHIFT(224),
  [336] = {.entry = {.count = 1, .reusable = true}}, SHIFT(228),
  [338] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [340] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [342] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_event_handler, 5, .production_id = 3),
  [344] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_column_decl, 5),
  [346] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_column_decl, 5),
  [348] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chart_decl, 4),
  [350] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_chart_decl, 4),
  [352] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [354] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_property_assignment, 4, .production_id = 1),
  [356] = {.entry = {.count = 1, .reusable = true}}, SHIFT(203),
  [358] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 4),
  [360] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 4),
  [362] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_declaration, 3),
  [364] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 2),
  [366] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 2),
  [368] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_domain_declaration, 7),
  [370] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 9),
  [372] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_label_declaration, 3),
  [374] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_label_declaration, 3),
  [376] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_module_declaration, 8),
  [378] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 5),
  [380] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 4),
  [382] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expression, 1),
  [384] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_declaration, 3),
  [386] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_declaration, 3),
  [388] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_declaration, 3),
  [390] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_container_declaration, 3),
  [392] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_container_declaration, 3),
  [394] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_component_body, 3),
  [396] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_component_body, 3),
  [398] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 3),
  [400] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 5),
  [402] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 3),
  [404] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 3),
  [406] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_view_body, 2),
  [408] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 4),
  [410] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_block, 2),
  [412] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parameter_block, 2),
  [414] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 3),
  [416] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_view_body, 2),
  [418] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 4),
  [420] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 3),
  [422] = {.entry = {.count = 1, .reusable = true}}, SHIFT(81),
  [424] = {.entry = {.count = 1, .reusable = true}}, SHIFT(291),
  [426] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_input_body_repeat1, 2),
  [428] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_input_body_repeat1, 2), SHIFT_REPEAT(291),
  [431] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_array_literal, 2),
  [433] = {.entry = {.count = 1, .reusable = true}}, SHIFT(260),
  [435] = {.entry = {.count = 1, .reusable = true}}, SHIFT(161),
  [437] = {.entry = {.count = 1, .reusable = true}}, SHIFT(259),
  [439] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [441] = {.entry = {.count = 1, .reusable = true}}, SHIFT(87),
  [443] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [445] = {.entry = {.count = 1, .reusable = true}}, SHIFT(139),
  [447] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [449] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_object_literal_repeat1, 2), SHIFT_REPEAT(192),
  [452] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_object_literal_repeat1, 2),
  [454] = {.entry = {.count = 1, .reusable = true}}, SHIFT(244),
  [456] = {.entry = {.count = 1, .reusable = true}}, SHIFT(210),
  [458] = {.entry = {.count = 1, .reusable = true}}, SHIFT(3),
  [460] = {.entry = {.count = 1, .reusable = true}}, SHIFT(131),
  [462] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2),
  [464] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_binding_repeat1, 2), SHIFT_REPEAT(210),
  [467] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [469] = {.entry = {.count = 1, .reusable = true}}, SHIFT(271),
  [471] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2),
  [473] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_parameter_block_repeat1, 2), SHIFT_REPEAT(178),
  [476] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [478] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [480] = {.entry = {.count = 1, .reusable = true}}, SHIFT(251),
  [482] = {.entry = {.count = 1, .reusable = true}}, SHIFT(239),
  [484] = {.entry = {.count = 1, .reusable = true}}, SHIFT(69),
  [486] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 3),
  [488] = {.entry = {.count = 1, .reusable = true}}, SHIFT(173),
  [490] = {.entry = {.count = 1, .reusable = true}}, SHIFT(175),
  [492] = {.entry = {.count = 1, .reusable = true}}, SHIFT(237),
  [494] = {.entry = {.count = 1, .reusable = true}}, SHIFT(197),
  [496] = {.entry = {.count = 1, .reusable = true}}, SHIFT(279),
  [498] = {.entry = {.count = 1, .reusable = true}}, SHIFT(226),
  [500] = {.entry = {.count = 1, .reusable = true}}, SHIFT(227),
  [502] = {.entry = {.count = 1, .reusable = true}}, SHIFT(132),
  [504] = {.entry = {.count = 1, .reusable = true}}, SHIFT(152),
  [506] = {.entry = {.count = 1, .reusable = true}}, SHIFT(150),
  [508] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 2),
  [510] = {.entry = {.count = 1, .reusable = true}}, SHIFT(72),
  [512] = {.entry = {.count = 1, .reusable = true}}, SHIFT(250),
  [514] = {.entry = {.count = 1, .reusable = true}}, SHIFT(137),
  [516] = {.entry = {.count = 1, .reusable = true}}, SHIFT(102),
  [518] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2), SHIFT_REPEAT(19),
  [521] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_call_expr_repeat1, 2),
  [523] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [525] = {.entry = {.count = 1, .reusable = true}}, SHIFT(178),
  [527] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 5),
  [529] = {.entry = {.count = 1, .reusable = true}}, SHIFT(180),
  [531] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2), SHIFT_REPEAT(251),
  [534] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_event_param_repeat1, 2),
  [536] = {.entry = {.count = 1, .reusable = true}}, SHIFT(209),
  [538] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [540] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_value_expression, 1),
  [542] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_literal, 4),
  [544] = {.entry = {.count = 1, .reusable = true}}, SHIFT(128),
  [546] = {.entry = {.count = 1, .reusable = true}}, SHIFT(167),
  [548] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [550] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2), SHIFT_REPEAT(3),
  [553] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_array_literal_repeat1, 2),
  [555] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_decl, 3, .production_id = 2),
  [557] = {.entry = {.count = 1, .reusable = true}}, SHIFT(93),
  [559] = {.entry = {.count = 1, .reusable = true}}, SHIFT(174),
  [561] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_member, 3, .production_id = 1),
  [563] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_object_member_value, 1),
  [565] = {.entry = {.count = 1, .reusable = true}}, SHIFT(157),
  [567] = {.entry = {.count = 1, .reusable = true}}, SHIFT(190),
  [569] = {.entry = {.count = 1, .reusable = true}}, SHIFT(214),
  [571] = {.entry = {.count = 1, .reusable = true}}, SHIFT(44),
  [573] = {.entry = {.count = 1, .reusable = true}}, SHIFT(189),
  [575] = {.entry = {.count = 1, .reusable = true}}, SHIFT(241),
  [577] = {.entry = {.count = 1, .reusable = true}}, SHIFT(188),
  [579] = {.entry = {.count = 1, .reusable = true}}, SHIFT(233),
  [581] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_input_type, 1),
  [583] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binding_pair, 3, .production_id = 1),
  [585] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_type, 1),
  [587] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_chart_kind, 1),
  [589] = {.entry = {.count = 1, .reusable = true}}, SHIFT(53),
  [591] = {.entry = {.count = 1, .reusable = true}}, SHIFT(45),
  [593] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [595] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_type_ref, 1),
  [597] = {.entry = {.count = 1, .reusable = true}}, SHIFT(204),
  [599] = {.entry = {.count = 1, .reusable = true}}, SHIFT(101),
  [601] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 4),
  [603] = {.entry = {.count = 1, .reusable = true}}, SHIFT(247),
  [605] = {.entry = {.count = 1, .reusable = true}}, SHIFT(225),
  [607] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [609] = {.entry = {.count = 1, .reusable = true}}, SHIFT(229),
  [611] = {.entry = {.count = 1, .reusable = true}}, SHIFT(230),
  [613] = {.entry = {.count = 1, .reusable = true}}, SHIFT(231),
  [615] = {.entry = {.count = 1, .reusable = true}}, SHIFT(232),
  [617] = {.entry = {.count = 1, .reusable = true}}, SHIFT(248),
  [619] = {.entry = {.count = 1, .reusable = true}}, SHIFT(222),
  [621] = {.entry = {.count = 1, .reusable = true}}, SHIFT(234),
  [623] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 6),
  [625] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 2),
  [627] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [629] = {.entry = {.count = 1, .reusable = true}}, SHIFT(235),
  [631] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 6),
  [633] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_refresh_action, 6),
  [635] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_field_ref, 4),
  [637] = {.entry = {.count = 1, .reusable = true}}, SHIFT(240),
  [639] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_action_invocation, 4),
  [641] = {.entry = {.count = 1, .reusable = true}}, SHIFT(220),
  [643] = {.entry = {.count = 1, .reusable = true}}, SHIFT(221),
  [645] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [647] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 3),
  [649] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expr_ref, 2),
  [651] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 4),
  [653] = {.entry = {.count = 1, .reusable = true}}, SHIFT(246),
  [655] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_navigate_action, 4),
  [657] = {.entry = {.count = 1, .reusable = true}}, SHIFT(283),
  [659] = {.entry = {.count = 1, .reusable = true}}, SHIFT(205),
  [661] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parameter_binding, 4),
  [663] = {.entry = {.count = 1, .reusable = true}}, SHIFT(223),
  [665] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_lookup_ref, 6),
  [667] = {.entry = {.count = 1, .reusable = true}}, SHIFT(208),
  [669] = {.entry = {.count = 1, .reusable = true}}, SHIFT(289),
  [671] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [673] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_param, 3),
  [675] = {.entry = {.count = 1, .reusable = true}}, SHIFT(187),
  [677] = {.entry = {.count = 1, .reusable = true}}, SHIFT(191),
  [679] = {.entry = {.count = 1, .reusable = true}}, SHIFT(195),
  [681] = {.entry = {.count = 1, .reusable = true}}, SHIFT(196),
  [683] = {.entry = {.count = 1, .reusable = true}}, SHIFT(293),
  [685] = {.entry = {.count = 1, .reusable = true}}, SHIFT(99),
  [687] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [689] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [691] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [693] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [695] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_event_action, 1),
  [697] = {.entry = {.count = 1, .reusable = true}}, SHIFT(252),
  [699] = {.entry = {.count = 1, .reusable = true}}, SHIFT(253),
  [701] = {.entry = {.count = 1, .reusable = true}}, SHIFT(254),
  [703] = {.entry = {.count = 1, .reusable = true}}, SHIFT(255),
  [705] = {.entry = {.count = 1, .reusable = true}}, SHIFT(256),
  [707] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [709] = {.entry = {.count = 1, .reusable = true}}, SHIFT(151),
  [711] = {.entry = {.count = 1, .reusable = true}}, SHIFT(78),
  [713] = {.entry = {.count = 1, .reusable = true}}, SHIFT(163),
  [715] = {.entry = {.count = 1, .reusable = true}}, SHIFT(156),
  [717] = {.entry = {.count = 1, .reusable = true}}, SHIFT(158),
  [719] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
  [721] = {.entry = {.count = 1, .reusable = true}}, SHIFT(100),
  [723] = {.entry = {.count = 1, .reusable = true}}, SHIFT(98),
  [725] = {.entry = {.count = 1, .reusable = true}}, SHIFT(199),
  [727] = {.entry = {.count = 1, .reusable = true}}, SHIFT(215),
  [729] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [731] = {.entry = {.count = 1, .reusable = true}}, SHIFT(111),
  [733] = {.entry = {.count = 1, .reusable = true}}, SHIFT(183),
  [735] = {.entry = {.count = 1, .reusable = true}}, SHIFT(184),
  [737] = {.entry = {.count = 1, .reusable = true}}, SHIFT(114),
  [739] = {.entry = {.count = 1, .reusable = true}}, SHIFT(193),
  [741] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [743] = {.entry = {.count = 1, .reusable = true}}, SHIFT(91),
  [745] = {.entry = {.count = 1, .reusable = true}}, SHIFT(70),
  [747] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [749] = {.entry = {.count = 1, .reusable = true}}, SHIFT(280),
  [751] = {.entry = {.count = 1, .reusable = true}}, SHIFT(74),
  [753] = {.entry = {.count = 1, .reusable = true}}, SHIFT(4),
  [755] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [757] = {.entry = {.count = 1, .reusable = true}}, SHIFT(41),
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
