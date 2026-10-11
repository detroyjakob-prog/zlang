//! Abstract Syntax Tree definitions for the Z language.

use crate::value::Value;

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub is_async: bool,
    pub is_static: bool,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl {
        name: String,
        type_ann: Option<String>,
        value: Expr,
    },
    MultiVarDecl {
        names: Vec<String>,
        values: Vec<Expr>,
    },
    ConstDecl {
        name: String,
        value: Expr,
    },
    Assign {
        target: Target,
        op: AssignOp,
        value: Expr,
    },
    MultiAssign {
        targets: Vec<Target>,
        values: Vec<Expr>,
    },
    FnDecl(FunctionDecl),
    ClassDecl(ClassDecl),
    EnumDecl {
        name: String,
        variants: Vec<(String, Option<Expr>)>,
    },
    If {
        cond: Expr,
        then_block: Vec<Stmt>,
        elif_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
    },
    Match {
        expr: Expr,
        cases: Vec<MatchCase>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        targets: Vec<String>,
        iter: Expr,
        body: Vec<Stmt>,
    },
    Try {
        body: Vec<Stmt>,
        excepts: Vec<ExceptHandler>,
        else_block: Option<Vec<Stmt>>,
        finally_block: Option<Vec<Stmt>>,
    },
    With {
        expr: Expr,
        alias: Option<String>,
        body: Vec<Stmt>,
    },
    Import {
        module: String,
        alias: Option<String>,
    },
    FromImport {
        module: String,
        names: Vec<(String, Option<String>)>,
    },
    DirImport {
        path: String,
        names: Vec<(String, Option<String>)>,
    },
    Return(Option<Expr>),
    Yield(Expr),
    CallError(Option<Expr>),
    Assert(Expr, Option<Expr>),
    Del(String),
    Global(Vec<String>),
    Outer(Vec<String>),
    Break,
    Continue,
    Pass,
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct MatchCase {
    pub patterns: Vec<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct ExceptHandler {
    pub exc_types: Vec<String>,
    pub alias: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Field { name: String, value: Value },
    Method {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        is_static: bool,
        return_type: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<String>,
    pub default: Option<Expr>,
    pub is_vararg: bool,
    pub is_kwarg: bool,
}

#[derive(Debug, Clone)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
}

#[derive(Debug, Clone)]
pub enum Target {
    Name(String),
    Attribute(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Deref(String),
}

#[derive(Debug, Clone)]
pub enum Expr {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Await {
        expr: Box<Expr>,
    },
    Attribute {
        obj: Box<Expr>,
        attr: String,
    },
    Index {
        obj: Box<Expr>,
        slice: Slice,
    },
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Tuple(Vec<Expr>),
    Set(Vec<Expr>),
    ListComp {
        expr: Box<Expr>,
        target: String,
        iter: Box<Expr>,
        cond: Option<Box<Expr>>,
    },
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        target: String,
        iter: Box<Expr>,
        cond: Option<Box<Expr>>,
    },
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    /// Converts a value to its `str` representation — produced by f-strings.
    ToStr(Box<Expr>),
    /// The function part of a `call` / `callclass` expression.
    CallTarget {
        target: Box<Expr>,
        is_class: bool,
    },
    /// Shorthand for `call f(...)` / `callclass C.m(...)`.
    CallExpr {
        func: Box<Expr>,
        args: Vec<Arg>,
    },
    CallClassExpr {
        func: Box<Expr>,
        args: Vec<Arg>,
    },
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: Option<String>,
    pub value: Expr,
    pub is_star: bool,
    pub is_double_star: bool,
}

#[derive(Debug, Clone)]
pub enum Slice {
    Full(Option<Box<Expr>>, Option<Box<Expr>>, Option<Box<Expr>>),
    Simple(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    In,
    NotIn,
    Is,
    IsNot,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Pos,
    Not,
    BitNot,
}