use super::{BlockFragment, FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct ForFragment {
    pub variable: String,
    pub items: String,
    pub body: BlockFragment,
}

impl FragmentRenderable for ForFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let pad = meta.pad();
        let mut lines = Vec::new();
        lines.push(format!("{pad}for {} in {} {{", self.variable, self.items));
        let rendered_body = self.body.to_string(meta);
        if !rendered_body.is_empty() {
            lines.push(rendered_body);
        }
        lines.push(format!("{pad}}}"));
        lines.join("\n")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::For(self)
    }
}
