use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct VarExprFragment {
    pub name: String,
}

impl FragmentRenderable for VarExprFragment {
    fn to_string(self, _meta: &mut TranslateMetadata) -> String {
        self.name
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::VarExpr(self)
    }
}
