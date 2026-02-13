use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let mut code: Option<String> = None;

    for arg in simple.words.iter().skip(1) {
        if arg.starts_with('-') {
            return None;
        }
        if code.is_some() {
            return None;
        }
        code = Some(word_to_expr(arg, ctx)?);
    }

    let result = match code {
        Some(c) => format!("exit({c})"),
        None => "exit()".to_string(),
    };

    Some(result)
}
