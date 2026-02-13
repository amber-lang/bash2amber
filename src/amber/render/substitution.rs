use crate::amber::builtins;
use crate::bash::ast::*;
use crate::bash::parser;

use super::context::RenderContext;
use super::fallback::{command_literal_from_command, command_literal_from_shell};
use super::word::{render_condition_expr, render_simple_function_call_expr, word_to_expr};

pub(super) fn render_and_or_ternary_expr(command: &Command, ctx: &RenderContext) -> Option<String> {
    let Command::Connection(or_connection) = command else {
        return None;
    };

    if or_connection.op != Connector::Or {
        return None;
    }

    let Command::Connection(and_connection) = or_connection.left.as_ref() else {
        return None;
    };

    if and_connection.op != Connector::And {
        return None;
    }

    let condition = render_condition_expr(&and_connection.left, ctx)?;
    let when_true = render_command_output_expr(and_connection.right.as_ref(), ctx)?;
    let when_false = render_command_output_expr(or_connection.right.as_ref(), ctx)?;

    Some(format!("{condition} then {when_true} else {when_false}"))
}

pub(super) fn render_command_output_expr(command: &Command, ctx: &RenderContext) -> Option<String> {
    let Command::Simple(simple) = command else {
        return None;
    };

    if simple.words.is_empty() {
        return None;
    }

    if simple.words[0] == "echo" {
        if simple.words.len() == 1 {
            return Some("\"\"".to_string());
        }
        if simple.words.len() == 2 {
            return word_to_expr(&simple.words[1], ctx);
        }
        return None;
    }

    if simple.words.len() == 1 {
        return word_to_expr(&simple.words[0], ctx);
    }

    None
}

pub(super) fn parse_if_command_substitution_expression(value: &str, ctx: &RenderContext) -> Option<String> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("$(")?.strip_suffix(')')?.trim();

    let parsed = parser::parse(inner, None).ok()?;
    if parsed.statements.len() != 1 {
        return None;
    }

    let Command::If(if_cmd) = parsed.statements.first()? else {
        return None;
    };

    let condition = render_condition_expr(&if_cmd.condition, ctx)?;
    let then_expr = extract_echo_expression(&if_cmd.then_body, ctx)?;
    let else_expr = extract_echo_expression(if_cmd.else_body.as_ref()?, ctx)?;

    Some(format!("{condition} then {then_expr} else {else_expr}"))
}

pub(super) fn parse_and_or_command_substitution_expression(
    value: &str,
    ctx: &RenderContext,
) -> Option<String> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("$(")?.strip_suffix(')')?.trim();

    let parsed = parser::parse(inner, None).ok()?;
    if parsed.statements.len() != 1 {
        return None;
    }

    let statement = parsed.statements.first()?;
    render_and_or_ternary_expr(statement, ctx)
}

pub(super) fn parse_function_call_command_substitution_expression(
    value: &str,
    ctx: &RenderContext,
) -> Option<String> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("$(")?.strip_suffix(')')?.trim();

    let parsed = parser::parse(inner, None).ok()?;
    if parsed.statements.len() != 1 {
        return None;
    }

    let Command::Simple(simple) = parsed.statements.first()? else {
        return None;
    };

    render_simple_function_call_expr(simple, ctx)
}

pub(super) fn parse_generic_command_substitution_expression(
    value: &str,
    ctx: &RenderContext,
) -> Option<String> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("$(")?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return None;
    }

    let parsed = parser::parse(inner, None);
    if let Ok(parsed) = parsed
        && parsed.statements.len() == 1
    {
        let command = parsed.statements.first()?;
        if let Command::Simple(simple) = command {
            if let Some(builtin_expr) = builtins::render_builtin_expr(simple, ctx) {
                return Some(builtin_expr);
            }
        }
        return Some(command_literal_from_command(command, ctx));
    }

    Some(command_literal_from_shell(inner))
}

fn extract_echo_expression(body: &[Command], ctx: &RenderContext) -> Option<String> {
    if body.len() != 1 {
        return None;
    }

    let Command::Simple(simple) = &body[0] else {
        return None;
    };

    if simple.words.first()?.as_str() != "echo" {
        return None;
    }

    if simple.words.len() == 1 {
        return Some("\"\"".to_string());
    }

    if simple.words.len() == 2 {
        return word_to_expr(&simple.words[1], ctx);
    }

    None
}
