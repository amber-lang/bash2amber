use crate::amber::render::context::RenderContext;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, _ctx: &RenderContext) -> Option<String> {
    if simple.words.len() > 1 {
        return None;
    }
    Some("clear()".to_string())
}
