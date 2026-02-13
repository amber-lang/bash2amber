use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    if simple.words.len() != 2 {
        return None;
    }

    let seconds_arg = &simple.words[1];
    if seconds_arg.starts_with('-') {
        return None;
    }

    let seconds = word_to_expr(seconds_arg, ctx)?;
    Some(format!("sleep({seconds})"))
}
