use crate::amber::render::context::RenderContext;
use crate::bash::ast::SimpleCommand;

pub mod cd;
pub mod clear;
pub mod cp;
pub mod exit;
pub mod len;
pub mod lines;
pub mod ls;
pub mod mv;
pub mod pwd;
pub mod rm;
pub mod sleep;
pub mod touch;

pub(crate) fn render_builtin(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let name = simple.words.first()?;
    match name.as_str() {
        "cd" => cd::render(simple, ctx),
        "clear" => clear::render(simple, ctx),
        "cp" => cp::render(simple, ctx),
        "exit" => exit::render(simple, ctx),
        "ls" => ls::render(simple, ctx),
        "mv" => mv::render(simple, ctx),
        "pwd" => pwd::render(simple, ctx),
        "rm" => rm::render(simple, ctx),
        "sleep" => sleep::render(simple, ctx),
        "touch" => touch::render(simple, ctx),
        "lines" => lines::render(simple, ctx),
        _ => None,
    }
}

pub(crate) fn render_builtin_expr(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let name = simple.words.first()?;
    match name.as_str() {
        "pwd" => pwd::render(simple, ctx),
        "ls" => ls::render(simple, ctx),
        "lines" => lines::render(simple, ctx),
        "len" => len::render(simple, ctx),
        _ => None,
    }
}
