#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub statements: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Simple(SimpleCommand),
    Connection(Connection),
    If(IfCommand),
    While(WhileCommand),
    For(ForCommand),
    CStyleFor(CStyleForCommand),
    Case(CaseCommand),
    Function(FunctionDef),
    Group(Vec<Command>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    pub words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub left: Box<Command>,
    pub op: Connector,
    pub right: Box<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connector {
    Pipe,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfCommand {
    pub condition: Box<Command>,
    pub then_body: Vec<Command>,
    pub else_body: Option<Vec<Command>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhileCommand {
    pub condition: Box<Command>,
    pub body: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForCommand {
    pub variable: String,
    pub items: Vec<String>,
    pub body: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CStyleForCommand {
    pub init: String,
    pub condition: String,
    pub update: String,
    pub body: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseCommand {
    pub word: String,
    pub clauses: Vec<CaseClause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseClause {
    pub patterns: Vec<String>,
    pub body: Vec<Command>,
    pub terminator: CaseClauseTerminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseClauseTerminator {
    Break,
    Fallthrough,
    TestNext,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub name: String,
    pub body: Vec<Command>,
}
