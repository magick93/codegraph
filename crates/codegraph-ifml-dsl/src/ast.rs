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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerDeclaration {
    pub name: String,
    pub is_default: bool,
    pub params: Vec<ParameterDecl>,
    pub properties: Vec<PropertyAssignment>,
    pub components: Vec<ComponentDeclaration>,
    pub events: Vec<EventHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDeclaration {
    pub name: String,
    pub properties: Vec<PropertyAssignment>,
    pub events: Vec<EventHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyAssignment {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
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
    Parse {
        position: String,
        message: String,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
