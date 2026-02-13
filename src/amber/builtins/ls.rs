use crate::amber::render::context::RenderContext;
use crate::amber::render::word::word_to_expr;
use crate::bash::ast::SimpleCommand;

pub(crate) fn render(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let mut all = false;
    let mut recursive = false;
    let mut path: Option<&String> = None;

    for arg in simple.words.iter().skip(1) {
        if arg == "-a" || arg == "-A" || arg == "--all" {
            all = true;
        } else if arg == "-R" || arg == "-r" || arg == "--recursive" {
            recursive = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            for ch in arg[1..].chars() {
                match ch {
                    'a' | 'A' => all = true,
                    'R' | 'r' => recursive = true,
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

    let path_expr = match path {
        Some(p) => Some(word_to_expr(p, ctx)?),
        None => None,
    };

    let mut args = Vec::new();
    if let Some(p) = path_expr {
        args.push(p);
    }
    if all {
        args.push("true".to_string());
    }
    if recursive {
        args.push("true".to_string());
    }

    let result = if args.is_empty() {
        "trust ls()".to_string()
    } else {
        format!("trust ls({})", args.join(", "))
    };

    Some(result)
}
