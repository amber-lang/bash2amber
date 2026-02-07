use super::{BlockFragment, FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct FunctionFragment {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub body: BlockFragment,
}

impl FragmentRenderable for FunctionFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let pad = meta.pad();
        let params = self.params.join(", ");
        let mut lines = Vec::new();

        if let Some(return_type) = self.return_type {
            lines.push(format!(
                "{pad}fun {}({params}): {return_type} {{",
                self.name
            ));
        } else {
            lines.push(format!("{pad}fun {}({params}) {{", self.name));
        }

        let rendered_body = self.body.to_string(meta);
        if !rendered_body.is_empty() {
            lines.push(rendered_body);
        }
        lines.push(format!("{pad}}}"));
        lines.join("\n")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::Function(self)
    }
}
