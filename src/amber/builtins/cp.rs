use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let mut force = false;
    let mut positional_args: Vec<&String> = Vec::new();

    for arg in simple.words.iter().skip(1) {
        if arg == "-f" || arg == "--force" {
            force = true;
        } else if arg.starts_with('-') {
            return None;
        } else {
            positional_args.push(arg);
        }
    }

    if positional_args.len() != 2 {
        return None;
    }

    let source = word_to_expr(positional_args[0], ctx)?;
    let dest = word_to_expr(positional_args[1], ctx)?;

    let result = if force {
        format!("trust cp({source}, {dest}, true)")
    } else {
        format!("trust cp({source}, {dest})")
    };

    Some(result)
}
