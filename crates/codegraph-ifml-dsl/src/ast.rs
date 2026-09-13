use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IfmlDefinition {
    Domain(DomainDeclaration),
    View(ViewDeclaration),
    Action(ActionDeclaration),
    Module(ModuleDeclaration),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainDeclaration {
    pub name: String,
    pub schema_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewDeclaration {
    pub name: String,
    pub label: Option<String>,
    pub is_landmark: bool,
    pub is_xor: bool,
    pub is_modal: bool,
    pub params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub containers: Vec<ContainerDeclaration>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
    pub condition: Option<Expression>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerDeclaration {
    pub name: String,
    pub is_default: bool,
    pub params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
    pub condition: Option<Expression>,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentType {
    List,
    Form,
    Details,
    Search,
    Tree,
    Chart,
    Table,
    Button,
    Link,
    Menu,
    Image,
    Embedded,
    Custom(String),
}

impl ComponentType {
    pub fn as_str(&self) -> &str {
        match self {
            ComponentType::List => "list",
            ComponentType::Form => "form",
            ComponentType::Details => "details",
            ComponentType::Search => "search",
            ComponentType::Tree => "tree",
            ComponentType::Chart => "chart",
            ComponentType::Table => "table",
            ComponentType::Button => "button",
            ComponentType::Link => "link",
            ComponentType::Menu => "menu",
            ComponentType::Image => "image",
            ComponentType::Embedded => "embedded",
            ComponentType::Custom(s) => s.as_str(),
        }
    }
}

impl From<&str> for ComponentType {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "list" => ComponentType::List,
            "form" => ComponentType::Form,
            "details" => ComponentType::Details,
            "search" => ComponentType::Search,
            "tree" => ComponentType::Tree,
            "chart" => ComponentType::Chart,
            "table" => ComponentType::Table,
            "button" => ComponentType::Button,
            "link" => ComponentType::Link,
            "menu" => ComponentType::Menu,
            "image" => ComponentType::Image,
            "embedded" => ComponentType::Embedded,
            _ => ComponentType::Custom(s.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDeclaration {
    pub name: String,
    pub component_type: Option<ComponentType>,
    pub spec: Option<ComponentSpec>,
    pub properties: Vec<PropertyAssignment>,
    pub events: Vec<EventHandler>,
    pub condition: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentSpec {
    Table(TableSpec),
    Form(FormSpec),
    Chart(ChartSpec),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSpec {
    pub columns: Vec<ColumnDef>,
    pub pagination: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnDef {
    Field {
        label: String,
        field: PropertyRef,
    },
    Lookup {
        label: String,
        field: PropertyRef,
        lookup: String,
    },
    Expression {
        label: String,
        expr: Expression,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyRef {
    pub entity: String,
    pub property: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormSpec {
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub input: InputFieldType,
    pub required: bool,
    pub validations: Vec<Expression>,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputFieldType {
    Text,
    TextArea,
    Password,
    Email,
    Number,
    Date,
    Time,
    DateTime,
    Dropdown,
    RadioGroup,
    Checkbox,
    Toggle,
    File,
    Hidden,
    Custom(String),
}

impl From<&str> for InputFieldType {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "text" => InputFieldType::Text,
            "textarea" => InputFieldType::TextArea,
            "password" => InputFieldType::Password,
            "email" => InputFieldType::Email,
            "number" => InputFieldType::Number,
            "date" => InputFieldType::Date,
            "time" => InputFieldType::Time,
            "datetime" => InputFieldType::DateTime,
            "dropdown" => InputFieldType::Dropdown,
            "radio" => InputFieldType::RadioGroup,
            "checkbox" => InputFieldType::Checkbox,
            "toggle" => InputFieldType::Toggle,
            "file" => InputFieldType::File,
            "hidden" => InputFieldType::Hidden,
            _ => InputFieldType::Custom(s.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub label_field: Option<String>,
    pub value_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
    Radar,
    Metric,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyAssignment {
    pub key: String,
    pub value: ValueExpression,
    #[serde(skip, default)]
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectMember {
    pub key: String,
    pub value: ValueExpression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValueExpression {
    Identifier(String),
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<ValueExpression>),
    Object(Vec<ObjectMember>),
    Call(String, Vec<ValueExpression>),
    FieldAccess {
        object: Box<ValueExpression>,
        field: String,
    },
    BinOp {
        left: Box<ValueExpression>,
        op: BinOp,
        right: Box<ValueExpression>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<ValueExpression>,
    },
    Group(Box<ValueExpression>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventHandler {
    pub event_type: EventType,
    pub params: Vec<String>,
    pub condition: Option<Expression>,
    pub action: EventAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    Select,
    Submit,
    Click,
    Change,
    Load,
    Save,
    Cancel,
    Delete,
    Confirm,
    Back,
    Custom(String),
}

#[allow(clippy::inherent_to_string)]
impl EventType {
    pub fn as_str(&self) -> &str {
        match self {
            EventType::Select => "select",
            EventType::Submit => "submit",
            EventType::Click => "click",
            EventType::Change => "change",
            EventType::Load => "load",
            EventType::Save => "save",
            EventType::Cancel => "cancel",
            EventType::Delete => "delete",
            EventType::Confirm => "confirm",
            EventType::Back => "back",
            EventType::Custom(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventAction {
    Navigate {
        target: String,
        binding: Option<ParameterBinding>,
    },
    Refresh {
        target: String,
        binding: Option<ParameterBinding>,
    },
    ActionInvocation {
        name: String,
        body: Option<ActionBody>,
    },
    Stay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionBody {
    pub properties: Vec<PropertyAssignment>,
    pub handlers: Vec<EventHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterDecl {
    pub name: String,
    pub type_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterBinding {
    pub pairs: Vec<(String, Expression)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    Ident(String),
    StringLit(String),
    NumLit(f64),
    BoolLit(bool),
    FieldExpr {
        object: Box<Expression>,
        field: String,
    },
    BinOp {
        left: Box<Expression>,
        op: BinOp,
        right: Box<Expression>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expression>,
    },
    Group(Box<Expression>),
    Call {
        name: String,
        args: Vec<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    RegexMatch,
    NegRegex,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
}

impl BinOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::RegexMatch => "~=",
            BinOp::NegRegex => "!~",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl UnaryOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        }
    }
}

/// Render an expression back to its DSL source form, deterministically:
/// binary operators are surrounded by single spaces, unary prefixes bind
/// directly, and explicit groups keep their parentheses.
pub fn render_expression(expr: &Expression) -> String {
    match expr {
        Expression::Ident(s) => s.clone(),
        Expression::StringLit(s) => format!("\"{s}\""),
        Expression::NumLit(n) => n.to_string(),
        Expression::BoolLit(b) => b.to_string(),
        Expression::FieldExpr { object, field } => {
            format!("{}.{}", render_expression(object), field)
        }
        Expression::BinOp { left, op, right } => format!(
            "{} {} {}",
            render_expression(left),
            op.as_str(),
            render_expression(right)
        ),
        Expression::UnaryOp { op, operand } => {
            format!("{}{}", op.as_str(), render_expression(operand))
        }
        Expression::Group(inner) => format!("({})", render_expression(inner)),
        Expression::Call { name, args } => {
            let args_str: Vec<String> = args.iter().map(render_expression).collect();
            format!("{}({})", name, args_str.join(", "))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDeclaration {
    pub name: String,
    pub properties: Vec<PropertyAssignment>,
    pub events: Vec<EventHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDeclaration {
    pub name: String,
    pub input_params: Vec<ParameterDecl>,
    pub output_params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub containers: Vec<ContainerDeclaration>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfmlModel {
    pub domains: Vec<DomainDeclaration>,
    pub views: Vec<ViewDeclaration>,
    pub actions: Vec<ActionDeclaration>,
    pub modules: Vec<ModuleDeclaration>,
}

#[derive(Debug, Error)]
pub enum IfmlParseError {
    #[error("Parse error at {position}: {message}")]
    Parse { position: String, message: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(object: &str, name: &str) -> Expression {
        Expression::FieldExpr {
            object: Box::new(Expression::Ident(object.to_string())),
            field: name.to_string(),
        }
    }

    fn bin_op(left: Expression, op: BinOp, right: Expression) -> Expression {
        Expression::BinOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }

    #[test]
    fn test_render_literals() {
        assert_eq!(render_expression(&Expression::Ident("x".to_string())), "x");
        assert_eq!(
            render_expression(&Expression::StringLit("admin".to_string())),
            "\"admin\""
        );
        assert_eq!(render_expression(&Expression::NumLit(42.0)), "42");
        assert_eq!(render_expression(&Expression::BoolLit(true)), "true");
    }

    #[test]
    fn test_render_field_call_and_unary() {
        assert_eq!(render_expression(&field("row", "active")), "row.active");
        assert_eq!(
            render_expression(&Expression::Call {
                name: "today".to_string(),
                args: vec![field("a", "b")],
            }),
            "today(a.b)"
        );
        assert_eq!(
            render_expression(&Expression::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(field("user", "locked")),
            }),
            "!user.locked"
        );
        assert_eq!(
            render_expression(&Expression::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(Expression::NumLit(1.5)),
            }),
            "-1.5"
        );
    }

    #[test]
    fn test_render_bin_ops_and_grouping() {
        let expr = bin_op(
            bin_op(
                field("user", "role"),
                BinOp::Eq,
                Expression::StringLit("admin".to_string()),
            ),
            BinOp::And,
            bin_op(
                field("account", "balance"),
                BinOp::Ge,
                Expression::NumLit(100.0),
            ),
        );
        assert_eq!(
            render_expression(&expr),
            "user.role == \"admin\" && account.balance >= 100"
        );

        let grouped = Expression::Group(Box::new(bin_op(
            Expression::Ident("a".to_string()),
            BinOp::Or,
            Expression::Ident("b".to_string()),
        )));
        assert_eq!(render_expression(&grouped), "(a || b)");

        let regex = bin_op(
            field("row", "name"),
            BinOp::NegRegex,
            Expression::StringLit("^A".to_string()),
        );
        assert_eq!(render_expression(&regex), "row.name !~ \"^A\"");
    }

    #[test]
    fn test_bin_op_and_unary_op_as_str() {
        assert_eq!(BinOp::Eq.as_str(), "==");
        assert_eq!(BinOp::RegexMatch.as_str(), "~=");
        assert_eq!(BinOp::Mod.as_str(), "%");
        assert_eq!(UnaryOp::Not.as_str(), "!");
        assert_eq!(UnaryOp::Neg.as_str(), "-");
    }
}
