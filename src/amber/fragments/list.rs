use super::{FragmentKind, FragmentRenderable, TranslateMetadata};

#[derive(Debug, Clone, Default)]
pub struct ListFragment {
    pub items: Vec<FragmentKind>,
}

impl FragmentRenderable for ListFragment {
    fn to_string(self, _meta: &mut TranslateMetadata) -> String {
        let mut inline_meta = TranslateMetadata::default();
        self.items
            .into_iter()
            .map(|item| item.to_string(&mut inline_meta))
            .collect::<Vec<String>>()
            .join(" ")
    }

    fn to_frag(self) -> FragmentKind {
        FragmentKind::List(self)
    }
}
