use super::{BlockFragment, FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone)]
pub struct IfChainFragment {
    pub branches: Vec<IfChainBranch>,
    pub else_body: Option<BlockFragment>,
}

impl FragmentRenderable for IfChainFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let pad = meta.pad();
        let mut lines = Vec::new();
        lines.push(format!("{pad}if {{"));

        for branch in self.branches {
            let mut inline_meta = TranslateMetadata::default();
            let condition = (*branch.condition).to_string(&mut inline_meta);
            lines.push(format!("{pad}    {condition} {{"));
            let mut branch_meta = meta.with_offset(1);
            let rendered_body = branch.body.to_string(&mut branch_meta);
            if !rendered_body.is_empty() {
                lines.push(rendered_body);
            }
            lines.push(format!("{pad}    }}"));
        }

        if let Some(else_body) = self.else_body {
            lines.push(format!("{pad}    else {{"));
            let mut else_meta = meta.with_offset(1);
            let rendered_else = else_body.to_string(&mut else_meta);
            if !rendered_else.is_empty() {
                lines.push(rendered_else);
            }
            lines.push(format!("{pad}    }}"));
        }

        lines.push(format!("{pad}}}"));
        lines.join("\n")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::IfChain(self)
    }
}

#[derive(Debug, Clone)]
pub struct IfChainBranch {
    pub condition: Box<FragmentKind>,
    pub body: BlockFragment,
}
