use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone, Default)]
pub struct InterpolableFragment {
    pub strings: Vec<String>,
    pub interpolations: Vec<String>,
}

impl FragmentRenderable for InterpolableFragment {
    fn to_string(self, _meta: &mut TranslateMetadata) -> String {
        if self.strings.is_empty() {
            return "\"\"".to_string();
        }

        if self.interpolations.is_empty() && self.strings.len() == 1 {
            return self.strings[0].clone();
        }

        let mut output = String::new();
        for (index, segment) in self.strings.into_iter().enumerate() {
            output.push_str(&segment);
            if let Some(interpolation) = self.interpolations.get(index) {
                output.push('{');
                output.push_str(interpolation);
                output.push('}');
            }
        }
        output
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::Interpolable(self)
    }
}
