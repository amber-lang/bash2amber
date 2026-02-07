use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct RawFragment {
    pub value: String,
}

impl FragmentRenderable for RawFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        format!("{}{}", meta.gen_indent(), self.value)
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::Raw(self)
    }
}
