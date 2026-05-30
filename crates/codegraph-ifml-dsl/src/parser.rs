use std::path::Path;

use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;

use crate::ast::*;

#[derive(Parser)]
#[grammar = "grammar/ifml.pest"]
pub struct IfmlParser;

fn parse_string(pair: &Pair<Rule>) -> String {
    let s = pair.as_str();
    let inner = &s[1..s.len() - 1];
    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(code) {
                            result.push(ch);
                        }
                    }
                }
                _ => {}
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn parse_number(pair: &Pair<Rule>) -> f64 {
    pair.as_str().parse::<f64>().unwrap_or(0.0)
}

fn parse_identifier(pair: &Pair<Rule>) -> String {
    pair.as_str().to_string()
}

fn parse_parameter_decl(pair: Pair<Rule>) -> ParameterDecl {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_identifier(&p)).unwrap_or_default();
    let type_ref = inner.next().map(|p| parse_identifier(&p)).unwrap_or_default();
    ParameterDecl { name, type_ref }
}

fn parse_parameter_block(pair: Pair<Rule>) -> Vec<ParameterDecl> {
    let mut params = Vec::new();
    for child in pair.clone().into_inner() {
        if child.as_rule() == Rule::parameter_decl {
            params.push(parse_parameter_decl(child));
        }
    }
    params
}

// ── Value/expression parsing ─────────────────────────────────────
// These two functions are mutually recursive:
//   parse_value_primary   — handles primary: string, number, ident, call, field, group
//   convert_expression_to_value — handles the full operator chain for ValueExpression
// They are defined before parse_value_expression which is the top-level entry.

fn parse_value_primary(pair: Pair<Rule>) -> ValueExpression {
    match pair.as_rule() {
        Rule::string => ValueExpression::String(parse_string(&pair)),
        Rule::number => ValueExpression::Number(parse_number(&pair)),
        Rule::boolean => ValueExpression::Bool(pair.as_str() == "true"),
        Rule::identifier => ValueExpression::Identifier(pair.as_str().to_string()),
        Rule::call_expr => {
            let mut inner = pair.clone().into_inner();
            let name = inner.next().map(|p| p.as_str().to_string()).unwrap_or_default();
            let mut args = Vec::new();
            for arg in inner {
                if arg.as_rule() != Rule::expression {
                    continue;
                }
                args.push(convert_expression_to_value(arg));
            }
            ValueExpression::Call(name, args)
        }
        Rule::field_expr => {
            let mut inner = pair.clone().into_inner();
            let object = inner.next().map(|p| p.as_str().to_string()).unwrap_or_default();
            let field = inner.next().map(|p| p.as_str().to_string()).unwrap_or_default();
            ValueExpression::FieldAccess {
                object: Box::new(ValueExpression::Identifier(object)),
                field,
            }
        }
        Rule::group_expr => {
            if let Some(inner) = pair.clone().into_inner().next() {
                ValueExpression::Group(Box::new(convert_expression_to_value(inner)))
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        Rule::array_literal => {
            let mut elems = Vec::new();
            for child in pair.clone().into_inner() {
                elems.push(parse_value_expression(child));
            }
            ValueExpression::Array(elems)
        }
        _ => ValueExpression::Identifier(pair.as_str().to_string()),
    }
}

fn convert_expression_to_value(pair: Pair<Rule>) -> ValueExpression {
    let rule = pair.as_rule();
    match rule {
        // expression wraps logical_or — delegate
        Rule::expression => {
            if let Some(inner) = pair.clone().into_inner().next() {
                convert_expression_to_value(inner)
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        // logical_or: all children are operands with || between them
        Rule::logical_or => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = convert_expression_to_value(first);
                for right in children {
                    let right_val = convert_expression_to_value(right);
                    result = ValueExpression::BinOp {
                        left: Box::new(result),
                        op: BinOp::Or,
                        right: Box::new(right_val),
                    };
                }
                result
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        // logical_and: all children are operands with && between them
        Rule::logical_and => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = convert_expression_to_value(first);
                for right in children {
                    let right_val = convert_expression_to_value(right);
                    result = ValueExpression::BinOp {
                        left: Box::new(result),
                        op: BinOp::And,
                        right: Box::new(right_val),
                    };
                }
                result
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        // comparison: children are [addition, comparison_op, addition, ...]
        Rule::comparison => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = convert_expression_to_value(first);
                while let Some(op_pair) = children.next() {
                    let op_str = op_pair.as_str();
                    let op = match op_str {
                        "==" => BinOp::Eq,
                        "!=" => BinOp::Ne,
                        "<" => BinOp::Lt,
                        "<=" => BinOp::Le,
                        ">" => BinOp::Gt,
                        ">=" => BinOp::Ge,
                        "~=" => BinOp::RegexMatch,
                        "!~" => BinOp::NegRegex,
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        "/" => BinOp::Div,
                        "%" => BinOp::Mod,
                        "&&" => BinOp::And,
                        "||" => BinOp::Or,
                        _ => BinOp::Eq,
                    };
                    if let Some(right) = children.next() {
                        let right_val = convert_expression_to_value(right);
                        result = ValueExpression::BinOp {
                            left: Box::new(result),
                            op,
                            right: Box::new(right_val),
                        };
                    }
                }
                result
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        Rule::addition | Rule::multiplication => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = convert_expression_to_value(first);
                while let Some(op_pair) = children.next() {
                    let op_str = op_pair.as_str();
                    let op = match op_str {
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        "/" => BinOp::Div,
                        "%" => BinOp::Mod,
                        _ => BinOp::Add,
                    };
                    if let Some(right) = children.next() {
                        let right_val = convert_expression_to_value(right);
                        result = ValueExpression::BinOp {
                            left: Box::new(result),
                            op,
                            right: Box::new(right_val),
                        };
                    }
                }
                result
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        Rule::unary => {
            if let Some(inner) = pair.clone().into_inner().next() {
                match inner.as_rule() {
                    Rule::unary_not => {
                        if let Some(operand) = inner.clone().into_inner().next() {
                            ValueExpression::UnaryOp {
                                op: UnaryOp::Not,
                                operand: Box::new(convert_expression_to_value(operand)),
                            }
                        } else {
                            ValueExpression::Identifier(pair.as_str().to_string())
                        }
                    }
                    Rule::unary_neg => {
                        if let Some(operand) = inner.clone().into_inner().next() {
                            ValueExpression::UnaryOp {
                                op: UnaryOp::Neg,
                                operand: Box::new(convert_expression_to_value(operand)),
                            }
                        } else {
                            ValueExpression::Identifier(pair.as_str().to_string())
                        }
                    }
                    Rule::primary => {
                        if let Some(primary_inner) = inner.clone().into_inner().next() {
                            parse_value_primary(primary_inner)
                        } else {
                            ValueExpression::Identifier(inner.as_str().to_string())
                        }
                    }
                    _ => convert_expression_to_value(inner),
                }
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        Rule::primary => {
            if let Some(inner) = pair.clone().into_inner().next() {
                parse_value_primary(inner)
            } else {
                ValueExpression::Identifier(pair.as_str().to_string())
            }
        }
        _ => parse_value_primary(pair),
    }
}

fn parse_value_expression(pair: Pair<Rule>) -> ValueExpression {
    if let Some(inner) = pair.clone().into_inner().next() {
        let rule = inner.as_rule();
        match rule {
            Rule::array_literal => {
                let mut elems = Vec::new();
                for child in inner.clone().into_inner() {
                    elems.push(parse_value_expression(child));
                }
                ValueExpression::Array(elems)
            }
            _ => convert_expression_to_value(inner),
        }
    } else {
        ValueExpression::Identifier(pair.as_str().to_string())
    }
}

fn parse_property_assignment(pair: Pair<Rule>) -> PropertyAssignment {
    let mut inner = pair.clone().into_inner();
    let key = inner.next().map(|p| p.as_str().to_string()).unwrap_or_default();
    let value = inner.next().map(parse_value_expression).unwrap_or(ValueExpression::Identifier("".to_string()));
    PropertyAssignment { key, value }
}

fn parse_event_type(pair: Pair<Rule>) -> EventType {
    match pair.as_str() {
        "select" => EventType::Select,
        "submit" => EventType::Submit,
        "click" => EventType::Click,
        "change" => EventType::Change,
        "load" => EventType::Load,
        "save" => EventType::Save,
        "cancel" => EventType::Cancel,
        "delete" => EventType::Delete,
        "confirm" => EventType::Confirm,
        "back" => EventType::Back,
        s => EventType::Custom(s.to_string()),
    }
}

fn parse_event_param(pair: Pair<Rule>) -> Vec<String> {
    let mut params = Vec::new();
    for child in pair.clone().into_inner() {
        params.push(child.as_str().to_string());
    }
    params
}

// ── Expression parsing (for parameter bindings) ──────────────────

fn fallback_expr(pair: &Pair<Rule>) -> Expression {
    Expression::Ident(pair.as_str().to_string())
}

fn parse_expression(pair: Pair<Rule>) -> Expression {
    let rule = pair.as_rule();
    match rule {
        Rule::expression => {
            if let Some(inner) = pair.clone().into_inner().next() {
                parse_expression(inner)
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::logical_or => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = parse_expression(first);
                for right in children {
                    let right_val = parse_expression(right);
                    result = Expression::BinOp {
                        left: Box::new(result),
                        op: BinOp::Or,
                        right: Box::new(right_val),
                    };
                }
                result
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::logical_and => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = parse_expression(first);
                for right in children {
                    let right_val = parse_expression(right);
                    result = Expression::BinOp {
                        left: Box::new(result),
                        op: BinOp::And,
                        right: Box::new(right_val),
                    };
                }
                result
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::comparison => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = parse_expression(first);
                while let Some(op_pair) = children.next() {
                    let op_str = op_pair.as_str();
                    let op = match op_str {
                        "==" => BinOp::Eq,
                        "!=" => BinOp::Ne,
                        "<" => BinOp::Lt,
                        "<=" => BinOp::Le,
                        ">" => BinOp::Gt,
                        ">=" => BinOp::Ge,
                        "~=" => BinOp::RegexMatch,
                        "!~" => BinOp::NegRegex,
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        "/" => BinOp::Div,
                        "%" => BinOp::Mod,
                        "&&" => BinOp::And,
                        "||" => BinOp::Or,
                        _ => BinOp::Eq,
                    };
                    if let Some(right) = children.next() {
                        let right_val = parse_expression(right);
                        result = Expression::BinOp {
                            left: Box::new(result),
                            op,
                            right: Box::new(right_val),
                        };
                    }
                }
                result
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::addition | Rule::multiplication => {
            let mut children = pair.clone().into_inner();
            if let Some(first) = children.next() {
                let mut result = parse_expression(first);
                while let Some(op_pair) = children.next() {
                    let op_str = op_pair.as_str();
                    let op = match op_str {
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        "/" => BinOp::Div,
                        "%" => BinOp::Mod,
                        _ => BinOp::Add,
                    };
                    if let Some(right) = children.next() {
                        let right_val = parse_expression(right);
                        result = Expression::BinOp {
                            left: Box::new(result),
                            op,
                            right: Box::new(right_val),
                        };
                    }
                }
                result
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::unary => {
            if let Some(inner) = pair.clone().into_inner().next() {
                match inner.as_rule() {
                    Rule::unary_not => {
                        if let Some(operand) = inner.clone().into_inner().next() {
                            Expression::UnaryOp {
                                op: UnaryOp::Not,
                                operand: Box::new(parse_expression(operand)),
                            }
                        } else {
                            fallback_expr(&pair)
                        }
                    }
                    Rule::unary_neg => {
                        if let Some(operand) = inner.clone().into_inner().next() {
                            Expression::UnaryOp {
                                op: UnaryOp::Neg,
                                operand: Box::new(parse_expression(operand)),
                            }
                        } else {
                            fallback_expr(&pair)
                        }
                    }
                    Rule::primary => {
                        if let Some(primary_inner) = inner.clone().into_inner().next() {
                            parse_primary_expression(primary_inner)
                        } else {
                            Expression::Ident(inner.as_str().to_string())
                        }
                    }
                    _ => parse_expression(inner),
                }
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::primary => {
            if let Some(inner) = pair.clone().into_inner().next() {
                parse_primary_expression(inner)
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::string => Expression::StringLit(parse_string(&pair)),
        Rule::number => Expression::NumLit(parse_number(&pair)),
        Rule::boolean => Expression::BoolLit(pair.as_str() == "true"),
        Rule::identifier => Expression::Ident(pair.as_str().to_string()),
        Rule::call_expr => {
            let mut inner = pair.clone().into_inner();
            if let Some(name_pair) = inner.next() {
                let name = name_pair.as_str().to_string();
                let mut args = Vec::new();
                for arg in inner {
                    if arg.as_rule() != Rule::expression {
                        continue;
                    }
                    args.push(parse_expression(arg));
                }
                Expression::Call { name, args }
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::field_expr => {
            let mut inner = pair.clone().into_inner();
            if let Some(obj_pair) = inner.next() {
                let object = obj_pair.as_str().to_string();
                if let Some(field_pair) = inner.next() {
                    let field = field_pair.as_str().to_string();
                    Expression::FieldExpr {
                        object: Box::new(Expression::Ident(object)),
                        field,
                    }
                } else {
                    Expression::Ident(object)
                }
            } else {
                fallback_expr(&pair)
            }
        }
        Rule::group_expr => {
            if let Some(inner) = pair.clone().into_inner().next() {
                Expression::Group(Box::new(parse_expression(inner)))
            } else {
                fallback_expr(&pair)
            }
        }
        _ => fallback_expr(&pair),
    }
}

fn parse_primary_expression(pair: Pair<Rule>) -> Expression {
    match pair.as_rule() {
        Rule::string => Expression::StringLit(parse_string(&pair)),
        Rule::number => Expression::NumLit(parse_number(&pair)),
        Rule::boolean => Expression::BoolLit(pair.as_str() == "true"),
        Rule::identifier => Expression::Ident(pair.as_str().to_string()),
        Rule::call_expr => {
            let mut inner = pair.clone().into_inner();
            if let Some(name_pair) = inner.next() {
                let name = name_pair.as_str().to_string();
                let mut args = Vec::new();
                for arg in inner {
                    if arg.as_rule() != Rule::expression {
                        continue;
                    }
                    args.push(parse_expression(arg));
                }
                Expression::Call { name, args }
            } else {
                Expression::Ident(pair.as_str().to_string())
            }
        }
        Rule::field_expr => {
            let mut inner = pair.clone().into_inner();
            if let Some(obj_pair) = inner.next() {
                let object = obj_pair.as_str().to_string();
                if let Some(field_pair) = inner.next() {
                    let field = field_pair.as_str().to_string();
                    Expression::FieldExpr {
                        object: Box::new(Expression::Ident(object)),
                        field,
                    }
                } else {
                    Expression::Ident(object)
                }
            } else {
                Expression::Ident(pair.as_str().to_string())
            }
        }
        Rule::group_expr => {
            if let Some(inner) = pair.clone().into_inner().next() {
                Expression::Group(Box::new(parse_expression(inner)))
            } else {
                Expression::Ident(pair.as_str().to_string())
            }
        }
        _ => Expression::Ident(pair.as_str().to_string()),
    }
}

// ── Event / Action parsing ───────────────────────────────────────

fn parse_parameter_binding(pair: Pair<Rule>) -> ParameterBinding {
    let mut pairs = Vec::new();
    for child in pair.clone().into_inner() {
        if child.as_rule() == Rule::parameter_binding_pair {
            let mut inner = child.into_inner();
            let key = inner.next().map(|p| p.as_str().to_string()).unwrap_or_default();
            let value = inner.next().map(parse_expression).unwrap_or(Expression::Ident("".to_string()));
            pairs.push((key, value));
        }
    }
    ParameterBinding { pairs }
}

fn parse_event_action(pair: Pair<Rule>) -> EventAction {
    let inner = if let Some(inner) = pair.clone().into_inner().next() {
        inner
    } else {
        return EventAction::Stay;
    };
    match inner.as_rule() {
        Rule::navigate_action => {
            let mut nav_inner = inner.clone().into_inner();
            let target = nav_inner.next().map(|p| parse_string(&p)).unwrap_or_default();
            let binding = nav_inner.next().map(parse_parameter_binding);
            EventAction::Navigate { target, binding }
        }
        Rule::refresh_action => {
            let mut ref_inner = inner.clone().into_inner();
            let target = ref_inner.next().map(|p| parse_string(&p)).unwrap_or_default();
            let binding = ref_inner.next().map(parse_parameter_binding);
            EventAction::Refresh { target, binding }
        }
        Rule::action_invocation => {
            let mut act_inner = inner.clone().into_inner();
            let name = act_inner.next().map(|p| parse_string(&p)).unwrap_or_default();
            let body = act_inner.next().map(parse_action_body);
            EventAction::ActionInvocation { name, body }
        }
        Rule::stay_statement => EventAction::Stay,
        _ => EventAction::Stay,
    }
}

fn parse_action_body(pair: Pair<Rule>) -> ActionBody {
    let mut properties = Vec::new();
    let mut handlers = Vec::new();
    for child in pair.clone().into_inner() {
        match child.as_rule() {
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::event_handler => handlers.push(parse_event_handler(child)),
            _ => {}
        }
    }
    ActionBody {
        properties,
        handlers,
    }
}

fn parse_event_handler(pair: Pair<Rule>) -> EventHandler {
    let mut inner = pair.clone().into_inner();
    let event_type = inner.next().map(parse_event_type).unwrap_or(EventType::Custom("unknown".to_string()));

    let mut params = Vec::new();
    let mut action = EventAction::Stay;

    for child in inner {
        match child.as_rule() {
            Rule::event_param => params = parse_event_param(child),
            _ => action = parse_event_action(child),
        }
    }

    EventHandler {
        event_type,
        params,
        action,
    }
}

// ── Component / Container / View parsing ─────────────────────────

fn parse_component_declaration(pair: Pair<Rule>) -> ComponentDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();
    let body = if let Some(b) = inner.next() { b } else {
        return ComponentDeclaration { name, properties: Vec::new(), events: Vec::new() };
    };

    let mut properties = Vec::new();
    let mut events = Vec::new();
    for child in body.into_inner() {
        match child.as_rule() {
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::event_handler => events.push(parse_event_handler(child)),
            _ => {}
        }
    }

    ComponentDeclaration {
        name,
        properties,
        events,
    }
}

fn parse_view_body_content(
    pair: Pair<Rule>,
) -> (
    Vec<ParameterDecl>,
    Option<String>,
    Vec<PropertyAssignment>,
    Vec<ContainerDeclaration>,
    Vec<ComponentDeclaration>,
    Vec<EventHandler>,
) {
    let mut params = Vec::new();
    let mut label = None;
    let mut properties = Vec::new();
    let mut containers = Vec::new();
    let mut components = Vec::new();
    let mut events = Vec::new();

    for child in pair.clone().into_inner() {
        match child.as_rule() {
            Rule::params_block => {
                if let Some(block) = child.into_inner().next() {
                    params = parse_parameter_block(block);
                }
            }
            Rule::label_declaration => {
                if let Some(s) = child.into_inner().next() {
                    label = Some(parse_string(&s));
                }
            }
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::container_declaration => containers.push(parse_container_declaration(child)),
            Rule::component_declaration => components.push(parse_component_declaration(child)),
            Rule::event_handler => events.push(parse_event_handler(child)),
            _ => {}
        }
    }

    (params, label, properties, containers, components, events)
}

fn parse_container_body_content(
    pair: Pair<Rule>,
) -> (
    Vec<ParameterDecl>,
    Vec<PropertyAssignment>,
    Vec<ComponentDeclaration>,
    Vec<EventHandler>,
) {
    let mut params = Vec::new();
    let mut properties = Vec::new();
    let mut components = Vec::new();
    let mut events = Vec::new();

    for child in pair.clone().into_inner() {
        match child.as_rule() {
            Rule::params_block => {
                if let Some(block) = child.into_inner().next() {
                    params = parse_parameter_block(block);
                }
            }
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::component_declaration => components.push(parse_component_declaration(child)),
            Rule::event_handler => events.push(parse_event_handler(child)),
            _ => {}
        }
    }

    (params, properties, components, events)
}

fn parse_container_declaration(pair: Pair<Rule>) -> ContainerDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();
    let body = if let Some(b) = inner.next() { b } else {
        return ContainerDeclaration {
            name,
            is_default: false,
            params: Vec::new(),
            properties: Vec::new(),
            components: Vec::new(),
            events: Vec::new(),
        };
    };

    let (params, properties, components, events) = parse_container_body_content(body);

    let is_default = properties
        .iter()
        .find(|p| p.key == "default")
        .and_then(|p| {
            if let ValueExpression::Bool(val) = p.value {
                Some(val)
            } else {
                None
            }
        })
        .unwrap_or(false);

    ContainerDeclaration {
        name,
        is_default,
        params,
        properties,
        components,
        events,
    }
}

fn extract_bool_property(properties: &[PropertyAssignment], key: &str) -> bool {
    properties
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| {
            if let ValueExpression::Bool(val) = p.value {
                Some(val)
            } else {
                None
            }
        })
        .unwrap_or(false)
}

fn parse_view_declaration(pair: Pair<Rule>) -> ViewDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();
    let body = if let Some(b) = inner.next() { b } else {
        return ViewDeclaration {
            name,
            label: None,
            is_landmark: false,
            is_xor: false,
            is_modal: false,
            params: Vec::new(),
            properties: Vec::new(),
            containers: Vec::new(),
            components: Vec::new(),
            events: Vec::new(),
        };
    };

    let (params, label, properties, containers, components, events) =
        parse_view_body_content(body);

    let is_landmark = extract_bool_property(&properties, "landmark");
    let is_xor = extract_bool_property(&properties, "xor");
    let is_modal = extract_bool_property(&properties, "modal");

    ViewDeclaration {
        name,
        label,
        is_landmark,
        is_xor,
        is_modal,
        params,
        properties,
        containers,
        components,
        events,
    }
}

fn parse_action_declaration(pair: Pair<Rule>) -> ActionDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();

    let mut properties = Vec::new();
    let mut events = Vec::new();

    for child in inner {
        match child.as_rule() {
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::event_handler => events.push(parse_event_handler(child)),
            _ => {}
        }
    }

    ActionDeclaration { name, properties, events }
}

fn parse_module_declaration(pair: Pair<Rule>) -> ModuleDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();

    let mut input_params = Vec::new();
    let mut output_params = Vec::new();
    let mut properties = Vec::new();
    let mut containers = Vec::new();
    let mut components = Vec::new();
    let mut events = Vec::new();

    for child in inner {
        match child.as_rule() {
            Rule::parameter_block => {
                if input_params.is_empty() {
                    input_params = parse_parameter_block(child);
                } else {
                    output_params = parse_parameter_block(child);
                }
            }
            Rule::property_assignment => properties.push(parse_property_assignment(child)),
            Rule::container_declaration => containers.push(parse_container_declaration(child)),
            Rule::component_declaration => components.push(parse_component_declaration(child)),
            Rule::event_handler => events.push(parse_event_handler(child)),
            _ => {}
        }
    }

    ModuleDeclaration {
        name,
        input_params,
        output_params,
        properties,
        containers,
        components,
        events,
    }
}

fn parse_domain_declaration(pair: Pair<Rule>) -> DomainDeclaration {
    let mut inner = pair.clone().into_inner();
    let name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();
    let schema_name = inner.next().map(|p| parse_string(&p)).unwrap_or_default();
    DomainDeclaration { name, schema_name }
}

fn parse_ifml_model(pairs: Pairs<Rule>) -> Result<IfmlModel, IfmlParseError> {
    let mut domains = Vec::new();
    let mut views = Vec::new();
    let mut actions = Vec::new();
    let mut modules = Vec::new();

    for pair in pairs {
        let rule = pair.as_rule();
        match rule {
            Rule::domain_declaration => domains.push(parse_domain_declaration(pair)),
            Rule::view_declaration => views.push(parse_view_declaration(pair)),
            Rule::action_declaration => actions.push(parse_action_declaration(pair)),
            Rule::module_declaration => modules.push(parse_module_declaration(pair)),
            Rule::EOI => {}
            _ => {
                return Err(IfmlParseError::Parse {
                    position: pair.line_col().0.to_string(),
                    message: format!("Unexpected rule: {:?}", rule),
                });
            }
        }
    }

    Ok(IfmlModel {
        domains,
        views,
        actions,
        modules,
    })
}

/// Parse an IFML DSL string into an AST model.
pub fn parse_ifml(input: &str) -> Result<IfmlModel, IfmlParseError> {
    let parsed = IfmlParser::parse(Rule::ifml_model, input)
        .map_err(|e| IfmlParseError::Parse {
            position: "unknown".to_string(),
            message: format!("{}", e),
        })?;

    let top_level_pairs: Vec<Pair<Rule>> = parsed.collect();
    if top_level_pairs.is_empty() {
        return Ok(IfmlModel {
            domains: Vec::new(),
            views: Vec::new(),
            actions: Vec::new(),
            modules: Vec::new(),
        });
    }

    let top = &top_level_pairs[0];
    if top.as_rule() != Rule::ifml_model {
        return Err(IfmlParseError::Parse {
            position: top.line_col().0.to_string(),
            message: format!("Expected ifml_model, got {:?}", top.as_rule()),
        });
    }

    parse_ifml_model(top.clone().into_inner())
}

/// Parse an IFML DSL file into an AST model.
pub fn parse_ifml_file(path: &Path) -> Result<IfmlModel, IfmlParseError> {
    let input = std::fs::read_to_string(path).map_err(IfmlParseError::Io)?;
    parse_ifml(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_view() {
        let input = r#"
view "Minimal" {
    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email];
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 1);
        assert_eq!(model.views[0].name, "Minimal");
        assert_eq!(model.views[0].components.len(), 1);
        assert_eq!(model.views[0].components[0].name, "grid");
    }

    #[test]
    fn test_view_with_params() {
        let input = r#"
view "Detail" {
    params { id: Uuid, mode: String };
    component "info" {
        type: details;
        data: Customer;
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 1);
        let view = &model.views[0];
        assert_eq!(view.params.len(), 2);
        assert_eq!(view.params[0].name, "id");
        assert_eq!(view.params[0].type_ref, "Uuid");
        assert_eq!(view.params[1].name, "mode");
        assert_eq!(view.params[1].type_ref, "String");
    }

    #[test]
    fn test_container_with_navigation() {
        let input = r#"
view "Wizard" {
    xor: true;

    container "Step1" {
        default: true;

        component "form" {
            type: form;
            data: Customer;

            on submit(values) -> navigate("Step2", {
                name: values.name
            });
        }
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 1);
        let view = &model.views[0];
        assert!(view.is_xor);
        assert_eq!(view.containers.len(), 1);
        let container = &view.containers[0];
        assert_eq!(container.name, "Step1");
        assert!(container.is_default);
        assert_eq!(container.components.len(), 1);
        assert_eq!(container.components[0].name, "form");
        assert_eq!(container.components[0].events.len(), 1);

        let event = &container.components[0].events[0];
        assert_eq!(event.event_type, EventType::Submit);
        assert_eq!(event.params, vec!["values"]);
        match &event.action {
            EventAction::Navigate { target, binding } => {
                assert_eq!(target, "Step2");
                assert!(binding.is_some());
                assert_eq!(binding.as_ref().unwrap().pairs[0].0, "name");
            }
            _ => panic!("Expected Navigate action"),
        }
    }

    #[test]
    fn test_all_action_types() {
        let input = r#"
view "Actions" {
    component "c1" {
        type: list;
        data: Customer;

        on select(row) -> navigate("Detail", { id: row.id });
    }

    component "c2" {
        type: list;
        data: Order;

        on load -> refresh("grid");
    }

    component "c3" {
        type: form;
        data: Customer;

        on save(values) -> action("UpdateCustomer", {
            body: values;
            on success -> navigate("List");
            on error -> stay;
        });

        on cancel -> navigate("List");
    }

    component "c4" {
        type: form;
        data: ConfirmDelete;

        on submit(values) -> action("DeleteProduct", {
            on success -> navigate("ProductList");
            on error -> stay;
        });
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 1);
        let view = &model.views[0];
        assert_eq!(view.components.len(), 4);

        let c1_event = &view.components[0].events[0];
        assert_eq!(c1_event.event_type, EventType::Select);
        assert_eq!(c1_event.params, vec!["row"]);
        match &c1_event.action {
            EventAction::Navigate { target, binding } => {
                assert_eq!(target, "Detail");
                assert!(binding.is_some());
                assert_eq!(binding.as_ref().unwrap().pairs[0].0, "id");
            }
            _ => panic!("Expected Navigate"),
        }

        let c2_event = &view.components[1].events[0];
        assert_eq!(c2_event.event_type, EventType::Load);
        match &c2_event.action {
            EventAction::Refresh { target, binding } => {
                assert_eq!(target, "grid");
                assert!(binding.is_none());
            }
            _ => panic!("Expected Refresh"),
        }

        let c3_save = &view.components[2].events[0];
        assert_eq!(c3_save.event_type, EventType::Save);
        assert_eq!(c3_save.params, vec!["values"]);
        match &c3_save.action {
            EventAction::ActionInvocation { name, body } => {
                assert_eq!(name, "UpdateCustomer");
                let body = body.as_ref().expect("Expected action body");
                assert_eq!(body.properties.len(), 1);
                assert_eq!(body.properties[0].key, "body");
                assert_eq!(body.handlers.len(), 2);
            }
            _ => panic!("Expected ActionInvocation"),
        }

        let c3_cancel = &view.components[2].events[1];
        assert_eq!(c3_cancel.event_type, EventType::Cancel);
        match &c3_cancel.action {
            EventAction::Navigate { target, binding } => {
                assert_eq!(target, "List");
                assert!(binding.is_none());
            }
            _ => panic!("Expected Navigate"),
        }

        let c4_event = &view.components[3].events[0];
        match &c4_event.action {
            EventAction::ActionInvocation { name, body } => {
                assert_eq!(name, "DeleteProduct");
                let body = body.as_ref().expect("Expected action body");
                assert_eq!(body.properties.len(), 0);
                assert_eq!(body.handlers.len(), 2);
            }
            _ => panic!("Expected ActionInvocation"),
        }
    }

    #[test]
    fn test_expressions_in_properties() {
        let input = r#"
view "ExprTest" {
    component "grid" {
        type: list;
        data: Product;
        fields: [name, price];
        filter: price > 10.0 && status == "active";
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let grid = &model.views[0].components[0];

        let filter_prop = grid
            .properties
            .iter()
            .find(|p| p.key == "filter")
            .expect("Expected filter property");

        match &filter_prop.value {
            ValueExpression::BinOp { op, .. } => {
                assert_eq!(*op, BinOp::And);
            }
            other => {
                panic!("Expected BinOp(And), got {:?}", other);
            }
        }

        assert_eq!(grid.properties.len(), 4);
    }

    #[test]
    fn test_domain_declaration() {
        let input = r#"
domain "sales" {
    schema "sales";
}

view "Simple" {
    component "c" {
        type: list;
        data: Item;
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.domains.len(), 1);
        assert_eq!(model.domains[0].name, "sales");
        assert_eq!(model.domains[0].schema_name, "sales");
        assert_eq!(model.views.len(), 1);
    }

    #[test]
    fn test_modal_view() {
        let input = r#"
view "DeleteConfirm" {
    modal: true;

    component "form" {
        type: form;
        data: ConfirmDelete;

        on cancel -> navigate("ProductList");
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let view = &model.views[0];
        assert!(view.is_modal);
        assert!(!view.is_landmark);
        assert!(!view.is_xor);
    }

    #[test]
    fn test_complex_view_with_label() {
        let input = r#"
view "Dashboard" {
    label "Analytics Dashboard";
    landmark: true;

    on load -> refresh("recentOrders");
    on load -> refresh("topProducts");

    component "recentOrders" {
        type: list;
        data: Order;
        fields: [id, customerName, total, date];
        filter: date == today();
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let view = &model.views[0];
        assert!(view.is_landmark);
        assert_eq!(view.label.as_deref(), Some("Analytics Dashboard"));
        assert_eq!(view.events.len(), 2);
        assert_eq!(view.components.len(), 1);
    }

    #[test]
    fn test_invalid_syntax() {
        let input = "view Broken { this is not valid }";
        let result = parse_ifml(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_missing_semicolon() {
        let input = r#"
view "Broken" {
    component "c" {
        type: list
    }
}
"#;
        let result = parse_ifml(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_action_declaration() {
        let input = r#"
action "ArchiveOldOrders" {
    description: "Archives orders older than 90 days";
    on success -> navigate("OrderList");
    on error -> stay;
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.actions.len(), 1);
        let action = &model.actions[0];
        assert_eq!(action.name, "ArchiveOldOrders");
        assert_eq!(action.properties.len(), 1);
        assert_eq!(action.properties[0].key, "description");
        assert_eq!(action.events.len(), 2);
    }

    #[test]
    fn test_search_page() {
        let input = r#"
view "SearchPage" {
    component "searchForm" {
        type: form;
        data: SearchQuery;

        on submit(values) -> navigate("SearchPage", {
            query: values.term
        });
    }

    component "results" {
        type: list;
        data: Product;
        fields: [name, sku, price];
        filter: name ~= params.query;

        on select(row) -> navigate("ProductDetail", {
            productId: row.id
        });
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let view = &model.views[0];
        assert_eq!(view.components.len(), 2);

        let form = &view.components[0];
        let submit_event = &form.events[0];
        assert_eq!(submit_event.params, vec!["values"]);
        match &submit_event.action {
            EventAction::Navigate { target, binding } => {
                assert_eq!(target, "SearchPage");
                assert!(binding.is_some());
                assert_eq!(binding.as_ref().unwrap().pairs[0].0, "query");
            }
            _ => panic!("Expected Navigate"),
        }

        let results = &view.components[1];
        let select_event = &results.events[0];
        assert_eq!(select_event.params, vec!["row"]);
        match &select_event.action {
            EventAction::Navigate { target, binding } => {
                assert_eq!(target, "ProductDetail");
                assert!(binding.is_some());
                assert_eq!(binding.as_ref().unwrap().pairs[0].0, "productId");
            }
            _ => panic!("Expected Navigate"),
        }
    }

    #[test]
    fn test_boolean_properties() {
        let input = r#"
view "Test" {
    landmark: true;
    xor: false;
    modal: true;

    component "c" {
        type: list;
        data: X;
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let view = &model.views[0];
        assert!(view.is_landmark);
        assert!(!view.is_xor);
        assert!(view.is_modal);
    }

    #[test]
    fn test_array_fields() {
        let input = r#"
view "Fields" {
    component "c" {
        type: list;
        data: Item;
        fields: [name, price, createdAt];
        filter: status == "active" && price > 100.50;
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let comp = &model.views[0].components[0];
        let fields_prop = comp.properties.iter().find(|p| p.key == "fields").unwrap();
        match &fields_prop.value {
            ValueExpression::Array(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_module_declaration() {
        let input = r#"
module "Pagination" {
    input { page: Int, pageSize: Int }
    output { result: String }

    component "pager" {
        type: list;
        data: Item;
        fields: [name];
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.modules.len(), 1);
        let module = &model.modules[0];
        assert_eq!(module.name, "Pagination");
        assert_eq!(module.input_params.len(), 2);
        assert_eq!(module.input_params[0].name, "page");
        assert_eq!(module.input_params[0].type_ref, "Int");
        assert_eq!(module.output_params.len(), 1);
        assert_eq!(module.output_params[0].name, "result");
        assert_eq!(module.components.len(), 1);
    }

    #[test]
    fn test_full_docs_example() {
        let input = r#"
domain "sales" {
    schema "sales";
}

view "CustomerList" {
    label "Customer Management";
    landmark: true;

    component "grid" {
        type: list;
        data: Customer;
        fields: [name, email, phone, status];
        sortable: true;
        paginated: true;

        on select(row) -> navigate("CustomerDetail", {
            customerId: row.id
        });
    }

    component "searchBar" {
        type: form;
        data: CustomerSearchCriteria;

        on submit(values) -> refresh("grid", {
            query: searchBar.query
        });
    }
}

view "CustomerDetail" {
    params { customerId: Uuid };

    component "info" {
        type: details;
        data: Customer;
        fields: [name, email, phone, status, createdAt];

        on edit -> navigate("CustomerEdit", {
            customerId: params.customerId
        });
    }
}

view "CustomerEdit" {
    params { customerId: Uuid };

    component "form" {
        type: form;
        data: Customer;
        mode: edit;

        on save(values) -> action("UpdateCustomer", {
            on success -> navigate("CustomerDetail", {
                customerId: params.customerId
            });
            on error -> stay;
        });

        on cancel -> navigate("CustomerDetail");
    }
}

view "WizardPage" {
    xor: true;

    container "Step1" {
        default: true;

        component "personalInfo" {
            type: form;
            data: Customer;

            on submit(values) -> navigate("Step2", {
                name: values.name,
                email: values.email
            });
        }
    }

    container "Step2" {
        component "addressInfo" {
            type: form;
            data: Address;

            on submit(values) -> navigate("Step3", {
                street: values.street,
                city: values.city
            });
        }
    }
}

view "Dashboard" {
    on load -> refresh("recentOrders");

    component "recentOrders" {
        type: list;
        data: Order;
        fields: [id, customerName, total, date];
        filter: date == today();
    }
}

view "DeleteConfirm" {
    modal: true;

    component "confirmForm" {
        type: form;
        data: ConfirmDelete;

        on submit(values) -> action("DeleteProduct", {
            on success -> navigate("ProductList");
            on error -> stay;
        });

        on cancel -> navigate("ProductList");
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.domains.len(), 1);
        assert_eq!(model.domains[0].name, "sales");
        assert_eq!(model.views.len(), 6);
        assert!(model.actions.is_empty());
        assert!(model.modules.is_empty());

        let cl = &model.views[0];
        assert_eq!(cl.name, "CustomerList");
        assert!(cl.is_landmark);
        assert_eq!(cl.components.len(), 2);
        assert_eq!(cl.containers.len(), 0);

        let cd = &model.views[1];
        assert_eq!(cd.name, "CustomerDetail");
        assert_eq!(cd.params.len(), 1);
        assert_eq!(cd.params[0].name, "customerId");

        let db = &model.views[4];
        assert_eq!(db.name, "Dashboard");
        assert_eq!(db.events.len(), 1);

        let dc = &model.views[5];
        assert_eq!(dc.name, "DeleteConfirm");
        assert!(dc.is_modal);
    }

    #[test]
    fn test_string_escape() {
        let input = r#"
view "Esc" {
    component "c" {
        type: list;
        data: Item;
        label: "hello\nworld";
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        let comp = &model.views[0].components[0];
        let label_prop = comp.properties.iter().find(|p| p.key == "label").unwrap();
        match &label_prop.value {
            ValueExpression::String(s) => {
                assert_eq!(s, "hello\nworld");
            }
            _ => panic!("Expected String"),
        }
    }

    #[test]
    fn test_comment_is_ignored() {
        let input = r#"
// This is a comment
view "Commented" {
    // inline comment
    component "c" {
        type: list;
        data: X;
        // another comment
    }
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 1);
        assert_eq!(model.views[0].name, "Commented");
    }

    #[test]
    fn test_empty_model() {
        let input = "";
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.views.len(), 0);
        assert_eq!(model.domains.len(), 0);
    }

    #[test]
    fn test_domain_only() {
        let input = r#"
domain "hr" {
    schema "hr_schema";
}
"#;
        let model = parse_ifml(input).unwrap();
        assert_eq!(model.domains.len(), 1);
        assert_eq!(model.domains[0].name, "hr");
        assert_eq!(model.domains[0].schema_name, "hr_schema");
    }
}
