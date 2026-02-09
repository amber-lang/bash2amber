use crate::bash::ast::*;

use crate::amber::fragments::{
    BlockFragment, FragmentKind, FragmentRenderable, IfChainBranch, IfChainFragment, RawFragment,
};

use super::context::RenderContext;
use super::fallback::command_literal_from_command;
use super::fragment_expr::render_expression_fragment;
use super::syntax::{is_identifier, is_number};
use super::word::{
    has_unresolved_shell_var, parse_assignment, parse_variable_reference, render_condition_expr,
    render_simple_function_call_expr, word_to_expr,
};

#[derive(Debug)]
pub(super) struct CStyleForRangeSpec {
    pub(super) variable: String,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) inclusive: bool,
}

pub(super) fn parse_c_style_for_range(
    for_cmd: &CStyleForCommand,
    ctx: &RenderContext,
) -> Option<CStyleForRangeSpec> {
    let init = strip_all_spaces(&for_cmd.init);
    let condition = strip_all_spaces(&for_cmd.condition);
    let update = strip_all_spaces(&for_cmd.update);

    let (variable, start_raw) = init.split_once('=')?;
    if !is_identifier(variable) {
        return None;
    }

    let (cond_var, op, end_raw) = parse_c_style_condition(&condition)?;
    if cond_var != variable {
        return None;
    }

    let update_kind = parse_c_style_update(&update, variable)?;
    let cond_kind = match op {
        "<" | "<=" => CStyleUpdateKind::Increment,
        ">" | ">=" => CStyleUpdateKind::Decrement,
        _ => return None,
    };
    if update_kind != cond_kind {
        return None;
    }

    let start = parse_c_style_bound_expr(start_raw, ctx)?;
    let end = parse_c_style_bound_expr(end_raw, ctx)?;

    Some(CStyleForRangeSpec {
        variable: variable.to_string(),
        start,
        end,
        inclusive: op.ends_with('='),
    })
}

fn parse_c_style_condition(condition: &str) -> Option<(&str, &str, &str)> {
    for op in ["<=", ">=", "<", ">"] {
        if let Some((lhs, rhs)) = condition.split_once(op) {
            if !lhs.is_empty() && !rhs.is_empty() {
                return Some((lhs, op, rhs));
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CStyleUpdateKind {
    Increment,
    Decrement,
}

fn parse_c_style_update(update: &str, variable: &str) -> Option<CStyleUpdateKind> {
    if update == format!("{variable}++") || update == format!("++{variable}") {
        return Some(CStyleUpdateKind::Increment);
    }
    if update == format!("{variable}--") || update == format!("--{variable}") {
        return Some(CStyleUpdateKind::Decrement);
    }
    None
}

fn parse_c_style_bound_expr(raw: &str, ctx: &RenderContext) -> Option<String> {
    if raw.is_empty() {
        return None;
    }

    if is_number(raw) {
        return Some(raw.to_string());
    }

    if is_identifier(raw) {
        if let Some(alias) = ctx.resolve_var(raw) {
            return Some(alias);
        }
        return parse_variable_reference(raw, ctx);
    }

    None
}

fn strip_all_spaces(input: &str) -> String {
    input.chars().filter(|ch| !ch.is_whitespace()).collect()
}

pub(super) fn render_if_ternary_assignment(if_cmd: &IfCommand, ctx: &mut RenderContext) -> Option<String> {
    let else_body = if_cmd.else_body.as_ref()?;
    if if_cmd.then_body.len() != 1 || else_body.len() != 1 {
        return None;
    }

    let condition = render_condition_expr(&if_cmd.condition, ctx)?;

    let then_simple = match &if_cmd.then_body[0] {
        Command::Simple(simple) => simple,
        _ => return None,
    };

    let else_simple = match &else_body[0] {
        Command::Simple(simple) => simple,
        _ => return None,
    };

    let mut then_ctx = ctx.with_child_scope();
    let then_assignment = parse_assignment(then_simple, &mut then_ctx)?;

    let mut else_ctx = ctx.with_child_scope();
    let else_assignment = parse_assignment(else_simple, &mut else_ctx)?;

    if then_assignment.raw_name != else_assignment.raw_name {
        return None;
    }

    if then_assignment.is_reassignment != else_assignment.is_reassignment {
        return None;
    }

    if !then_assignment.is_reassignment {
        let declared = ctx.declare_var(&then_assignment.raw_name);
        if declared != then_assignment.name {
            return None;
        }
    }

    let lhs = if then_assignment.is_reassignment {
        then_assignment.name.clone()
    } else {
        format!("let {}", then_assignment.name)
    };

    Some(format!(
        "{lhs} = {condition} then {} else {}",
        then_assignment.value, else_assignment.value
    ))
}

pub(super) fn render_tail_return_expr(simple: &SimpleCommand, ctx: &mut RenderContext) -> Option<String> {
    if simple.words.first().is_some_and(|word| word == "echo") {
        if simple.words.len() == 1 {
            return Some("\"\"".to_string());
        }
        if simple.words.len() == 2 {
            return word_to_expr(&simple.words[1], ctx);
        }
        let merged = simple.words[1..].join(" ");
        let merged_trimmed = merged.trim();
        if merged_trimmed.starts_with("$((") && merged_trimmed.ends_with("))") {
            return word_to_expr(merged_trimmed, ctx);
        }
        return None;
    }

    if let Some(return_var) = ctx.current_return_var() {
        let assigns_return_var = simple
            .words
            .first()
            .and_then(|word| word.split_once('='))
            .is_some_and(|(raw_name, _)| raw_name == return_var);
        if assigns_return_var && let Some(assignment) = parse_assignment(simple, ctx) {
            return Some(assignment.value);
        }
    }

    render_simple_function_call_expr(simple, ctx)
}

fn case_fallback(case_cmd: &CaseCommand, ctx: &RenderContext) -> FragmentKind {
    RawFragment {
        value: command_literal_from_command(&Command::Case(case_cmd.clone()), ctx),
    }
    .to_frag()
}

pub(super) fn render_case(
    case_cmd: &CaseCommand,
    ctx: &mut RenderContext,
    return_tail: bool,
    render_commands: fn(&[Command], &mut RenderContext, bool) -> Vec<FragmentKind>,
) -> FragmentKind {
    let Some(subject) = word_to_expr(&case_cmd.word, ctx) else {
        return case_fallback(case_cmd, ctx);
    };

    let mut rendered_clauses = Vec::new();
    let mut rendered_else = None::<&CaseClause>;

    for (index, clause) in case_cmd.clauses.iter().enumerate() {
        if !matches!(
            clause.terminator,
            CaseClauseTerminator::Break | CaseClauseTerminator::End
        ) {
            return case_fallback(case_cmd, ctx);
        }

        if clause.patterns.len() == 1 && clause.patterns[0] == "*" {
            if index + 1 != case_cmd.clauses.len() {
                return case_fallback(case_cmd, ctx);
            }
            if rendered_else.is_some() {
                return case_fallback(case_cmd, ctx);
            }
            rendered_else = Some(clause);
            continue;
        }

        let mut conditions = Vec::new();
        for pattern in &clause.patterns {
            let Some(condition) = render_case_pattern_condition(&subject, pattern, ctx) else {
                return case_fallback(case_cmd, ctx);
            };
            conditions.push(condition);
        }

        if conditions.is_empty() {
            return case_fallback(case_cmd, ctx);
        }

        rendered_clauses.push((conditions.join(" or "), clause));
    }

    if rendered_clauses.is_empty() && rendered_else.is_none() {
        return case_fallback(case_cmd, ctx);
    }

    let mut branches = Vec::new();

    for (condition, clause) in rendered_clauses {
        let mut clause_ctx = ctx.with_child_scope();
        let body = BlockFragment::new(
            render_commands(&clause.body, &mut clause_ctx, return_tail),
            true,
        );
        ctx.merge_from_child(clause_ctx);
        branches.push(IfChainBranch {
            condition: Box::new(render_expression_fragment(&condition)),
            body,
        });
    }

    let else_body = if let Some(clause) = rendered_else {
        let mut clause_ctx = ctx.with_child_scope();
        let body = BlockFragment::new(
            render_commands(&clause.body, &mut clause_ctx, return_tail),
            true,
        );
        ctx.merge_from_child(clause_ctx);
        Some(body)
    } else {
        None
    };

    IfChainFragment {
        branches,
        else_body,
    }
    .to_frag()
}

fn render_case_pattern_condition(
    subject: &str,
    pattern: &str,
    ctx: &RenderContext,
) -> Option<String> {
    if pattern == "*" {
        return None;
    }

    if pattern
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | ']'))
    {
        return None;
    }

    if has_unresolved_shell_var(pattern, ctx) {
        return None;
    }

    let rhs = word_to_expr(pattern, ctx)?;
    Some(format!("{subject} == {rhs}"))
}

pub(super) fn render_echo_ternary_connection(connection: &Connection, ctx: &RenderContext) -> Option<String> {
    if connection.op != Connector::Or {
        return None;
    }

    let Command::Connection(and_connection) = connection.left.as_ref() else {
        return None;
    };

    if and_connection.op != Connector::And {
        return None;
    }

    let condition = render_condition_expr(&and_connection.left, ctx)?;
    let when_true = echo_payload_expr(and_connection.right.as_ref(), ctx)?;
    let when_false = echo_payload_expr(connection.right.as_ref(), ctx)?;

    Some(format!(
        "echo({condition} then {when_true} else {when_false})"
    ))
}

fn echo_payload_expr(command: &Command, ctx: &RenderContext) -> Option<String> {
    let Command::Simple(simple) = command else {
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
