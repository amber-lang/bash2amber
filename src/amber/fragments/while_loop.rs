use super::{BlockFragment, FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct WhileFragment {
    pub condition: Box<FragmentKind>,
    pub body: BlockFragment,
}

impl FragmentRenderable for WhileFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let pad = meta.pad();
        let mut lines = Vec::new();
        let mut inline_meta = TranslateMetadata::default();
        lines.push(format!(
            "{pad}while {} {{",
            (*self.condition).to_string(&mut inline_meta)
        ));
        let rendered_body = self.body.to_string(meta);
        if !rendered_body.is_empty() {
            lines.push(rendered_body);
        }
        lines.push(format!("{pad}}}"));
        lines.join("\n")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::While(self)
    }
}
