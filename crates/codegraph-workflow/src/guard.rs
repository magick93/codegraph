//! Guard expression evaluator using nom.
//!
//! Evaluates boolean predicates against serde_json::Value.
//! Supports: >, >=, <, <=, ==, !=, IS NULL, IS NOT NULL,
//! IS EMPTY, IS NOT EMPTY, AND, OR, IN, dot-notation field access.

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::{map, value},
    multi::separated_list1,
    number::complete::double,
    sequence::{delimited, preceded, tuple},
    IResult,
};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum GuardError {
    #[error("parse error: {0}")]
    Parse(String),
}

pub struct GuardEvaluator;

impl GuardEvaluator {
    pub fn evaluate(expr: &str, data: &Value) -> Result<bool, GuardError> {
        let (remaining, ast) =
            parse_or_expr(expr.trim()).map_err(|e| GuardError::Parse(format!("{e}")))?;
        if !remaining.trim().is_empty() {
            return Err(GuardError::Parse(format!(
                "unexpected trailing input: '{remaining}'"
            )));
        }
        Ok(eval_expr(&ast, data))
    }
}

// === AST ===

#[derive(Debug, Clone)]
enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Compare(String, CmpOp, LitValue),
    IsNull(String),
    IsNotNull(String),
    IsEmpty(String),
    IsNotEmpty(String),
    In(String, Vec<LitValue>),
}

#[derive(Debug, Clone)]
enum CmpOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
}

#[derive(Debug, Clone)]
enum LitValue {
    Num(f64),
    Str(String),
}

// === Parser ===

fn parse_or_expr(input: &str) -> IResult<&str, Expr> {
    let (input, first) = parse_and_expr(input)?;
    let (input, rest) = nom::multi::many0(preceded(
        delimited(multispace0, tag_no_case("OR"), multispace1),
        parse_and_expr,
    ))(input)?;
    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, e| Expr::Or(Box::new(acc), Box::new(e))),
    ))
}

fn parse_and_expr(input: &str) -> IResult<&str, Expr> {
    let (input, first) = parse_atom(input)?;
    let (input, rest) = nom::multi::many0(preceded(
        delimited(multispace0, tag_no_case("AND"), multispace1),
        parse_atom,
    ))(input)?;
    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, e| Expr::And(Box::new(acc), Box::new(e))),
    ))
}

fn parse_atom(input: &str) -> IResult<&str, Expr> {
    let (input, _) = multispace0(input)?;
    alt((parse_is_expr, parse_in_expr, parse_comparison))(input)
}

fn parse_field_name(input: &str) -> IResult<&str, String> {
    let (input, name) = take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '.')(input)?;
    Ok((input, name.to_string()))
}

fn parse_is_expr(input: &str) -> IResult<&str, Expr> {
    let (input, field) = parse_field_name(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("IS")(input)?;
    let (input, _) = multispace1(input)?;
    let f1 = field.clone();
    let f2 = field.clone();
    let f3 = field.clone();
    let f4 = field;
    alt((
        map(
            preceded(
                tuple((tag_no_case("NOT"), multispace1)),
                tag_no_case("NULL"),
            ),
            move |_| Expr::IsNotNull(f1.clone()),
        ),
        map(
            preceded(
                tuple((tag_no_case("NOT"), multispace1)),
                tag_no_case("EMPTY"),
            ),
            move |_| Expr::IsNotEmpty(f2.clone()),
        ),
        value(Expr::IsNull(f3), tag_no_case("NULL")),
        value(Expr::IsEmpty(f4), tag_no_case("EMPTY")),
    ))(input)
}

fn parse_in_expr(input: &str) -> IResult<&str, Expr> {
    let (input, field) = parse_field_name(input)?;
    let (input, _) = delimited(multispace0, tag_no_case("IN"), multispace0)(input)?;
    let (input, _) = char('(')(input)?;
    let (input, _) = multispace0(input)?;
    let (input, values) = separated_list1(
        delimited(multispace0, char(','), multispace0),
        parse_literal,
    )(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = char(')')(input)?;
    Ok((input, Expr::In(field, values)))
}

fn parse_comparison(input: &str) -> IResult<&str, Expr> {
    let (input, field) = parse_field_name(input)?;
    let (input, _) = multispace0(input)?;
    let (input, op) = alt((
        value(CmpOp::Gte, tag(">=")),
        value(CmpOp::Gt, tag(">")),
        value(CmpOp::Lte, tag("<=")),
        value(CmpOp::Lt, tag("<")),
        value(CmpOp::Neq, tag("!=")),
        value(CmpOp::Eq, tag("==")),
    ))(input)?;
    let (input, _) = multispace0(input)?;
    let (input, lit) = parse_literal(input)?;
    Ok((input, Expr::Compare(field, op, lit)))
}

fn parse_literal(input: &str) -> IResult<&str, LitValue> {
    alt((parse_string_literal, parse_num_literal))(input)
}

fn parse_string_literal(input: &str) -> IResult<&str, LitValue> {
    let (input, _) = char('"')(input)?;
    let (input, s) = take_while(|c: char| c != '"')(input)?;
    let (input, _) = char('"')(input)?;
    Ok((input, LitValue::Str(s.to_string())))
}

fn parse_num_literal(input: &str) -> IResult<&str, LitValue> {
    map(double, LitValue::Num)(input)
}

// === Evaluator ===

fn resolve_field<'a>(data: &'a Value, field: &str) -> &'a Value {
    let mut current = data;
    for part in field.split('.') {
        current = current.get(part).unwrap_or(&Value::Null);
    }
    current
}

fn eval_expr(expr: &Expr, data: &Value) -> bool {
    match expr {
        Expr::And(a, b) => eval_expr(a, data) && eval_expr(b, data),
        Expr::Or(a, b) => eval_expr(a, data) || eval_expr(b, data),
        Expr::IsNull(f) => resolve_field(data, f).is_null(),
        Expr::IsNotNull(f) => !resolve_field(data, f).is_null(),
        Expr::IsEmpty(f) => {
            let v = resolve_field(data, f);
            v.is_null()
                || v.as_array().is_some_and(|a| a.is_empty())
                || v.as_str().is_some_and(|s| s.is_empty())
        }
        Expr::IsNotEmpty(f) => {
            let v = resolve_field(data, f);
            !v.is_null()
                && !v.as_str().is_some_and(|s| s.is_empty())
                && !v.as_array().is_some_and(|a| a.is_empty())
        }
        Expr::Compare(f, op, lit) => {
            let val = resolve_field(data, f);
            match (val, lit) {
                (Value::Number(n), LitValue::Num(l)) => {
                    let n = n.as_f64().unwrap_or(0.0);
                    match op {
                        CmpOp::Gt => n > *l,
                        CmpOp::Gte => n >= *l,
                        CmpOp::Lt => n < *l,
                        CmpOp::Lte => n <= *l,
                        CmpOp::Eq => (n - *l).abs() < f64::EPSILON,
                        CmpOp::Neq => (n - *l).abs() >= f64::EPSILON,
                    }
                }
                (Value::String(s), LitValue::Str(l)) => match op {
                    CmpOp::Eq => s == l,
                    CmpOp::Neq => s != l,
                    CmpOp::Gt => s.as_str() > l.as_str(),
                    CmpOp::Gte => s.as_str() >= l.as_str(),
                    CmpOp::Lt => s.as_str() < l.as_str(),
                    CmpOp::Lte => s.as_str() <= l.as_str(),
                },
                (_, LitValue::Str(_) | LitValue::Num(_)) => matches!(op, CmpOp::Neq),
            }
        }
        Expr::In(f, values) => {
            let val = resolve_field(data, f);
            values.iter().any(|lit| match (val, lit) {
                (Value::String(s), LitValue::Str(l)) => s == l,
                (Value::Number(n), LitValue::Num(l)) => {
                    n.as_f64().is_some_and(|n| (n - *l).abs() < f64::EPSILON)
                }
                _ => false,
            })
        }
    }
}
