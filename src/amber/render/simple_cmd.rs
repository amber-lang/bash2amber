use crate::bash::ast::*;

use crate::amber::fragments::{FragmentKind, FragmentRenderable, RawFragment, VarStmtFragment};

use super::context::{FunctionRenderMode, RenderContext, TypeCommentReturnContract};
use super::fallback::command_literal_from_command;
use super::fragment_expr::render_expression_fragment;
use super::syntax::{is_identifier, is_number};
use super::word::{parse_assignment, render_simple_function_call_expr, word_to_expr};

pub(super) fn render_simple(simple: &SimpleCommand, ctx: &mut RenderContext) -> Option<FragmentKind> {
    if let Some(assignment) = parse_assignment(simple, ctx) {
        // Skip redundant self-assignments like `let first = first` that arise when
        // a fundoc parameter name matches the positional binding variable.
        if !assignment.is_reassignment && assignment.name == assignment.value {
            return None;
        }
        return Some(
            VarStmtFragment {
                name: assignment.name,
                value: Box::new(render_expression_fragment(&assignment.value)),
                is_reassignment: assignment.is_reassignment,
            }
            .to_frag(),
        );
    }

    if let Some(printf_v) = render_printf_v(simple, ctx) {
        return Some(RawFragment { value: printf_v }.to_frag());
    }

    if let Some(call) = render_function_call_statement(simple, ctx) {
        return Some(RawFragment { value: call }.to_frag());
    }

    if simple.words.first().is_some_and(|word| word == "echo") {
        if simple.words.len() == 1 {
            return Some(
                RawFragment {
                    value: "echo(\"\")".to_string(),
                }
                .to_frag(),
            );
        }

        if simple.words.len() == 2
            && let Some(expr) = word_to_expr(&simple.words[1], ctx)
        {
            return Some(
                RawFragment {
                    value: format!("echo({expr})"),
                }
                .to_frag(),
            );
        }

        if simple.words.len() > 2 {
            let merged = simple.words[1..].join(" ");
            let merged_trimmed = merged.trim();
            if merged_trimmed.starts_with("$((")
                && merged_trimmed.ends_with("))")
                && let Some(expr) = word_to_expr(merged_trimmed, ctx)
            {
                return Some(
                    RawFragment {
                        value: format!("echo({expr})"),
                    }
                    .to_frag(),
                );
            }
        }
    }

    Some(
        RawFragment {
            value: command_literal_from_command(&Command::Simple(simple.clone()), ctx),
        }
        .to_frag(),
    )
}

fn render_printf_v(simple: &SimpleCommand, ctx: &mut RenderContext) -> Option<String> {
    if simple.words.len() < 4 {
        return None;
    }

    if simple.words[0] != "printf" || simple.words[1] != "-v" {
        return None;
    }

    let target_raw = &simple.words[2];
    if !is_identifier(target_raw) {
        return None;
    }

    let target = ctx
        .resolve_var(target_raw)
        .unwrap_or_else(|| ctx.declare_var(target_raw));

    let mut args = Vec::new();
    for word in simple.words.iter().skip(3) {
        let expr = word_to_expr(word, ctx)?;
        if (expr.starts_with('"') && expr.ends_with('"')) || is_number(&expr) {
            args.push(expr);
        } else {
            args.push(format!("{{{expr}}}"));
        }
    }

    Some(format!(
        "trust $ printf -v {{nameof {target}}} {} $",
        args.join(" ")
    ))
}

fn render_function_call_statement(simple: &SimpleCommand, ctx: &mut RenderContext) -> Option<String> {
    let function_name = simple.words.first()?;
    let sig = ctx.resolve_function(function_name)?.clone();
    // Only FullFallback prevents native call site rendering
    if sig.render_mode == FunctionRenderMode::FullFallback {
        return None;
    }

    let expr = render_simple_function_call_expr(simple, ctx)?;

    if sig.returns_value
        && let Some(type_signature) = sig.typed_signature
        && let TypeCommentReturnContract::TypedVariable { variable_name, .. } =
            type_signature.return_contract
    {
        let existing = ctx.resolve_var(&variable_name);
        let target = existing
            .clone()
            .unwrap_or_else(|| ctx.declare_var(&variable_name));
        if existing.is_some() {
            return Some(format!("{target} = {expr}"));
        }
        return Some(format!("let {target} = {expr}"));
    }

    Some(expr)
}
