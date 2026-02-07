use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone, Default)]
pub struct BlockFragment {
    pub statements: Vec<FragmentKind>,
    pub increase_indent: bool,
}

impl BlockFragment {
    pub fn new(statements: Vec<FragmentKind>, increase_indent: bool) -> Self {
        Self {
            statements,
            increase_indent,
        }
    }

    pub fn append(&mut self, statement: FragmentKind) {
        self.statements.push(statement);
    }
}

impl FragmentRenderable for BlockFragment {
    fn to_string(self, meta: &mut TranslateMetadata) -> String {
        let root_scope = meta.is_root();
        if self.increase_indent {
            meta.increase_indent();
        }

        let mut result = Vec::new();
        for statement in self.statements {
            match statement {
                FragmentKind::Block(block) => {
                    let rendered = block.to_string(meta);
                    if !rendered.is_empty() {
                        result.push(rendered);
                    }
                }
                _ => {
                    let rendered = statement.to_string(meta);
                    if !rendered.is_empty() {
                        result.push(rendered);
                    }
                }
            }
        }

        if self.increase_indent {
            meta.decrease_indent();
        }

        let mut output = result.join("\n");
        if root_scope && !self.increase_indent && !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::Block(self)
    }
}
