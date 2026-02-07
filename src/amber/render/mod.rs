mod context;
mod expr;
mod fallback;
mod syntax;

use crate::bash::ast::*;

use context::RenderContext;
use expr::{
    has_unresolved_shell_var, parse_assignment, parse_variable_reference, render_condition_expr,
    render_for_items, render_simple_function_call_expr, word_to_expr,
};
use fallback::command_literal_from_command;
use syntax::{
    detect_function_arity, is_double_quoted, is_identifier, is_number, is_reserved_keyword,
    normalize_identifier, sanitize_function_name,
};

pub fn render_program(program: &Program) -> String {
    let mut lines = Vec::new();
    let mut ctx = RenderContext::new();
    render_commands(&program.statements, 0, &mut ctx, &mut lines, false);

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn render_commands(
    commands: &[Command],
    indent: usize,
    ctx: &mut RenderContext,
    output: &mut Vec<String>,
    return_tail: bool,
) {
    for (index, command) in commands.iter().enumerate() {
        let tail_return = return_tail && index + 1 == commands.len();
        render_command(command, indent, ctx, output, tail_return);
    }
}

fn render_command(
    command: &Command,
    indent: usize,
    ctx: &mut RenderContext,
    output: &mut Vec<String>,
    tail_return: bool,
) {
    let pad = "    ".repeat(indent);
    match command {
        Command::If(if_cmd) => {
            if let Some(ternary_assignment) = render_if_ternary_assignment(if_cmd, ctx) {
                output.push(format!("{pad}{ternary_assignment}"));
                return;
            }

            let condition = render_condition_expr(&if_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&if_cmd.condition, ctx));

            output.push(format!("{pad}if {condition} {{"));
            let mut then_ctx = ctx.with_child_scope();
            render_commands(
                &if_cmd.then_body,
                indent + 1,
                &mut then_ctx,
                output,
                tail_return,
            );

            if let Some(else_body) = &if_cmd.else_body {
                output.push(format!("{pad}}} else {{"));
                let mut else_ctx = ctx.with_child_scope();
                render_commands(else_body, indent + 1, &mut else_ctx, output, tail_return);
                ctx.merge_from_child(else_ctx);
            }

            output.push(format!("{pad}}}"));
            ctx.merge_from_child(then_ctx);
        }
        Command::While(while_cmd) => {
            let condition = render_condition_expr(&while_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&while_cmd.condition, ctx));

            output.push(format!("{pad}while {condition} {{"));
            let mut loop_ctx = ctx.with_child_scope();
            render_commands(&while_cmd.body, indent + 1, &mut loop_ctx, output, false);
            output.push(format!("{pad}}}"));
            ctx.merge_from_child(loop_ctx);
        }
        Command::For(for_cmd) => {
            let items = render_for_items(&for_cmd.items, ctx).unwrap_or_else(|| {
                let command = Command::Simple(SimpleCommand {
                    words: for_cmd.items.clone(),
                });
                command_literal_from_command(&command, ctx)
            });

            let var_name = ctx.declare_var(&for_cmd.variable);
            output.push(format!("{pad}for {var_name} in {items} {{"));

            let mut loop_ctx = ctx.with_child_scope();
            loop_ctx.declare_var(&for_cmd.variable);
            render_commands(&for_cmd.body, indent + 1, &mut loop_ctx, output, false);

            output.push(format!("{pad}}}"));
            ctx.merge_from_child(loop_ctx);
        }
        Command::CStyleFor(for_cmd) => {
            if let Some(spec) = parse_c_style_for_range(for_cmd, ctx) {
                let var_name = ctx.declare_var(&spec.variable);
                let range_op = if spec.inclusive { "..=" } else { ".." };
                output.push(format!(
                    "{pad}for {var_name} in {}{range_op}{} {{",
                    spec.start, spec.end
                ));

                let mut loop_ctx = ctx.with_child_scope();
                loop_ctx.declare_var(&spec.variable);
                render_commands(&for_cmd.body, indent + 1, &mut loop_ctx, output, false);

                output.push(format!("{pad}}}"));
                ctx.merge_from_child(loop_ctx);
            } else {
                output.push(format!(
                    "{pad}{}",
                    command_literal_from_command(command, ctx)
                ));
            }
        }
        Command::Case(case_cmd) => {
            render_case(case_cmd, indent, ctx, output, tail_return);
        }
        Command::Function(function) => {
            render_function(function, indent, ctx, output);
        }
        Command::Group(body) => {
            render_commands(body, indent, ctx, output, tail_return);
        }
        Command::Simple(simple) => {
            if tail_return && let Some(expr) = render_tail_return_expr(simple, ctx) {
                output.push(format!("{pad}let result = {expr}"));
                output.push(format!("{pad}return result"));
            } else {
                output.push(format!("{pad}{}", render_simple(simple, ctx)));
            }
        }
        Command::Connection(connection) => {
            if let Some(rendered) = render_echo_ternary_connection(connection, ctx) {
                output.push(format!("{pad}{rendered}"));
            } else {
                output.push(format!(
                    "{pad}{}",
                    command_literal_from_command(command, ctx)
                ));
            }
        }
    }
}

fn render_function(
    function: &FunctionDef,
    indent: usize,
    ctx: &mut RenderContext,
    output: &mut Vec<String>,
) {
    let pad = "    ".repeat(indent);
    let fun_name = sanitize_function_name(&function.name);
    let arity = detect_function_arity(&function.body);
    let positional_bindings = infer_positional_bindings_prefix(&function.body, arity);
    let params = build_function_params(arity, &positional_bindings);
    let function_body = &function.body[positional_bindings.len()..];
    let return_mode = function_requires_value_return(function_body, &function.name);

    ctx.register_function(&function.name, fun_name.clone(), arity, return_mode);
    let params_rendered = if return_mode {
        params
            .iter()
            .map(|param| format!("{param}: Num"))
            .collect::<Vec<String>>()
            .join(", ")
    } else {
        params.join(", ")
    };
    if return_mode {
        output.push(format!("{pad}fun {fun_name}({params_rendered}): Num {{"));
    } else {
        output.push(format!("{pad}fun {fun_name}({params_rendered}) {{"));
    }

    let mut fn_ctx = ctx.with_child_scope();
    fn_ctx.push_positional_scope_with_names(&params);
    for (raw_name, index) in &positional_bindings {
        if let Some(alias) = params.get(index.saturating_sub(1)) {
            fn_ctx.bind_var_alias(raw_name, alias);
        }
    }
    render_commands(function_body, indent + 1, &mut fn_ctx, output, return_mode);
    fn_ctx.pop_positional_scope();

    output.push(format!("{pad}}}"));
}

#[derive(Debug)]
struct CStyleForRangeSpec {
    variable: String,
    start: String,
    end: String,
    inclusive: bool,
}

fn parse_c_style_for_range(
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
        return parse_variable_reference(raw, ctx).or_else(|| Some(raw.to_string()));
    }

    None
}

fn strip_all_spaces(input: &str) -> String {
    input.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn infer_positional_bindings_prefix(commands: &[Command], arity: usize) -> Vec<(String, usize)> {
    let mut bindings = Vec::new();
    for command in commands {
        let Some((raw_name, index)) = parse_positional_binding(command) else {
            break;
        };
        if index == 0 || index > arity {
            break;
        }
        bindings.push((raw_name, index));
    }
    bindings
}

fn parse_positional_binding(command: &Command) -> Option<(String, usize)> {
    let Command::Simple(simple) = command else {
        return None;
    };
    if simple.words.len() != 1 {
        return None;
    }

    let (raw_name, value) = simple.words[0].split_once('=')?;
    if !is_identifier(raw_name) {
        return None;
    }

    let index = parse_positional_index(value)?;
    Some((raw_name.to_string(), index))
}

fn parse_positional_index(value: &str) -> Option<usize> {
    let raw = if is_double_quoted(value) {
        &value[1..value.len() - 1]
    } else {
        value
    };

    if let Some(inner) = raw
        .strip_prefix("${")
        .and_then(|text| text.strip_suffix('}'))
    {
        return inner.parse::<usize>().ok();
    }

    raw.strip_prefix('$')?.parse::<usize>().ok()
}

fn build_function_params(arity: usize, bindings: &[(String, usize)]) -> Vec<String> {
    let mut params = (1..=arity)
        .map(|index| format!("arg{index}"))
        .collect::<Vec<String>>();

    for (raw_name, index) in bindings {
        if *index == 0 || *index > arity {
            continue;
        }
        let target_index = index - 1;
        let candidate = sanitize_param_name(raw_name);
        params[target_index] = unique_param_name(candidate, &params, target_index);
    }

    params
}

fn sanitize_param_name(raw_name: &str) -> String {
    let mut candidate = normalize_identifier(raw_name);
    if !is_identifier(&candidate) {
        candidate.insert(0, '_');
    }
    if is_reserved_keyword(&candidate) {
        candidate.push_str("_param");
    }
    candidate
}

fn unique_param_name(mut candidate: String, params: &[String], current_index: usize) -> String {
    let base = candidate.clone();
    let mut suffix = 2usize;

    while params
        .iter()
        .enumerate()
        .any(|(index, existing)| index != current_index && existing == &candidate)
    {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }

    candidate
}

fn function_requires_value_return(commands: &[Command], raw_function_name: &str) -> bool {
    commands
        .iter()
        .any(|command| command_contains_self_call(command, raw_function_name))
}

fn command_contains_self_call(command: &Command, raw_function_name: &str) -> bool {
    match command {
        Command::Simple(simple) => {
            if simple
                .words
                .first()
                .is_some_and(|word| word == raw_function_name)
            {
                return true;
            }
            let marker = format!("$({raw_function_name}");
            simple.words.iter().any(|word| word.contains(&marker))
        }
        Command::Connection(connection) => {
            command_contains_self_call(&connection.left, raw_function_name)
                || command_contains_self_call(&connection.right, raw_function_name)
        }
        Command::If(if_cmd) => {
            command_contains_self_call(&if_cmd.condition, raw_function_name)
                || function_requires_value_return(&if_cmd.then_body, raw_function_name)
                || if_cmd
                    .else_body
                    .as_ref()
                    .map(|body| function_requires_value_return(body, raw_function_name))
                    .unwrap_or(false)
        }
        Command::While(while_cmd) => {
            command_contains_self_call(&while_cmd.condition, raw_function_name)
                || function_requires_value_return(&while_cmd.body, raw_function_name)
        }
        Command::For(for_cmd) => function_requires_value_return(&for_cmd.body, raw_function_name),
        Command::CStyleFor(for_cmd) => {
            function_requires_value_return(&for_cmd.body, raw_function_name)
        }
        Command::Case(case_cmd) => case_cmd
            .clauses
            .iter()
            .any(|clause| function_requires_value_return(&clause.body, raw_function_name)),
        Command::Function(function) => {
            function_requires_value_return(&function.body, raw_function_name)
        }
        Command::Group(body) => function_requires_value_return(body, raw_function_name),
    }
}

fn render_tail_return_expr(simple: &SimpleCommand, ctx: &mut RenderContext) -> Option<String> {
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

    render_function_call(simple, ctx)
}

fn render_if_ternary_assignment(if_cmd: &IfCommand, ctx: &mut RenderContext) -> Option<String> {
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

fn render_case(
    case_cmd: &CaseCommand,
    indent: usize,
    ctx: &mut RenderContext,
    output: &mut Vec<String>,
    return_tail: bool,
) {
    let pad = "    ".repeat(indent);
    let Some(subject) = word_to_expr(&case_cmd.word, ctx) else {
        output.push(format!(
            "{pad}{}",
            command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
        ));
        return;
    };

    let mut rendered_clauses = Vec::new();
    let mut rendered_else = None::<&CaseClause>;

    for (index, clause) in case_cmd.clauses.iter().enumerate() {
        if !matches!(
            clause.terminator,
            CaseClauseTerminator::Break | CaseClauseTerminator::End
        ) {
            output.push(format!(
                "{pad}{}",
                command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
            ));
            return;
        }

        if clause.patterns.len() == 1 && clause.patterns[0] == "*" {
            if index + 1 != case_cmd.clauses.len() {
                output.push(format!(
                    "{pad}{}",
                    command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
                ));
                return;
            }
            if rendered_else.is_some() {
                output.push(format!(
                    "{pad}{}",
                    command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
                ));
                return;
            }
            rendered_else = Some(clause);
            continue;
        }

        let mut conditions = Vec::new();
        for pattern in &clause.patterns {
            let Some(condition) = render_case_pattern_condition(&subject, pattern, ctx) else {
                output.push(format!(
                    "{pad}{}",
                    command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
                ));
                return;
            };
            conditions.push(condition);
        }

        if conditions.is_empty() {
            output.push(format!(
                "{pad}{}",
                command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
            ));
            return;
        }

        rendered_clauses.push((conditions.join(" or "), clause));
    }

    if rendered_clauses.is_empty() && rendered_else.is_none() {
        output.push(format!(
            "{pad}{}",
            command_literal_from_command(&Command::Case(case_cmd.clone()), ctx)
        ));
        return;
    }

    output.push(format!("{pad}if {{"));

    for (condition, clause) in rendered_clauses {
        output.push(format!("{pad}    {condition} {{"));
        let mut clause_ctx = ctx.with_child_scope();
        render_commands(
            &clause.body,
            indent + 2,
            &mut clause_ctx,
            output,
            return_tail,
        );
        output.push(format!("{pad}    }}"));
        ctx.merge_from_child(clause_ctx);
    }

    if let Some(clause) = rendered_else {
        output.push(format!("{pad}    else {{"));
        let mut clause_ctx = ctx.with_child_scope();
        render_commands(
            &clause.body,
            indent + 2,
            &mut clause_ctx,
            output,
            return_tail,
        );
        output.push(format!("{pad}    }}"));
        ctx.merge_from_child(clause_ctx);
    }

    output.push(format!("{pad}}}"));
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

fn render_simple(simple: &SimpleCommand, ctx: &mut RenderContext) -> String {
    if let Some(assignment) = parse_assignment(simple, ctx) {
        if assignment.is_reassignment {
            return format!("{} = {}", assignment.name, assignment.value);
        }
        return format!("let {} = {}", assignment.name, assignment.value);
    }

    if let Some(printf_v) = render_printf_v(simple, ctx) {
        return printf_v;
    }

    if let Some(call) = render_function_call_statement(simple, ctx) {
        return call;
    }

    if simple.words.first().is_some_and(|word| word == "echo") {
        if simple.words.len() == 1 {
            return "echo(\"\")".to_string();
        }

        if simple.words.len() == 2
            && let Some(expr) = word_to_expr(&simple.words[1], ctx)
        {
            return format!("echo({expr})");
        }

        if simple.words.len() > 2 {
            let merged = simple.words[1..].join(" ");
            let merged_trimmed = merged.trim();
            if merged_trimmed.starts_with("$((")
                && merged_trimmed.ends_with("))")
                && let Some(expr) = word_to_expr(merged_trimmed, ctx)
            {
                return format!("echo({expr})");
            }
        }
    }

    command_literal_from_command(&Command::Simple(simple.clone()), ctx)
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

fn render_function_call(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    render_simple_function_call_expr(simple, ctx)
}

fn render_function_call_statement(simple: &SimpleCommand, ctx: &RenderContext) -> Option<String> {
    let function_name = simple.words.first()?;
    let sig = ctx.resolve_function(function_name)?;
    let expr = render_function_call(simple, ctx)?;
    if sig.returns_value {
        Some(format!("echo({expr})"))
    } else {
        Some(expr)
    }
}

fn render_echo_ternary_connection(connection: &Connection, ctx: &RenderContext) -> Option<String> {
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
