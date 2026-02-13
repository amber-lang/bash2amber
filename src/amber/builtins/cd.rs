use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    if simple.words.len() < 2 {
        return None;
    }

    let args: Vec<&String> = simple.words.iter().skip(1).collect();

    for arg in &args {
        if arg.starts_with('-') {
            return None;
        }
    }

    let path = word_to_expr(args[0], ctx)?;
    Some(format!("cd({path})"))
}
