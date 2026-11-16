//! Typed repository queries (DD9, issue #659).
//!
//! pherolang lowers a declared repository operation's `where`/`sort`/`page`
//! clauses to a read-only [`QuerySpec`](phero_ir::ddd::QuerySpec): the
//! predicate as **verbatim, already-typed source text** (scoped to the
//! designed class's features plus the operation's parameters), each sort key
//! with its **resolved** field type, and the pagination window. This module
//! lowers that spec to a provider-neutral plan and renders it for both
//! repository providers.
//!
//! # The closed subset
//!
//! | pherolang                                  | lowered form            |
//! |--------------------------------------------|-------------------------|
//! | `self.x` / bare `x` (a stored scalar)      | column reference        |
//! | a bare operation parameter                 | bind parameter          |
//! | `123`, `-1.5`, `"text"`, `true`, `false`   | bound literal           |
//! | `date("YYYY-MM-DD")`                       | bound date literal      |
//! | `==`/`!=`/`<`/`<=`/`>`/`>=`                | comparison              |
//! | `&&`/`\|\|`/`!`                            | AND/OR/NOT (pushed down)|
//! | parentheses                                | preserved               |
//! | `field == null` / `field != null`          | IS NULL / IS NOT NULL   |
//! | a bare `bool` column                       | `= TRUE` (bound)        |
//!
//! **Every value is a bind parameter** — parameter values arrive as function
//! arguments, and even predicate literals are bound rather than interpolated,
//! so string interpolation is structurally impossible. This is the one
//! deliberate difference from [`crate::db::expr_sql`], which inlines
//! literals into RLS predicates (there is no bind surface in a `USING`
//! expression).
//!
//! Anything else — arithmetic, `?.`, `?:`, `let`, lambdas, collection
//! algebra, `if`, operation calls, package-function calls, list literals,
//! class-typed parameter navigation, column-to-column comparisons, a bare
//! boolean PARAMETER as the predicate — is a NAMED rejection (the
//! Cedar-backend philosophy): generation fails with a message naming the
//! repository, the operation, and the construct. A refusal never produces a
//! broader query.
//!
//! `sort` on a codelist/enum feature orders the column's **stored code**
//! (the text PostgreSQL keeps, e.g. `"D"`), which is the only stored order —
//! the DSL's sort key is not a ranking.
//!
//! `page cursor` is accepted but lowers an offset page (`LIMIT`/`OFFSET`)
//! with a one-time WARN: the keyset predicate on the sort keys plus identity
//! is a recorded deferral (issue #659), never a silent ignore.

use serde::Serialize;

use phero_expr::{BinOp, Expr, ExprKind, UnOp};

use crate::ddd::design::FinderParam;

/// What a predicated feature resolves to in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryColumnKind {
    /// A stored scalar column — the only kind a query may compare or sort.
    Scalar,
    /// An entity reference (ordering needs a join; comparing needs the id).
    ClassReference,
    /// A collection (no stored scalar order).
    Collection,
    /// A contained value object (its columns live in a child table).
    Contained,
}

/// One resolved entity feature a query may reference: the authored feature
/// name, its Rust field, its PostgreSQL column, and the storage type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryColumn {
    /// The authored feature name (`.mox` spelling, e.g. `runtimeMinutes`).
    pub feature: String,
    /// The snake_cased Rust field name (`runtime_minutes`).
    pub field: String,
    /// The PostgreSQL column (`runtime_minutes`).
    pub pg_column: String,
    /// The column's Rust type (the STORAGE type — a codelist column is text).
    pub rust_type: String,
    /// Whether the column is nullable.
    pub is_nullable: bool,
    /// What the feature is — only [`QueryColumnKind::Scalar`] is comparable.
    pub kind: QueryColumnKind,
}

impl QueryColumn {
    /// The SeaORM `Column` variant name for this column.
    pub fn variant(&self) -> String {
        let bare = self.field.strip_prefix("r#").unwrap_or(&self.field);
        codegraph_naming::to_pascal_case(bare)
    }
}

/// A comparison operator of the closed subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl CmpOp {
    /// The SQL spelling.
    pub fn sql(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Ne => "<>",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }

    /// The SeaORM `ColumnTrait` method.
    pub fn seaorm(self) -> &'static str {
        match self {
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
            CmpOp::Lt => "lt",
            CmpOp::Le => "lte",
            CmpOp::Gt => "gt",
            CmpOp::Ge => "gte",
        }
    }

    /// The operator of the logically negated comparison (`NOT (a < b)` ⇒
    /// `a >= b`; three-valued logic agrees — both sides are unknown on
    /// NULL).
    fn negated(self) -> CmpOp {
        match self {
            CmpOp::Eq => CmpOp::Ne,
            CmpOp::Ne => CmpOp::Eq,
            CmpOp::Lt => CmpOp::Ge,
            CmpOp::Le => CmpOp::Gt,
            CmpOp::Gt => CmpOp::Le,
            CmpOp::Ge => CmpOp::Lt,
        }
    }

    /// The operator with the comparison's operands swapped (`a < b` ⇒
    /// `b > a`).
    fn swapped(self) -> CmpOp {
        match self {
            CmpOp::Eq => CmpOp::Eq,
            CmpOp::Ne => CmpOp::Ne,
            CmpOp::Lt => CmpOp::Gt,
            CmpOp::Le => CmpOp::Ge,
            CmpOp::Gt => CmpOp::Lt,
            CmpOp::Ge => CmpOp::Le,
        }
    }
}

/// A literal value appearing in a predicate. Bound, never interpolated.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum QueryLiteral {
    /// An integer literal.
    Int(i64),
    /// A floating-point literal.
    Float(f64),
    /// A string literal.
    String(String),
    /// A boolean literal.
    Bool(bool),
    /// A `date("YYYY-MM-DD")` literal, carried as its validated ISO text.
    Date(String),
}

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "source", content = "value")]
pub enum QueryOperand {
    /// An operation parameter — the index into the operation's declared
    /// parameter list.
    Param(usize),
    /// A predicate literal.
    Literal(QueryLiteral),
}

/// One non-negated predicate atom.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "atom")]
pub enum QueryAtom {
    /// A comparison between a column and a parameter/literal.
    Cmp {
        /// The compared column.
        column: QueryColumn,
        /// The comparison operator.
        op: CmpOp,
        /// The compared value.
        value: QueryOperand,
    },
    /// `column IS NULL` / `column IS NOT NULL`.
    Null {
        /// The tested column.
        column: QueryColumn,
        /// `true` for `IS NULL`, `false` for `IS NOT NULL`.
        is_null: bool,
    },
    /// A bare boolean column as a predicate (`where (self.active)`).
    Bool {
        /// The boolean column.
        column: QueryColumn,
        /// `true` for `= TRUE`, `false` for `= FALSE`.
        expected: bool,
    },
}

/// A negation-free predicate tree: atoms combined with AND/OR.
///
/// Externally tagged (`{"And": […]}`): the plan rides the Tera context, and
/// an internally tagged newtype variant holding a sequence is not
/// serializable. Nothing reads the tag shape programmatically.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryPredicate {
    /// A single atom.
    Atom(QueryAtom),
    /// Every child must hold.
    And(Vec<QueryPredicate>),
    /// At least one child must hold.
    Or(Vec<QueryPredicate>),
}

impl QueryPredicate {
    /// Combine `children` with AND, normalizing the degenerate shapes (a
    /// single child is the child; none yields `None`).
    fn and(children: Vec<QueryPredicate>) -> Option<QueryPredicate> {
        if children.is_empty() {
            return None;
        }
        let mut children = children;
        Some(if children.len() == 1 {
            children.remove(0)
        } else {
            QueryPredicate::And(children)
        })
    }

    /// Combine `children` with OR, normalizing the degenerate shapes.
    fn or(children: Vec<QueryPredicate>) -> Option<QueryPredicate> {
        if children.is_empty() {
            return None;
        }
        let mut children = children;
        Some(if children.len() == 1 {
            children.remove(0)
        } else {
            QueryPredicate::Or(children)
        })
    }

    /// The logical negation, pushed all the way down to the atoms.
    fn negate(self) -> QueryPredicate {
        match self {
            QueryPredicate::Atom(QueryAtom::Cmp { column, op, value }) => {
                QueryPredicate::Atom(QueryAtom::Cmp {
                    column,
                    op: op.negated(),
                    value,
                })
            }
            QueryPredicate::Atom(QueryAtom::Null { column, is_null }) => {
                QueryPredicate::Atom(QueryAtom::Null {
                    column,
                    is_null: !is_null,
                })
            }
            QueryPredicate::Atom(QueryAtom::Bool { column, expected }) => {
                QueryPredicate::Atom(QueryAtom::Bool {
                    column,
                    expected: !expected,
                })
            }
            QueryPredicate::And(children) => {
                QueryPredicate::Or(children.into_iter().map(QueryPredicate::negate).collect())
            }
            QueryPredicate::Or(children) => {
                QueryPredicate::And(children.into_iter().map(QueryPredicate::negate).collect())
            }
        }
    }

    /// Every atom in walk order.
    pub fn atoms(&self) -> Vec<&QueryAtom> {
        let mut out = Vec::new();
        self.collect_atoms(&mut out);
        out
    }

    fn collect_atoms<'a>(&'a self, out: &mut Vec<&'a QueryAtom>) {
        match self {
            QueryPredicate::Atom(atom) => out.push(atom),
            QueryPredicate::And(children) | QueryPredicate::Or(children) => {
                for child in children {
                    child.collect_atoms(out);
                }
            }
        }
    }
}

/// One resolved sort key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryOrder {
    /// The ordered column.
    pub column: QueryColumn,
    /// `true` for `DESC`, `false` for `ASC` (the DSL default).
    pub descending: bool,
}

/// The pagination window of a lowered query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct QueryPage {
    /// The default page size (`page limit n`).
    pub limit: Option<u32>,
    /// The caller-visible cap (`page max n`).
    pub max: Option<u32>,
    /// Whether the DSL declared cursor paging. v1 lowers an offset page and
    /// warns once (the keyset predicate is a recorded deferral).
    pub cursor: bool,
}

/// The provider-neutral lowering of one `QuerySpec`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryPlan {
    /// The predicate tree (always present — the DSL requires `where`).
    pub predicate: QueryPredicate,
    /// The order-by keys, in declaration order.
    pub order: Vec<QueryOrder>,
    /// The pagination window; `None` when the DSL declared no `page`.
    pub page: Option<QueryPage>,
    /// The resolved read capabilities guarding the query. Recorded on the
    /// generated method's doc comment — runtime enforcement rides the API
    /// plane (issue #659's recorded follow-up).
    pub capabilities: Vec<String>,
    /// The indices of the parameters the predicate references, in
    /// DECLARATION order (deduped) — the generated impl binds exactly these.
    pub params: Vec<usize>,
}

impl QueryPlan {
    /// Whether the plan declares pagination (the generated method then takes
    /// `limit`/`offset` runtime parameters).
    pub fn has_page(&self) -> bool {
        self.page.is_some()
    }
}

/// The generated method's page-size parameter name.
pub const PAGE_LIMIT_PARAM: &str = "limit";

/// The generated method's page-offset parameter name.
pub const PAGE_OFFSET_PARAM: &str = "offset";

/// The SQL bind name for the page size (`LIMIT :page_size`). Deliberately
/// not `limit`, which is a SQL keyword.
pub const PAGE_SIZE_BIND: &str = "page_size";

/// The SQL bind name for the page offset (`OFFSET :offset`, the emitted
/// list query's convention).
pub const PAGE_OFFSET_BIND: &str = "offset";

/// The Rust expression producing the effective page size (`u64`) in a
/// generated impl: the caller's `limit` defaults to the DSL's `page limit`
/// and is capped by the DSL's `page max`. With neither declared the page is
/// unbounded — the author declared no bound — so `None` survives.
pub fn page_size_expr(page: &QueryPage) -> String {
    let base = match page.limit.or(page.max) {
        Some(default) => format!("{PAGE_LIMIT_PARAM}.or(Some({default}u64))"),
        None => PAGE_LIMIT_PARAM.to_string(),
    };
    match page.max {
        Some(max) => format!("{base}.map(|page_size| page_size.min({max}u64))"),
        None => base,
    }
}

/// The same page size as an `i64` for a cornucopia bind: an unbounded page
/// binds `i64::MAX`, the largest `LIMIT` PostgreSQL accepts (there is no
/// nullable `LIMIT` bind — the header's nullability idiom is reserved for
/// the `findByExample` filters).
pub fn page_size_i64_expr(page: &QueryPage) -> String {
    format!("{}.unwrap_or(i64::MAX as u64) as i64", page_size_expr(page))
}

/// A named bind of the rendered SQL, in first-occurrence order.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlArg {
    /// The placeholder name (`:name` in the SQL, and the cornucopia header
    /// parameter name).
    pub name: String,
    /// The explicit SQL cast appended to the placeholder (`::bigint`,
    /// `::text`, …). Every bind carries one: cornucopia infers a parameter's
    /// type from the SQL, so the cast pins the bind to the Rust type the
    /// adapter passes instead of letting a column's narrower type (an
    /// `INTEGER` column vs an `i64` parameter) choose it.
    pub cast: &'static str,
    /// What the caller binds.
    pub source: QueryOperand,
}

/// The SQL cast pinning a parameter bind to its Rust type.
fn param_cast(rust_type: &str) -> &'static str {
    if rust_type.ends_with("Uuid") {
        "::uuid"
    } else if rust_type.starts_with('i') || rust_type.starts_with('u') {
        "::bigint"
    } else if rust_type.starts_with('f') {
        "::double precision"
    } else if rust_type == "bool" {
        "::boolean"
    } else if rust_type.ends_with("NaiveDate") {
        "::date"
    } else if rust_type.contains("DateTime") || rust_type.contains("Timestamp") {
        "::timestamptz"
    } else if rust_type.ends_with("Decimal") {
        "::numeric"
    } else {
        "::text"
    }
}

/// The SQL cast for a predicate literal bind.
fn literal_cast(literal: &QueryLiteral) -> &'static str {
    match literal {
        QueryLiteral::Int(_) => "::bigint",
        QueryLiteral::Float(_) => "::double precision",
        QueryLiteral::String(_) => "::text",
        QueryLiteral::Bool(_) => "::boolean",
        QueryLiteral::Date(_) => "::date",
    }
}

/// Whether a query parameter binds as TEXT (with an explicit cast) rather
/// than its natural postgres type — avoids depending on the generated
/// queries crate's chrono/numeric `ToSql` features. The temporal/decimal
/// values round-trip through their ISO/plain text forms, which PostgreSQL
/// parses under the pinned cast.
pub fn query_param_binds_as_text(rust_type: &str) -> bool {
    rust_type.ends_with("NaiveDate")
        || rust_type.contains("DateTime")
        || rust_type.contains("Timestamp")
        || rust_type.ends_with("Decimal")
}

/// The cornucopia rendering of a [`QueryPlan`]: predicate SQL, ORDER BY
/// clause, and the ordered bind list.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlQuery {
    /// The predicate, e.g. `("title" = :title AND "runtime_minutes" >= :m)`.
    pub predicate: String,
    /// The full ORDER BY clause (never empty — the identity tiebreaker is
    /// always last).
    pub order_by: String,
    /// The binds, in first-occurrence order (matching the SQL placeholders).
    pub args: Vec<SqlArg>,
}

/// Resolve `name` against the query column universe: the authored feature
/// name, the Rust field name, or the PostgreSQL column name (the `?sort=`
/// allow-list convention). An ambiguous match is a refusal, never a guess.
fn resolve_column<'a>(
    label: &str,
    name: &str,
    columns: &'a [QueryColumn],
) -> Result<&'a QueryColumn, String> {
    let snake = codegraph_naming::to_snake_case(name);
    let matches: Vec<&QueryColumn> = columns
        .iter()
        .filter(|column| {
            let bare = column.field.strip_prefix("r#").unwrap_or(&column.field);
            column.feature == name || bare == name || column.pg_column == name || bare == snake
        })
        .collect();
    match matches.as_slice() {
        [column] => Ok(column),
        [] => Err(format!(
            "{label}: `{name}` is neither a parameter of the operation nor a feature of the \
             designed class"
        )),
        _ => Err(format!(
            "{label}: `{name}` is ambiguous — it names {} features ({})",
            matches.len(),
            matches
                .iter()
                .map(|column| column.feature.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Require a resolved column to be a stored scalar (the only comparable /
/// orderable kind).
fn scalar_column<'a>(label: &str, column: &'a QueryColumn) -> Result<&'a QueryColumn, String> {
    match column.kind {
        QueryColumnKind::Scalar => Ok(column),
        QueryColumnKind::ClassReference => Err(format!(
            "{label}: `{}` is a class reference — comparing it needs a join; compare the \
             `{}` column instead",
            column.feature, column.pg_column
        )),
        QueryColumnKind::Collection => Err(format!(
            "{label}: `{}` is a collection — it has no stored scalar value to compare or order",
            column.feature
        )),
        QueryColumnKind::Contained => Err(format!(
            "{label}: `{}` is a contained value — its columns live in a child table; query the \
             root's feature instead",
            column.feature
        )),
    }
}

/// The shared refusal text for every out-of-subset construct.
fn unsupported(label: &str, construct: &str) -> String {
    format!(
        "{label}: {construct} cannot be lowered to a SQL predicate; the supported subset is \
         stored scalar feature refs, operation parameters, string/number/boolean/date literals, \
         == != < <= > >=, && || !, parentheses, and null comparisons"
    )
}

/// One side of a parsed comparison, before the orientation is known.
enum Side<'a> {
    Column(&'a QueryColumn),
    Value(QueryOperand),
}

/// The context one predicate lowering runs in.
struct PredicateContext<'a> {
    label: &'a str,
    params: &'a [FinderParam],
    columns: &'a [QueryColumn],
}

impl PredicateContext<'_> {
    /// Resolve a bare name: an operation parameter first (the upstream
    /// compiler refuses a parameter/feature collision, so the order is
    /// unambiguous), then a stored feature.
    fn resolve_name<'a>(&'a self, name: &str) -> Result<Side<'a>, String> {
        // Predicate names are authored (`targetGenre`); `FinderParam` stores
        // the snake_cased Rust name the signature uses.
        let snake = codegraph_naming::to_snake_case(name);
        let matched: Vec<usize> = self
            .params
            .iter()
            .enumerate()
            .filter(|(_, param)| param.name == name || param.name == snake)
            .map(|(index, _)| index)
            .collect();
        match matched.as_slice() {
            [index] => {
                let param = &self.params[*index];
                if param.rust_type.starts_with("Vec<") {
                    return Err(unsupported(
                        self.label,
                        &format!("the collection parameter `{name}`"),
                    ));
                }
                return Ok(Side::Value(QueryOperand::Param(*index)));
            }
            [] => {}
            _ => {
                return Err(format!(
                    "{label}: `{name}` is ambiguous — it names {} operation parameters",
                    matched.len(),
                    label = self.label
                ));
            }
        }
        let column = resolve_column(self.label, name, self.columns)?;
        let column = scalar_column(self.label, column)?;
        Ok(Side::Column(column))
    }

    /// Lower an expression to a comparison operand.
    fn operand<'a>(&'a self, expr: &Expr) -> Result<Side<'a>, String> {
        match &expr.kind {
            ExprKind::Int(value) => Ok(Side::Value(QueryOperand::Literal(QueryLiteral::Int(
                *value,
            )))),
            ExprKind::Float(value) => Ok(Side::Value(QueryOperand::Literal(QueryLiteral::Float(
                *value,
            )))),
            ExprKind::String(text) => Ok(Side::Value(QueryOperand::Literal(QueryLiteral::String(
                text.clone(),
            )))),
            ExprKind::Bool(value) => Ok(Side::Value(QueryOperand::Literal(QueryLiteral::Bool(
                *value,
            )))),
            ExprKind::Date { text, .. } => {
                // The upstream checker validated the calendar date; validate
                // again so a hand-written artifact cannot smuggle arbitrary
                // text into the SQL cast (the `expr_sql` rule).
                if crate::db::expr_sql::parse_date_text(text).is_some() {
                    Ok(Side::Value(QueryOperand::Literal(QueryLiteral::Date(
                        text.clone(),
                    ))))
                } else {
                    Err(format!(
                        "{label}: the date literal {text:?} is not a calendar date `YYYY-MM-DD`",
                        label = self.label
                    ))
                }
            }
            ExprKind::Null => Err(unsupported(self.label, "`null` on its own")),
            ExprKind::Name(name) => self.resolve_name(name),
            ExprKind::FeatureAccess {
                receiver,
                name,
                optional_safe,
            } => {
                if *optional_safe {
                    return Err(unsupported(self.label, "safe navigation (`?.`)"));
                }
                match &receiver.kind {
                    // `self.<feature>` — the explicit feature spelling.
                    ExprKind::Name(root) if root == "self" => {
                        let column = resolve_column(self.label, &name.value, self.columns)?;
                        let column = scalar_column(self.label, column)?;
                        Ok(Side::Column(column))
                    }
                    // `<param>.<field>` — class-typed parameter navigation.
                    _ => Err(unsupported(
                        self.label,
                        &format!("the navigation `{}.{}`", write_path(receiver), name.value),
                    )),
                }
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                expr: inner,
            } => match &inner.kind {
                ExprKind::Int(value) => Ok(Side::Value(QueryOperand::Literal(QueryLiteral::Int(
                    -value,
                )))),
                ExprKind::Float(value) => Ok(Side::Value(QueryOperand::Literal(
                    QueryLiteral::Float(-value),
                ))),
                _ => Err(unsupported(
                    self.label,
                    "negation of a non-literal (arithmetic)",
                )),
            },
            ExprKind::Unary { op: UnOp::Not, .. } => {
                Err(unsupported(self.label, "`!` on a non-boolean value"))
            }
            ExprKind::Binary { op, .. } => Err(unsupported(
                self.label,
                &format!("arithmetic (`{op}`) inside a comparison operand"),
            )),
            ExprKind::Coalesce { .. } => Err(unsupported(self.label, "`?:`")),
            ExprKind::Call { receiver, name, .. } => Err(unsupported(
                self.label,
                &format!(
                    "an operation call (`{}.{}`)",
                    write_path(receiver),
                    name.value
                ),
            )),
            ExprKind::FunctionCall { name, .. } => Err(unsupported(
                self.label,
                &format!("a package-function call (`{}`)", name.value),
            )),
            ExprKind::Algebra { kind, .. } => Err(unsupported(
                self.label,
                &format!("collection algebra `{kind}`"),
            )),
            ExprKind::If { .. } => Err(unsupported(self.label, "`if`")),
            ExprKind::Let { .. } => Err(unsupported(self.label, "`let`")),
            ExprKind::Lambda { .. } => Err(unsupported(self.label, "a lambda")),
            ExprKind::ListLiteral(_) => Err(unsupported(self.label, "list literals")),
        }
    }

    /// Lower an expression to a boolean predicate, or `None` when it is a
    /// bare value that the caller must turn into an atom.
    fn predicate(&self, expr: &Expr) -> Result<Option<QueryPredicate>, String> {
        match &expr.kind {
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And | BinOp::Or => {
                    let left = self.boolean_operand(lhs)?;
                    let right = self.boolean_operand(rhs)?;
                    let predicate = if matches!(op, BinOp::And) {
                        QueryPredicate::and(vec![left, right])
                    } else {
                        QueryPredicate::or(vec![left, right])
                    };
                    Ok(predicate)
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    Ok(Some(self.comparison(*op, lhs, rhs)?))
                }
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    Err(unsupported(self.label, &format!("arithmetic (`{op}`)")))
                }
            },
            ExprKind::Unary {
                op: UnOp::Not,
                expr: inner,
            } => {
                let inner = self.boolean_operand(inner)?;
                Ok(Some(inner.negate()))
            }
            _ => Ok(None),
        }
    }

    /// Lower an expression that must be boolean: a nested predicate or a
    /// bare boolean column.
    fn boolean_operand(&self, expr: &Expr) -> Result<QueryPredicate, String> {
        if let Some(predicate) = self.predicate(expr)? {
            return Ok(predicate);
        }
        match self.operand(expr)? {
            Side::Column(column) => {
                if column.rust_type != "bool" {
                    return Err(unsupported(
                        self.label,
                        &format!(
                            "the non-boolean feature `{}` used as a predicate",
                            column.feature
                        ),
                    ));
                }
                Ok(QueryPredicate::Atom(QueryAtom::Bool {
                    column: column.clone(),
                    expected: true,
                }))
            }
            Side::Value(_) => Err(unsupported(
                self.label,
                &format!("`{}` used as a predicate", write_path(expr)),
            )),
        }
    }

    /// Lower one comparison, orienting it column-op-value (`value < column`
    /// becomes `column > value`) and refusing every shape the store cannot
    /// express.
    fn comparison(&self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<QueryPredicate, String> {
        let op = match op {
            BinOp::Eq => CmpOp::Eq,
            BinOp::Ne => CmpOp::Ne,
            BinOp::Lt => CmpOp::Lt,
            BinOp::Le => CmpOp::Le,
            BinOp::Gt => CmpOp::Gt,
            BinOp::Ge => CmpOp::Ge,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::And | BinOp::Or => {
                return Err(unsupported(
                    self.label,
                    &format!("the operator `{op}` in a comparison"),
                ));
            }
        };
        // Null comparisons are structural (`IS NULL`), so they are decided
        // before either side is lowered as a value.
        let null_side = matches!(lhs.kind, ExprKind::Null) || matches!(rhs.kind, ExprKind::Null);
        if null_side {
            if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                return Err(unsupported(
                    self.label,
                    "an ordering comparison against `null`",
                ));
            }
            let other = if matches!(lhs.kind, ExprKind::Null) {
                self.operand(rhs)?
            } else {
                self.operand(lhs)?
            };
            return match other {
                Side::Column(column) => {
                    if column.is_nullable {
                        Ok(QueryPredicate::Atom(QueryAtom::Null {
                            column: column.clone(),
                            is_null: op == CmpOp::Eq,
                        }))
                    } else {
                        // A non-nullable column compared to null is a
                        // constant, not a filter — refusing is safer than
                        // quietly returning every row.
                        Err(unsupported(
                            self.label,
                            &format!(
                                "`{}` compared to `null` (the column is not nullable)",
                                column.feature
                            ),
                        ))
                    }
                }
                Side::Value(_) => Err(unsupported(
                    self.label,
                    "a parameter compared to `null` (parameters are not nullable)",
                )),
            };
        }
        let left = self.operand(lhs)?;
        let right = self.operand(rhs)?;
        let (column, op, value) = match (left, right) {
            (Side::Column(column), Side::Value(value)) => (column, op, value),
            (Side::Value(value), Side::Column(column)) => (column, op.swapped(), value),
            (Side::Column(_), Side::Column(_)) => {
                return Err(unsupported(self.label, "a comparison between two columns"));
            }
            (Side::Value(_), Side::Value(_)) => {
                return Err(unsupported(
                    self.label,
                    "a comparison between two values with no column",
                ));
            }
        };
        // An ordering comparison is only meaningful on an orderable storage
        // type — a boolean column ordered with `<` is a modeling bug the
        // store would otherwise accept as `false < true`.
        if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge) && column.rust_type == "bool"
        {
            return Err(unsupported(
                self.label,
                &format!(
                    "an ordering comparison on the boolean feature `{}`",
                    column.feature
                ),
            ));
        }
        Ok(QueryPredicate::Atom(QueryAtom::Cmp {
            column: column.clone(),
            op,
            value,
        }))
    }
}

/// Render an expression path back to source-like text for diagnostics.
fn write_path(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Name(name) => name.clone(),
        ExprKind::FeatureAccess {
            receiver,
            name,
            optional_safe,
        } => format!(
            "{}{}{}",
            write_path(receiver),
            if *optional_safe { "?." } else { "." },
            name.value
        ),
        ExprKind::String(text) => format!("{text:?}"),
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Float(value) => value.to_string(),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Null => "null".to_string(),
        ExprKind::Call { receiver, name, .. } => {
            format!("{}.{}()", write_path(receiver), name.value)
        }
        ExprKind::FunctionCall { name, .. } => format!("{}()", name.value),
        _ => "<expression>".to_string(),
    }
}

/// Lower a `QuerySpec`'s predicate source text, sort keys, and pagination.
///
/// `label` is the human prefix embedded in every refusal — the caller passes
/// `query \`{Repository}.{operation}\`` so a rejection always names both.
pub fn lower_query_spec(
    spec: &phero_ir::ddd::QuerySpec,
    label: &str,
    capabilities: &[String],
    params: &[FinderParam],
    columns: &[QueryColumn],
) -> Result<QueryPlan, String> {
    if !spec.read_only {
        return Err(format!(
            "{label}: the query is not read-only (`readOnly: false`) — a repository query is a \
             read"
        ));
    }
    let context = PredicateContext {
        label,
        params,
        columns,
    };
    let parsed = phero_expr::parse(&spec.predicate);
    let ast = match (parsed.ast, parsed.errors.is_empty()) {
        (Some(ast), true) => ast,
        _ => {
            let detail = parsed
                .errors
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "{label}: the predicate is not valid pherolang expression syntax \
                 ({}): {detail}",
                spec.predicate
            ));
        }
    };
    let Some(predicate) = context.predicate(&ast)? else {
        // A bare boolean column is the only non-Binary predicate the subset
        // admits; anything else is not a predicate at all.
        let predicate = context.boolean_operand(&ast)?;
        return finish_plan(predicate, spec, label, capabilities, columns);
    };
    finish_plan(predicate, spec, label, capabilities, columns)
}

/// Resolve the sort keys, record the referenced parameters, and assemble the
/// plan.
fn finish_plan(
    predicate: QueryPredicate,
    spec: &phero_ir::ddd::QuerySpec,
    label: &str,
    capabilities: &[String],
    columns: &[QueryColumn],
) -> Result<QueryPlan, String> {
    let mut order = Vec::with_capacity(spec.sort.len());
    for key in &spec.sort {
        let column = resolve_column(label, &key.field, columns).map_err(|err| {
            format!(
                "{label}: sort key `{}` cannot be resolved — {err}",
                key.field
            )
        })?;
        let column = scalar_column(label, column).map_err(|err| {
            format!(
                "{label}: sort key `{}` cannot be ordered — {err}",
                key.field
            )
        })?;
        let descending = matches!(key.direction, phero_ir::ddd::SortDirection::Desc);
        order.push(QueryOrder {
            column: column.clone(),
            descending,
        });
    }
    let page = spec.pagination.map(|pagination| QueryPage {
        limit: pagination.limit,
        max: pagination.max,
        cursor: pagination.cursor,
    });
    if let Some(page) = page
        && page.cursor
    {
        warn_keyset_deferral(label);
    }
    // The referenced parameters, in declaration order (deduped) — the
    // generated impl binds exactly these, so an unused declared parameter
    // never reaches the SQL.
    let mut used: Vec<usize> = Vec::new();
    for atom in predicate.atoms() {
        match atom {
            QueryAtom::Cmp { value, .. } => {
                if let QueryOperand::Param(index) = value
                    && !used.contains(index)
                {
                    used.push(*index);
                }
            }
            QueryAtom::Null { .. } | QueryAtom::Bool { .. } => {}
        }
    }
    used.sort_unstable();
    Ok(QueryPlan {
        predicate,
        order,
        page,
        capabilities: capabilities.to_vec(),
        params: used,
    })
}

/// One-time stderr warning: cursor paging lowers an offset page in v1.
fn warn_keyset_deferral(label: &str) {
    static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let set = WARNED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
    if let Ok(mut set) = set.lock()
        && set.insert(label.to_string())
    {
        eprintln!(
            "warning: ddd design: {label}: `page cursor` declared — v1 lowers LIMIT/OFFSET \
             paging; keyset pagination is a recorded deferral (issue #659)"
        );
    }
}

// ── SQL rendering (cornucopia) ──────────────────────────────────────────

/// Allocate a unique placeholder name for a literal bind.
fn literal_name(taken: &[String], next: &mut usize) -> String {
    loop {
        let candidate = format!("_lit{}", *next);
        *next += 1;
        if !taken.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
}

/// Render the plan as cornucopia SQL: a parameterized predicate, the ORDER
/// BY clause, and the ordered bind list.
pub fn render_sql(plan: &QueryPlan, params: &[FinderParam]) -> SqlQuery {
    let mut args: Vec<SqlArg> = Vec::new();
    let mut taken: Vec<String> = params.iter().map(|param| param.name.clone()).collect();
    let mut next_literal = 0usize;
    let predicate = render_sql_predicate(
        &plan.predicate,
        params,
        &mut args,
        &mut taken,
        &mut next_literal,
    );
    let mut order_by: Vec<String> = plan
        .order
        .iter()
        .map(|key| {
            format!(
                "\"{}\" {}",
                key.column.pg_column,
                if key.descending { "DESC" } else { "ASC" }
            )
        })
        .collect();
    if !plan
        .order
        .iter()
        .any(|key| key.column.pg_column == "id" || key.column.field == "id")
    {
        order_by.push("\"id\" ASC".to_string());
    }
    SqlQuery {
        predicate,
        order_by: order_by.join(", "),
        args,
    }
}

fn render_sql_predicate(
    predicate: &QueryPredicate,
    params: &[FinderParam],
    args: &mut Vec<SqlArg>,
    taken: &mut Vec<String>,
    next_literal: &mut usize,
) -> String {
    match predicate {
        QueryPredicate::Atom(atom) => render_sql_atom(atom, params, args, taken, next_literal),
        QueryPredicate::And(children) => format!(
            "({})",
            children
                .iter()
                .map(|child| render_sql_predicate(child, params, args, taken, next_literal))
                .collect::<Vec<_>>()
                .join(" AND ")
        ),
        QueryPredicate::Or(children) => format!(
            "({})",
            children
                .iter()
                .map(|child| render_sql_predicate(child, params, args, taken, next_literal))
                .collect::<Vec<_>>()
                .join(" OR ")
        ),
    }
}

fn render_sql_atom(
    atom: &QueryAtom,
    params: &[FinderParam],
    args: &mut Vec<SqlArg>,
    taken: &mut Vec<String>,
    next_literal: &mut usize,
) -> String {
    match atom {
        QueryAtom::Null { column, is_null } => format!(
            "\"{}\" {}",
            column.pg_column,
            if *is_null { "IS NULL" } else { "IS NOT NULL" }
        ),
        QueryAtom::Bool { column, expected } => {
            let name = bind_literal(
                &QueryOperand::Literal(QueryLiteral::Bool(*expected)),
                args,
                taken,
                next_literal,
            );
            format!("\"{}\" = :{name}::boolean", column.pg_column)
        }
        QueryAtom::Cmp { column, op, value } => {
            let name = bind_operand(value, params, args, taken, next_literal);
            // Every bind is cast to the type the adapter passes: cornucopia
            // infers a parameter's type from the SQL, and a column's
            // narrower storage type (INTEGER vs an i64 parameter) would
            // otherwise choose the generated signature.
            let cast = match value {
                QueryOperand::Param(index) => params
                    .get(*index)
                    .map(|param| param_cast(&param.rust_type))
                    .unwrap_or("::text"),
                QueryOperand::Literal(literal) => literal_cast(literal),
            };
            format!("\"{}\" {} :{name}{cast}", column.pg_column, op.sql())
        }
    }
}

fn bind_operand(
    value: &QueryOperand,
    params: &[FinderParam],
    args: &mut Vec<SqlArg>,
    taken: &mut Vec<String>,
    next_literal: &mut usize,
) -> String {
    match value {
        QueryOperand::Param(index) => {
            let name = params
                .get(*index)
                .map(|param| param.name.clone())
                .unwrap_or_else(|| format!("param{index}"));
            if !args.iter().any(|arg| arg.name == name) {
                args.push(SqlArg {
                    name: name.clone(),
                    cast: params
                        .get(*index)
                        .map(|param| param_cast(&param.rust_type))
                        .unwrap_or("::text"),
                    source: QueryOperand::Param(*index),
                });
            }
            name
        }
        QueryOperand::Literal(literal) => bind_literal(
            &QueryOperand::Literal(literal.clone()),
            args,
            taken,
            next_literal,
        ),
    }
}

fn bind_literal(
    value: &QueryOperand,
    args: &mut Vec<SqlArg>,
    taken: &mut Vec<String>,
    next_literal: &mut usize,
) -> String {
    // Reuse an identical literal within one predicate so the SQL carries one
    // bind per distinct literal (deterministic: first occurrence wins).
    if let Some(existing) = args.iter().find(|arg| &arg.source == value) {
        return existing.name.clone();
    }
    let name = literal_name(taken, next_literal);
    taken.push(name.clone());
    let cast = match value {
        QueryOperand::Literal(literal) => literal_cast(literal),
        QueryOperand::Param(_) => "::text",
    };
    args.push(SqlArg {
        name: name.clone(),
        cast,
        source: value.clone(),
    });
    name
}

// ── SeaORM rendering ────────────────────────────────────────────────────

/// The `.eq(...)` argument for a value of the given rust type.
fn value_arg(param: &str, rust_type: &str) -> String {
    // `ColumnTrait::eq` takes `impl Into<Value>`; sea-orm implements
    // `From<T>` for owned values but not for references, so a `&param`
    // binding only moves for Copy types (primitives, Uuid).
    const COPY: &[&str] = &[
        "bool", "f32", "f64", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "Uuid",
    ];
    if COPY.iter().any(|ty| rust_type.starts_with(ty)) {
        param.to_string()
    } else {
        format!("{param}.clone()")
    }
}

/// Render a literal as a Rust expression for `ColumnTrait` comparisons.
fn seaorm_literal(literal: &QueryLiteral) -> String {
    match literal {
        QueryLiteral::Int(value) => format!("{value}i64"),
        QueryLiteral::Float(value) => format!("{value}f64"),
        QueryLiteral::String(text) => format!("{text:?}.to_string()"),
        QueryLiteral::Bool(value) => value.to_string(),
        // Dates render through the custom-expression arm (an explicit cast).
        QueryLiteral::Date(text) => format!("{text:?}.to_string()"),
    }
}

/// Render the plan as a SeaORM `Condition` builder expression:
/// `sea_orm::Condition::all().add(…)…`.
pub fn render_seaorm(plan: &QueryPlan, params: &[FinderParam], entity_module: &str) -> String {
    format!(
        "sea_orm::Condition::all().add({})",
        render_seaorm_predicate(&plan.predicate, params, entity_module)
    )
}

fn render_seaorm_predicate(
    predicate: &QueryPredicate,
    params: &[FinderParam],
    entity_module: &str,
) -> String {
    let (constructor, children) = match predicate {
        QueryPredicate::Atom(atom) => {
            return render_seaorm_atom(atom, params, entity_module);
        }
        QueryPredicate::And(children) => ("all", children),
        QueryPredicate::Or(children) => ("any", children),
    };
    let mut expr = format!("sea_orm::Condition::{constructor}()");
    for child in children {
        expr.push_str(&format!(
            ".add({})",
            render_seaorm_predicate(child, params, entity_module)
        ));
    }
    expr
}

fn render_seaorm_atom(atom: &QueryAtom, params: &[FinderParam], entity_module: &str) -> String {
    match atom {
        QueryAtom::Null { column, is_null } => format!(
            "crate::entity::{entity_module}::Column::{}.{}()",
            column.variant(),
            if *is_null { "is_null" } else { "is_not_null" }
        ),
        QueryAtom::Bool { column, expected } => format!(
            "crate::entity::{entity_module}::Column::{}.eq({expected})",
            column.variant()
        ),
        QueryAtom::Cmp { column, op, value } => {
            if let QueryOperand::Literal(QueryLiteral::Date(text)) = value {
                // A date literal binds its ISO text with an explicit cast:
                // `NaiveDate` construction needs a fallible parse, so the
                // bound text + `::date` keeps the statement parameterized
                // and the value typed without a panic path.
                return format!(
                    "sea_orm::sea_query::Expr::cust_with_values(\"\\\"{}\\\" {} $1::date\", \
                     vec![sea_orm::Value::from({text:?})])",
                    column.pg_column,
                    op.sql()
                );
            }
            let arg = match value {
                QueryOperand::Param(index) => params
                    .get(*index)
                    .map(|param| value_arg(&param.name, &param.rust_type))
                    .unwrap_or_else(|| format!("param{index}")),
                QueryOperand::Literal(literal) => seaorm_literal(literal),
            };
            format!(
                "crate::entity::{entity_module}::Column::{}.{}({arg})",
                column.variant(),
                op.seaorm()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(feature: &str, rust_type: &str, is_nullable: bool) -> QueryColumn {
        QueryColumn {
            feature: feature.to_string(),
            field: codegraph_naming::to_snake_case(feature),
            pg_column: codegraph_naming::to_snake_case(feature),
            rust_type: rust_type.to_string(),
            is_nullable,
            kind: QueryColumnKind::Scalar,
        }
    }

    fn columns() -> Vec<QueryColumn> {
        vec![
            column("title", "String", false),
            column("synopsis", "String", true),
            column("runtimeMinutes", "i64", false),
            column("rating", "f64", false),
            column("active", "bool", false),
            column("premieredOn", "NaiveDate", false),
            column("genre", "String", false),
        ]
    }

    fn param(name: &str, rust_type: &str) -> FinderParam {
        FinderParam {
            name: name.to_string(),
            rust_type: rust_type.to_string(),
            optional: false,
        }
    }

    fn params() -> Vec<FinderParam> {
        vec![
            param("target_title", "String"),
            param("minimum", "i64"),
            param("threshold", "f64"),
            param("on", "NaiveDate"),
        ]
    }

    fn spec(predicate: &str) -> phero_ir::ddd::QuerySpec {
        phero_ir::ddd::QuerySpec::new(predicate)
    }

    fn lower(predicate: &str) -> QueryPlan {
        lower_query_spec(
            &spec(predicate),
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .unwrap_or_else(|err| panic!("{predicate} must lower: {err}"))
    }

    fn refusal(predicate: &str) -> String {
        lower_query_spec(
            &spec(predicate),
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .expect_err("expected a refusal")
    }

    fn atom(plan: &QueryPlan) -> &QueryAtom {
        match &plan.predicate {
            QueryPredicate::Atom(atom) => atom,
            other => panic!("expected a single atom, got {other:?}"),
        }
    }

    fn cmp(plan: &QueryPlan) -> (&str, CmpOp, &QueryOperand) {
        match atom(plan) {
            QueryAtom::Cmp { column, op, value } => (column.feature.as_str(), *op, value),
            other => panic!("expected a comparison, got {other:?}"),
        }
    }

    // ── comparisons ─────────────────────────────────────────────────────

    #[test]
    fn equality_on_a_parameter_binds_the_parameter() {
        let plan = lower("self.title == target_title");
        assert_eq!(cmp(&plan), ("title", CmpOp::Eq, &QueryOperand::Param(0)));
        assert_eq!(plan.params, vec![0]);
    }

    #[test]
    fn every_comparison_operator_lowers() {
        for (source, op) in [
            ("self.runtimeMinutes == minimum", CmpOp::Eq),
            ("self.runtimeMinutes != minimum", CmpOp::Ne),
            ("self.runtimeMinutes < minimum", CmpOp::Lt),
            ("self.runtimeMinutes <= minimum", CmpOp::Le),
            ("self.runtimeMinutes > minimum", CmpOp::Gt),
            ("self.runtimeMinutes >= minimum", CmpOp::Ge),
        ] {
            let plan = lower(source);
            assert_eq!(cmp(&plan).1, op, "{source}");
        }
    }

    #[test]
    fn a_reversed_comparison_swaps_the_operator() {
        let plan = lower("minimum <= self.runtimeMinutes");
        assert_eq!(cmp(&plan).1, CmpOp::Ge);
        assert_eq!(cmp(&plan).2, &QueryOperand::Param(1));
    }

    #[test]
    fn bare_features_resolve_without_self() {
        let plan = lower("runtimeMinutes >= minimum");
        assert_eq!(cmp(&plan).0, "runtimeMinutes");
    }

    #[test]
    fn an_integer_literal_is_bound_not_interpolated() {
        let plan = lower("self.runtimeMinutes >= 100");
        assert_eq!(cmp(&plan).2, &QueryOperand::Literal(QueryLiteral::Int(100)));
    }

    #[test]
    fn a_negative_literal_folds_into_the_literal() {
        let plan = lower("self.runtimeMinutes >= -5");
        assert_eq!(cmp(&plan).2, &QueryOperand::Literal(QueryLiteral::Int(-5)));
    }

    #[test]
    fn float_string_bool_and_date_literals_lower() {
        assert_eq!(
            cmp(&lower("self.rating > 4.5")).2,
            &QueryOperand::Literal(QueryLiteral::Float(4.5))
        );
        assert_eq!(
            cmp(&lower("self.title == \"Dune\"")).2,
            &QueryOperand::Literal(QueryLiteral::String("Dune".to_string()))
        );
        assert_eq!(
            cmp(&lower("self.active == true")).2,
            &QueryOperand::Literal(QueryLiteral::Bool(true))
        );
        assert_eq!(
            cmp(&lower("self.premieredOn >= date(\"2020-01-02\")")).2,
            &QueryOperand::Literal(QueryLiteral::Date("2020-01-02".to_string()))
        );
    }

    #[test]
    fn a_null_comparison_becomes_is_null() {
        match atom(&lower("self.synopsis == null")) {
            QueryAtom::Null { column, is_null } => {
                assert_eq!(column.feature, "synopsis");
                assert!(is_null);
            }
            other => panic!("expected IS NULL, got {other:?}"),
        }
        match atom(&lower("self.synopsis != null")) {
            QueryAtom::Null { is_null, .. } => assert!(!is_null),
            other => panic!("expected IS NOT NULL, got {other:?}"),
        }
        match atom(&lower("null == self.synopsis")) {
            QueryAtom::Null { is_null, .. } => assert!(is_null),
            other => panic!("expected IS NULL, got {other:?}"),
        }
    }

    #[test]
    fn a_non_nullable_column_compared_to_null_is_refused() {
        let err = refusal("self.title == null");
        assert!(err.contains("compared to `null`"), "{err}");
        assert!(err.contains("`title`"), "{err}");
    }

    #[test]
    fn a_bare_boolean_column_is_a_predicate() {
        match atom(&lower("self.active")) {
            QueryAtom::Bool { column, expected } => {
                assert_eq!(column.feature, "active");
                assert!(expected);
            }
            other => panic!("expected a boolean atom, got {other:?}"),
        }
        match atom(&lower("!self.active")) {
            QueryAtom::Bool { expected, .. } => assert!(!expected),
            other => panic!("expected a negated boolean atom, got {other:?}"),
        }
    }

    #[test]
    fn and_or_nest_in_source_order() {
        let plan = lower("self.title == target_title && self.runtimeMinutes >= minimum");
        match &plan.predicate {
            QueryPredicate::And(children) => {
                assert_eq!(children.len(), 2);
                assert_eq!(
                    children[0].atoms()[0],
                    &atom(&lower("self.title == target_title")).clone()
                );
            }
            other => panic!("expected AND, got {other:?}"),
        }
        let plan = lower("self.title == target_title || self.runtimeMinutes >= minimum");
        assert!(matches!(plan.predicate, QueryPredicate::Or(_)));
    }

    #[test]
    fn not_is_pushed_down_to_the_atoms() {
        let plan = lower("!(self.title == target_title && self.runtimeMinutes >= minimum)");
        match &plan.predicate {
            QueryPredicate::Or(children) => {
                assert_eq!(children.len(), 2);
                match &children[0] {
                    QueryPredicate::Atom(QueryAtom::Cmp { op, .. }) => assert_eq!(*op, CmpOp::Ne),
                    other => panic!("expected a comparison, got {other:?}"),
                }
                match &children[1] {
                    QueryPredicate::Atom(QueryAtom::Cmp { op, .. }) => assert_eq!(*op, CmpOp::Lt),
                    other => panic!("expected a comparison, got {other:?}"),
                }
            }
            other => panic!("expected OR, got {other:?}"),
        }
        match atom(&lower("!(self.synopsis == null)")) {
            QueryAtom::Null { is_null, .. } => assert!(!is_null),
            other => panic!("expected IS NOT NULL, got {other:?}"),
        }
    }

    #[test]
    fn referenced_parameters_are_listed_in_declaration_order() {
        let plan = lower("self.runtimeMinutes >= minimum && self.title == target_title");
        assert_eq!(plan.params, vec![0, 1]);
    }

    // ── refusals ────────────────────────────────────────────────────────

    #[test]
    fn every_refusal_names_the_repository_operation_and_construct() {
        for (source, construct) in [
            ("self.title.startsWith(\"A\")", "an operation call"),
            ("length(self.title) > 2", "a package-function call"),
            ("self.runtimeMinutes + 10 > minimum", "arithmetic"),
            ("self.title == self.synopsis", "between two columns"),
            ("self.genre?.title == target_title", "safe navigation"),
            ("self.synopsis ?: \"\" == target_title", "`?:`"),
            ("if self.active { true } else { false }", "`if`"),
            ("self.title == does_not_exist", "`does_not_exist`"),
            ("self.active > false", "boolean feature"),
        ] {
            let err = refusal(source);
            assert!(
                err.starts_with("query `MovieRepository.recent`:"),
                "refusal must name the repository and operation: {err}"
            );
            assert!(
                err.contains(construct),
                "refusal must name the construct {construct}: {err}"
            );
        }
    }

    #[test]
    fn a_class_reference_is_refused_with_the_id_column_help() {
        let mut cols = columns();
        cols.push(QueryColumn {
            feature: "publisher".to_string(),
            field: "publisher_id".to_string(),
            pg_column: "publisher_id".to_string(),
            rust_type: "Uuid".to_string(),
            is_nullable: false,
            kind: QueryColumnKind::ClassReference,
        });
        let err = lower_query_spec(
            &spec("self.publisher == publisher"),
            "query `MovieRepository.recent`",
            &[],
            &[param("publisher", "Uuid")],
            &cols,
        )
        .expect_err("class references are refused");
        assert!(err.contains("`publisher` is a class reference"), "{err}");
        assert!(err.contains("`publisher_id` column"), "{err}");
    }

    #[test]
    fn a_multiplicity_parameter_is_refused() {
        let mut parameters = params();
        parameters.push(param("genres", "Vec<String>"));
        let err = lower_query_spec(
            &spec("self.genre == genres"),
            "query `MovieRepository.recent`",
            &[],
            &parameters,
            &columns(),
        )
        .expect_err("collection parameters are refused");
        assert!(err.contains("`genres`"), "{err}");
        assert!(err.contains("collection"), "{err}");
    }

    #[test]
    fn a_hand_written_non_read_only_spec_is_refused() {
        let err = lower_query_spec(
            &phero_ir::ddd::QuerySpec::new("self.title == target_title").with_read_only(false),
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .expect_err("read-only is structural");
        assert!(err.contains("not read-only"), "{err}");
    }

    // ── sort + page ─────────────────────────────────────────────────────

    fn sorted_spec() -> phero_ir::ddd::QuerySpec {
        phero_ir::ddd::QuerySpec::new("self.title == target_title").with_sort(vec![
            phero_ir::ddd::QuerySortKey::new(
                "runtimeMinutes",
                phero_ir::TypeRef::Primitive(phero_ir::PrimitiveType::Int),
                phero_ir::ddd::SortDirection::Desc,
            ),
        ])
    }

    #[test]
    fn sort_keys_lower_with_their_direction() {
        let plan = lower_query_spec(
            &sorted_spec(),
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .unwrap();
        assert_eq!(plan.order.len(), 1);
        assert_eq!(plan.order[0].column.feature, "runtimeMinutes");
        assert!(plan.order[0].descending);
    }

    #[test]
    fn an_unknown_sort_key_is_refused() {
        let spec =
            spec("self.title == target_title").with_sort(vec![phero_ir::ddd::QuerySortKey::new(
                "ghost",
                phero_ir::TypeRef::Primitive(phero_ir::PrimitiveType::String),
                phero_ir::ddd::SortDirection::Asc,
            )]);
        let err = lower_query_spec(
            &spec,
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .expect_err("unknown sort keys are refused");
        assert!(err.contains("sort key `ghost`"), "{err}");
    }

    #[test]
    fn pagination_lowers_the_window() {
        let spec = spec("self.title == target_title").with_pagination(Some(
            phero_ir::ddd::QueryPagination::new()
                .with_limit(Some(20))
                .with_max(Some(100)),
        ));
        let plan = lower_query_spec(
            &spec,
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .unwrap();
        let page = plan.page.expect("page");
        assert_eq!(page.limit, Some(20));
        assert_eq!(page.max, Some(100));
        assert!(!page.cursor);
    }

    #[test]
    fn a_cursor_page_still_lowers_to_an_offset_page() {
        let spec = spec("self.title == target_title").with_pagination(Some(
            phero_ir::ddd::QueryPagination::new().with_cursor(true),
        ));
        let plan = lower_query_spec(
            &spec,
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .unwrap();
        assert!(plan.page.expect("page").cursor);
    }

    // ── rendering ───────────────────────────────────────────────────────

    #[test]
    fn sql_renders_a_parameterized_predicate_in_first_occurrence_order() {
        let plan = lower("self.title == target_title && self.runtimeMinutes >= minimum");
        let sql = render_sql(&plan, &params());
        assert_eq!(
            sql.predicate,
            "(\"title\" = :target_title::text AND \"runtime_minutes\" >= :minimum::bigint)"
        );
        assert_eq!(
            sql.args,
            vec![
                SqlArg {
                    name: "target_title".to_string(),
                    cast: "::text",
                    source: QueryOperand::Param(0),
                },
                SqlArg {
                    name: "minimum".to_string(),
                    cast: "::bigint",
                    source: QueryOperand::Param(1),
                },
            ]
        );
        assert_eq!(sql.order_by, "\"id\" ASC");
    }

    #[test]
    fn sql_binds_a_repeated_parameter_once() {
        let plan = lower("self.title == target_title || self.synopsis == target_title");
        let sql = render_sql(&plan, &params());
        assert_eq!(
            sql.predicate,
            "(\"title\" = :target_title::text OR \"synopsis\" = :target_title::text)"
        );
        assert_eq!(sql.args.len(), 1);
    }

    #[test]
    fn sql_binds_literals_under_generated_names() {
        let plan = lower("self.runtimeMinutes >= 100 && self.synopsis == \"Dune\"");
        let sql = render_sql(&plan, &params());
        assert_eq!(
            sql.predicate,
            "(\"runtime_minutes\" >= :_lit0::bigint AND \"synopsis\" = :_lit1::text)"
        );
        assert_eq!(
            sql.args,
            vec![
                SqlArg {
                    name: "_lit0".to_string(),
                    cast: "::bigint",
                    source: QueryOperand::Literal(QueryLiteral::Int(100)),
                },
                SqlArg {
                    name: "_lit1".to_string(),
                    cast: "::text",
                    source: QueryOperand::Literal(QueryLiteral::String("Dune".to_string())),
                },
            ]
        );
    }

    #[test]
    fn sql_orders_by_the_declared_keys_then_identity() {
        let plan = lower_query_spec(
            &sorted_spec(),
            "query `MovieRepository.recent`",
            &[],
            &params(),
            &columns(),
        )
        .unwrap();
        let sql = render_sql(&plan, &params());
        assert_eq!(sql.order_by, "\"runtime_minutes\" DESC, \"id\" ASC");
    }

    #[test]
    fn sql_renders_null_and_boolean_atoms() {
        let sql = render_sql(&lower("self.synopsis == null"), &params());
        assert_eq!(sql.predicate, "\"synopsis\" IS NULL");
        let sql = render_sql(&lower("self.synopsis != null"), &params());
        assert_eq!(sql.predicate, "\"synopsis\" IS NOT NULL");
        let sql = render_sql(&lower("self.active"), &params());
        assert_eq!(sql.predicate, "\"active\" = :_lit0::boolean");
        assert_eq!(
            sql.args[0].source,
            QueryOperand::Literal(QueryLiteral::Bool(true))
        );
    }

    #[test]
    fn sql_renders_a_date_literal_with_an_explicit_cast() {
        let plan = lower("self.premieredOn >= date(\"2020-01-02\")");
        let sql = render_sql(&plan, &params());
        assert_eq!(sql.predicate, "\"premiered_on\" >= :_lit0::date");
        assert_eq!(
            sql.args[0].source,
            QueryOperand::Literal(QueryLiteral::Date("2020-01-02".to_string()))
        );
    }

    #[test]
    fn sql_lowering_is_deterministic() {
        let source = "self.title == target_title && (self.runtimeMinutes >= minimum || \
                      self.synopsis == null)";
        let first = render_sql(&lower(source), &params());
        let second = render_sql(&lower(source), &params());
        assert_eq!(first, second);
    }

    #[test]
    fn seaorm_renders_a_condition_chain_with_parameter_values() {
        let plan = lower("self.title == target_title && self.runtimeMinutes >= minimum");
        let expr = render_seaorm(&plan, &params(), "library_movie");
        assert_eq!(
            expr,
            "sea_orm::Condition::all().add(sea_orm::Condition::all()\
             .add(crate::entity::library_movie::Column::Title.eq(target_title.clone()))\
             .add(crate::entity::library_movie::Column::RuntimeMinutes.gte(minimum)))"
        );
    }

    #[test]
    fn seaorm_renders_null_boolean_and_date_atoms() {
        let expr = render_seaorm(&lower("self.synopsis != null"), &params(), "library_movie");
        assert_eq!(
            expr,
            "sea_orm::Condition::all().add(crate::entity::library_movie::Column::Synopsis.is_not_null())"
        );
        let expr = render_seaorm(&lower("self.active"), &params(), "library_movie");
        assert_eq!(
            expr,
            "sea_orm::Condition::all().add(crate::entity::library_movie::Column::Active.eq(true))"
        );
        let expr = render_seaorm(
            &lower("self.premieredOn >= date(\"2020-01-02\")"),
            &params(),
            "library_movie",
        );
        assert_eq!(
            expr,
            "sea_orm::Condition::all().add(sea_orm::sea_query::Expr::cust_with_values(\
             \"\\\"premiered_on\\\" >= $1::date\", vec![sea_orm::Value::from(\"2020-01-02\")]))"
        );
    }

    #[test]
    fn seaorm_renders_or_as_a_nested_any_condition() {
        let plan = lower("self.title == target_title || self.runtimeMinutes >= minimum");
        let expr = render_seaorm(&plan, &params(), "library_movie");
        assert_eq!(
            expr,
            "sea_orm::Condition::all().add(sea_orm::Condition::any()\
             .add(crate::entity::library_movie::Column::Title.eq(target_title.clone()))\
             .add(crate::entity::library_movie::Column::RuntimeMinutes.gte(minimum)))"
        );
    }
}
