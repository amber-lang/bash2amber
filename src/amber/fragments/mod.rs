mod block;
mod for_loop;
mod function;
mod if_chain;
mod if_stmt;
mod interpolable;
mod list;
mod raw;
mod var_expr;
mod var_stmt;
mod while_loop;

pub use block::BlockFragment;
pub use for_loop::ForFragment;
pub use function::FunctionFragment;
pub use if_chain::{IfChainBranch, IfChainFragment};
pub use if_stmt::IfFragment;
pub use interpolable::InterpolableFragment;
pub use list::ListFragment;
pub use raw::RawFragment;
pub use var_expr::VarExprFragment;
pub use var_stmt::VarStmtFragment;
pub use while_loop::WhileFragment;

#[derive(Debug, Clone, Default)]
pub struct TranslateMetadata {
    indent: usize,
}

impl TranslateMetadata {
    pub(crate) fn with_offset(&self, offset: usize) -> Self {
        Self {
            indent: self.indent + offset,
        }
    }

    pub(crate) fn is_root(&self) -> bool {
        self.indent == 0
    }

    pub(crate) fn increase_indent(&mut self) {
        self.indent += 1;
    }

    pub(crate) fn decrease_indent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    pub(crate) fn gen_indent(&self) -> String {
        self.pad()
    }

    pub(crate) fn pad(&self) -> String {
        "    ".repeat(self.indent)
    }
}

pub trait FragmentRenderable: Sized {
    fn to_string(self, meta: &mut TranslateMetadata) -> String;
    fn to_frag(self) -> FragmentKind;
}

#[derive(Debug, Clone, Default)]
pub struct Fragments {
    pub fragment: FragmentKind,
}

impl FragmentRenderable for Fragments {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        self.fragment.to_string(meta)
    }

    fn to_frag(self) -> FragmentKind {
        self.fragment
    }
}

#[derive(Debug, Clone)]
pub enum FragmentKind {
    Block(BlockFragment),
    Raw(RawFragment),
    If(IfFragment),
    While(WhileFragment),
    For(ForFragment),
    Function(FunctionFragment),
    IfChain(IfChainFragment),
    VarStmt(VarStmtFragment),
    List(ListFragment),
    VarExpr(VarExprFragment),
    Interpolable(InterpolableFragment),
}

impl Default for FragmentKind {
    fn default() -> Self {
        BlockFragment::new(Vec::new(), false).to_frag()
    }
}

impl FragmentKind {
    pub fn raw(value: String) -> Self {
        RawFragment { value }.to_frag()
    }

    pub fn block(statements: Vec<FragmentKind>) -> Self {
        BlockFragment::new(statements, false).to_frag()
    }
}

impl FragmentRenderable for FragmentKind {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        match self {
            Self::Block(fragment) => fragment.to_string(meta),
            Self::Raw(fragment) => fragment.to_string(meta),
            Self::If(fragment) => fragment.to_string(meta),
            Self::While(fragment) => fragment.to_string(meta),
            Self::For(fragment) => fragment.to_string(meta),
            Self::Function(fragment) => fragment.to_string(meta),
            Self::IfChain(fragment) => fragment.to_string(meta),
            Self::VarStmt(fragment) => fragment.to_string(meta),
            Self::List(fragment) => fragment.to_string(meta),
            Self::VarExpr(fragment) => fragment.to_string(meta),
            Self::Interpolable(fragment) => fragment.to_string(meta),
        }
    }

    fn to_frag(self) -> FragmentKind {
        self
    }
}
