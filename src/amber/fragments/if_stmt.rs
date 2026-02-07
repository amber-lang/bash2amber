use super::{BlockFragment, FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct IfFragment {
    pub condition: Box<FragmentKind>,
    pub then_body: BlockFragment,
    pub else_body: Option<BlockFragment>,
}

impl FragmentRenderable for IfFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let IfFragment {
            condition,
            then_body,
            else_body,
        } = self;

        let pad = meta.pad();
        let mut lines = Vec::new();
        let mut inline_meta = TranslateMetadata::default();
        lines.push(format!(
            "{pad}if {} {{",
            (*condition).to_string(&mut inline_meta)
        ));

        let rendered_then = then_body.to_string(meta);
        if !rendered_then.is_empty() {
            lines.push(rendered_then);
        }

        if let Some(else_body) = else_body {
            lines.push(format!("{pad}}} else {{"));
            let rendered_else = else_body.to_string(meta);
            if !rendered_else.is_empty() {
                lines.push(rendered_else);
            }
        }

        lines.push(format!("{pad}}}"));
        lines.join("\n")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::If(self)
    }
}
