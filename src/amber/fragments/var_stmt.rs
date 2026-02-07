use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct VarStmtFragment {
    pub name: String,
    pub value: Box<FragmentKind>,
    pub is_reassignment: bool,
}

impl FragmentRenderable for VarStmtFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let mut inline_meta = TranslateMetadata::default();
        let value = (*self.value).to_string(&mut inline_meta);
        let lhs = if self.is_reassignment {
            self.name
        } else {
            format!("let {}", self.name)
        };
        format!("{}{} = {}", meta.gen_indent(), lhs, value)
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::VarStmt(self)
    }
}
