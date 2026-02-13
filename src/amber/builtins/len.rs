use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    if simple.words.len() != 2 {
        return None;
    }

    for arg in simple.words.iter().skip(1) {
        if arg.starts_with('-') {
            return None;
        }
    }

    let expr = word_to_expr(&simple.words[1], ctx)?;
    Some(format!("len({expr})"))
}
