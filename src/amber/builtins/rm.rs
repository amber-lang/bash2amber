use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let mut recursive = false;
    let mut force = false;
    let mut path: Option<&String> = None;

    for arg in simple.words.iter().skip(1) {
        if arg == "-r" || arg == "-R" || arg == "--recursive" {
            recursive = true;
        } else if arg == "-f" || arg == "--force" {
            force = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'r' | 'R' => recursive = true,
                    'f' => force = true,
                    _ => return None,
                }
            }
        } else {
            if path.is_some() {
                return None;
            }
            path = Some(arg);
        }
    }

    let path = path?;
    let path_expr = word_to_expr(path, ctx)?;

    let mut args = vec![path_expr];
    if recursive {
        args.push("true".to_string());
    }
    if force {
        args.push("true".to_string());
    }

    Some(format!("trust rm({})", args.join(", ")))
}
