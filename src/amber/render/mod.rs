mod context;
mod expr;
mod fallback;
mod syntax;

use std::collections::HashMap;

use crate::amber::fragments::{
    BlockFragment, ForFragment, FragmentKind, FragmentRenderable, Fragments, FunctionFragment,
    IfChainBranch, IfChainFragment, IfFragment, InterpolableFragment, ListFragment, RawFragment,
    TranslateMetadata, VarExprFragment, VarStmtFragment, WhileFragment,
};
use crate::bash::ast::*;
use crate::bash::parser;

use context::{
    FunctionHint, FunctionRenderMode, FunctionSig, FunctionTypeHint, RenderContext,
    TypeCommentParam, TypeCommentReturnContract, TypeCommentSignature,
};
use expr::{
    has_unresolved_shell_var, parse_assignment, parse_variable_reference,
    render_arithmetic_statement_expr, render_condition_expr, render_for_items,
    render_simple_function_call_expr, word_to_expr,
};
use fallback::command_literal_from_command;
use syntax::{
    detect_function_arity, is_double_quoted, is_identifier, is_number, is_reserved_keyword,
    normalize_identifier, sanitize_function_name,
};

pub fn render_program(
    program: &Program,
    source: Option<&str>,
    source_path: Option<&str>,
) -> String {
    let hints = source.map(collect_function_hints).unwrap_or_default();
    let mut ctx = RenderContext::new(hints, source_path.map(ToOwned::to_owned));
    let items = render_commands(&program.statements, &mut ctx, false);

    let fragments = Fragments {
        fragment: FragmentKind::block(items),
    };
    fragments.to_string(&mut TranslateMetadata::default())
}

fn collect_function_hints(source: &str) -> HashMap<String, Vec<FunctionHint>> {
    let mut by_name: HashMap<String, Vec<FunctionHint>> = HashMap::new();
    let lines = source.lines().collect::<Vec<&str>>();

    for (index, line) in lines.iter().enumerate() {
        let Some(name) = parse_function_name_from_line(line) else {
            continue;
        };

        let function_line = index + 1;
        let type_hint = find_preceding_type_hint(&lines, index);
        by_name.entry(name).or_default().push(FunctionHint {
            function_line,
            type_hint,
        });
    }

    by_name
}

fn find_preceding_type_hint(lines: &[&str], function_line_index: usize) -> FunctionTypeHint {
    let mut scan = function_line_index;
    while scan > 0 {
        scan -= 1;
        let trimmed = lines[scan].trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with('#') {
            return FunctionTypeHint::Missing;
        }
        if !trimmed.starts_with("##") {
            continue;
        }

        let comment_line = scan + 1;
        return parse_type_comment(trimmed, comment_line);
    }

    FunctionTypeHint::Missing
}

fn parse_type_comment(comment_line_text: &str, comment_line: usize) -> FunctionTypeHint {
    let raw = comment_line_text
        .trim()
        .strip_prefix("##")
        .unwrap_or(comment_line_text.trim())
        .trim();
    let Some(signature) = parse_type_comment_signature(raw, comment_line) else {
        return FunctionTypeHint::Invalid {
            comment_line,
            raw: raw.to_string(),
        };
    };
    FunctionTypeHint::Typed(signature)
}

fn parse_type_comment_signature(raw: &str, comment_line: usize) -> Option<TypeCommentSignature> {
    let raw = raw.trim();
    if !raw.starts_with('(') {
        return None;
    }

    let close = raw.find(')')?;
    let params_raw = raw[1..close].trim();
    let rest = raw[close + 1..].trim();
    let return_text = rest.strip_prefix(':')?.trim();
    let return_contract = parse_type_comment_return_contract(return_text)?;

    let mut params = Vec::new();
    if !params_raw.is_empty() {
        for part in params_raw.split(',') {
            let part = part.trim();
            let (name, type_name) = part.split_once(':')?;
            let name = name.trim();
            let type_name = type_name.trim();
            if !is_identifier(name) || !is_valid_type_hint(type_name) {
                return None;
            }
            params.push(TypeCommentParam {
                name: name.to_string(),
                type_name: type_name.to_string(),
            });
        }
    }

    Some(TypeCommentSignature {
        params,
        return_contract,
        comment_line,
    })
}

fn parse_type_comment_return_contract(raw: &str) -> Option<TypeCommentReturnContract> {
    if raw == "Null" {
        return Some(TypeCommentReturnContract::Null);
    }

    let open = raw.find('(')?;
    let close = raw.rfind(')')?;
    if close <= open {
        return None;
    }
    if !raw[close + 1..].trim().is_empty() {
        return None;
    }

    let type_name = raw[..open].trim();
    let variable_name = raw[open + 1..close].trim();

    if type_name != "Int" || !is_identifier(variable_name) {
        return None;
    }

    Some(TypeCommentReturnContract::TypedVariable {
        type_name: type_name.to_string(),
        variable_name: variable_name.to_string(),
    })
}

fn is_valid_type_hint(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut bracket_depth = 0usize;
    let mut has_alpha = false;
    for ch in trimmed.chars() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => {
                if bracket_depth == 0 {
                    return false;
                }
                bracket_depth -= 1;
            }
            '|' | ' ' | '_' => {}
            _ if ch.is_ascii_alphanumeric() => {
                if ch.is_ascii_alphabetic() {
                    has_alpha = true;
                }
            }
            _ => return false,
        }
    }

    bracket_depth == 0 && has_alpha
}

fn parse_function_name_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("function ") {
        let rest = rest.trim_start();
        let (name, tail) = split_identifier_prefix(rest)?;
        let mut tail = tail.trim_start();
        if let Some(after) = tail.strip_prefix("()") {
            tail = after.trim_start();
        }
        if tail.is_empty() || tail.starts_with('{') {
            return Some(name.to_string());
        }
    }

    let (name, tail) = split_identifier_prefix(trimmed)?;
    let tail = tail.trim_start();
    let tail = tail.strip_prefix("()")?.trim_start();
    if tail.is_empty() || tail.starts_with('{') {
        return Some(name.to_string());
    }
    None
}

fn split_identifier_prefix(text: &str) -> Option<(&str, &str)> {
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        if index == 0 {
            if !(ch.is_ascii_alphabetic() || ch == '_') {
                return None;
            }
            end = ch.len_utf8();
            continue;
        }

        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = index + ch.len_utf8();
            continue;
        }
        break;
    }

    if end == 0 {
        return None;
    }

    let name = &text[..end];
    if !is_identifier(name) {
        return None;
    }
    Some((name, &text[end..]))
}

fn render_commands(
    commands: &[Command],
    ctx: &mut RenderContext,
    return_tail: bool,
) -> Vec<FragmentKind> {
    preregister_function_signatures(commands, ctx);

    let mut fragments = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let tail_return = return_tail && index + 1 == commands.len();
        fragments.extend(render_command(command, ctx, tail_return));
    }
    fragments
}

fn preregister_function_signatures(commands: &[Command], ctx: &mut RenderContext) {
    for command in commands {
        let Command::Function(function) = command else {
            continue;
        };

        let amber_name = sanitize_function_name(&function.name);
        let arity = detect_function_arity(&function.body);
        let positional_bindings = infer_positional_bindings_prefix(&function.body, arity);
        let function_body = &function.body[positional_bindings.len()..];
        let hint = ctx.next_function_hint(&function.name);

        let mut sig = FunctionSig {
            amber_name,
            arity,
            returns_value: false,
            render_mode: FunctionRenderMode::Native,
            typed_signature: None,
        };

        // Check if function has a type comment - if so, use it
        if let Some(hint) = hint.as_ref() {
            if let FunctionTypeHint::Typed(type_signature) = &hint.type_hint {
                if type_signature.params.len() == arity {
                    if let TypeCommentReturnContract::TypedVariable { variable_name, .. } =
                        &type_signature.return_contract
                    {
                        if function_contains_assignment_target(function_body, variable_name) {
                            sig.returns_value = true;
                            sig.typed_signature = Some(type_signature.clone());
                            ctx.register_function(&function.name, sig);
                            continue;
                        }
                    }
                }
            }
        }

        // No valid type comment - check if function captures self-call values
        let captures_self_call_value =
            function_captures_self_call_value(function_body, &function.name);
        if captures_self_call_value {
            // Needs type comment but doesn't have one - fallback
            sig.render_mode = FunctionRenderMode::FallbackLiteral;
            let line = hint.as_ref().map(|item| item.function_line).unwrap_or(0);
            let reason = "function captures return value from recursive call but has no type comment";
            if line > 0 {
                ctx.warn(
                    &format!(
                        "Function '{}' at line {line} is unsupported without explicit type comment. Falling back to trust literal.",
                        function.name
                    ),
                    Some(reason),
                );
            } else {
                ctx.warn(
                    &format!(
                        "Function '{}' is unsupported without explicit type comment. Falling back to trust literal.",
                        function.name
                    ),
                    Some(reason),
                );
            }
        }

        ctx.register_function(&function.name, sig);
    }
}

fn type_comment_for_unsupported_function(
    hint: Option<&FunctionHint>,
    arity: usize,
    function_body: &[Command],
) -> Result<TypeCommentSignature, String> {
    let Some(hint) = hint else {
        return Err("missing type comment immediately above the function (supported: `## (arg1: Int, arg2: Int): Null` or `## (arg1: Int, arg2: Int): Int(return_var)`)".to_string());
    };

    match &hint.type_hint {
        FunctionTypeHint::Missing => Err("missing type comment immediately above the function (supported: `## (arg1: Int, arg2: Int): Null` or `## (arg1: Int, arg2: Int): Int(return_var)`)".to_string()),
        FunctionTypeHint::Invalid { comment_line, raw } => Err(format!(
            "invalid type comment at line {comment_line}: `{raw}`"
        )),
        FunctionTypeHint::Typed(signature) => {
            if signature.params.len() != arity {
                return Err(format!(
                    "type comment at line {} defines {} args but function uses {}",
                    signature.comment_line,
                    signature.params.len(),
                    arity
                ));
            }
            let TypeCommentReturnContract::TypedVariable { variable_name, .. } =
                &signature.return_contract
            else {
                return Err(
                    "unsupported value-returning function cannot use `: Null`; use `: Int(return_var)`"
                        .to_string(),
                );
            };
            if !function_contains_assignment_target(function_body, variable_name) {
                return Err(format!(
                    "declared return variable `{variable_name}` is not assigned in function body"
                ));
            }
            Ok(signature.clone())
        }
    }
}

fn render_command(
    command: &Command,
    ctx: &mut RenderContext,
    tail_return: bool,
) -> Vec<FragmentKind> {
    match command {
        Command::If(if_cmd) => {
            if let Some(ternary_assignment) = render_if_ternary_assignment(if_cmd, ctx) {
                return vec![
                    RawFragment {
                        value: ternary_assignment,
                    }
                    .to_frag(),
                ];
            }

            let condition = render_condition_expr(&if_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&if_cmd.condition, ctx));

            let mut then_ctx = ctx.with_child_scope();
            let then_body = BlockFragment::new(
                render_commands(&if_cmd.then_body, &mut then_ctx, tail_return),
                true,
            );

            let else_body = if let Some(else_body) = &if_cmd.else_body {
                let mut else_ctx = ctx.with_child_scope();
                let rendered = BlockFragment::new(
                    render_commands(else_body, &mut else_ctx, tail_return),
                    true,
                );
                ctx.merge_from_child(else_ctx);
                Some(rendered)
            } else {
                None
            };

            ctx.merge_from_child(then_ctx);
            vec![
                IfFragment {
                    condition: Box::new(render_expression_fragment(&condition)),
                    then_body,
                    else_body,
                }
                .to_frag(),
            ]
        }
        Command::While(while_cmd) => {
            let condition = render_condition_expr(&while_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&while_cmd.condition, ctx));

            let mut loop_ctx = ctx.with_child_scope();
            let body =
                BlockFragment::new(render_commands(&while_cmd.body, &mut loop_ctx, false), true);
            ctx.merge_from_child(loop_ctx);
            vec![
                WhileFragment {
                    condition: Box::new(render_expression_fragment(&condition)),
                    body,
                }
                .to_frag(),
            ]
        }
        Command::For(for_cmd) => {
            let items = render_for_items(&for_cmd.items, ctx).unwrap_or_else(|| {
                let command = Command::Simple(SimpleCommand {
                    words: for_cmd.items.clone(),
                });
                command_literal_from_command(&command, ctx)
            });

            let var_name = ctx.declare_var(&for_cmd.variable);

            let mut loop_ctx = ctx.with_child_scope();
            loop_ctx.declare_var(&for_cmd.variable);
            let body =
                BlockFragment::new(render_commands(&for_cmd.body, &mut loop_ctx, false), true);
            ctx.merge_from_child(loop_ctx);

            vec![
                ForFragment {
                    variable: var_name,
                    items,
                    body,
                }
                .to_frag(),
            ]
        }
        Command::CStyleFor(for_cmd) => {
            if let Some(spec) = parse_c_style_for_range(for_cmd, ctx) {
                let var_name = ctx.declare_var(&spec.variable);
                let range_op = if spec.inclusive { "..=" } else { ".." };
                let items = format!("{}{range_op}{}", spec.start, spec.end);

                let mut loop_ctx = ctx.with_child_scope();
                loop_ctx.declare_var(&spec.variable);
                let body =
                    BlockFragment::new(render_commands(&for_cmd.body, &mut loop_ctx, false), true);
                ctx.merge_from_child(loop_ctx);

                vec![
                    ForFragment {
                        variable: var_name,
                        items,
                        body,
                    }
                    .to_frag(),
                ]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
        Command::Arithmetic(arith) => {
            if let Some(rendered) = render_arithmetic_statement_expr(&arith.expression, ctx) {
                vec![RawFragment { value: rendered }.to_frag()]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
        Command::Background(_) => vec![
            RawFragment {
                value: command_literal_from_command(command, ctx),
            }
            .to_frag(),
        ],
        Command::Case(case_cmd) => vec![render_case(case_cmd, ctx, tail_return)],
        Command::Function(function) => vec![render_function(function, ctx)],
        Command::Group(body) => render_commands(body, ctx, tail_return),
        Command::Simple(simple) => {
            if tail_return && let Some(expr) = render_tail_return_expr(simple, ctx) {
                return vec![
                    RawFragment {
                        value: format!("return {expr}"),
                    }
                    .to_frag(),
                ];
            }
            vec![render_simple(simple, ctx)]
        }
        Command::Connection(connection) => {
            if let Some(rendered) = render_echo_ternary_connection(connection, ctx) {
                vec![RawFragment { value: rendered }.to_frag()]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
    }
}

fn render_function(function: &FunctionDef, ctx: &mut RenderContext) -> FragmentKind {
    let sig = ctx.resolve_function(&function.name).cloned();
    if sig
        .as_ref()
        .is_some_and(|sig| sig.render_mode == FunctionRenderMode::FallbackLiteral)
    {
        return RawFragment {
            value: command_literal_from_command(&Command::Function(function.clone()), ctx),
        }
        .to_frag();
    }

    let fun_name = sig
        .as_ref()
        .map(|sig| sig.amber_name.clone())
        .unwrap_or_else(|| sanitize_function_name(&function.name));
    let arity = sig
        .as_ref()
        .map(|sig| sig.arity)
        .unwrap_or_else(|| detect_function_arity(&function.body));
    let positional_bindings = infer_positional_bindings_prefix(&function.body, arity);
    let mut params = build_function_params(arity, &positional_bindings);
    let mut typed_params = None::<Vec<String>>;
    let mut return_type = None::<String>;
    let mut return_var_name = None::<String>;
    let mut return_tail = false;

    if let Some(sig) = sig.as_ref()
        && sig.returns_value
    {
        return_tail = true;
        if let Some(type_comment) = sig.typed_signature.as_ref() {
            let mut comment_names = Vec::new();
            let mut rendered_typed = Vec::new();

            for param in &type_comment.params {
                let name =
                    push_unique_param_name(&mut comment_names, sanitize_param_name(&param.name));
                rendered_typed.push(format!("{name}: {}", param.type_name));
            }

            params = comment_names;
            typed_params = Some(rendered_typed);
            if let TypeCommentReturnContract::TypedVariable {
                type_name,
                variable_name,
            } = &type_comment.return_contract
            {
                return_type = Some(type_name.clone());
                return_var_name = Some(variable_name.clone());
            }
        }
    }

    let function_body = &function.body[positional_bindings.len()..];

    let mut fn_ctx = ctx.with_child_scope();
    if let Some(return_var_name) = &return_var_name {
        fn_ctx.push_return_var(return_var_name);
    }
    fn_ctx.push_positional_scope_with_names(&params);
    for (raw_name, index) in &positional_bindings {
        if let Some(alias) = params.get(index.saturating_sub(1)) {
            fn_ctx.bind_var_alias(raw_name, alias);
        }
    }
    let body = BlockFragment::new(
        render_commands(function_body, &mut fn_ctx, return_tail),
        true,
    );
    if return_tail {
        fn_ctx.pop_return_var();
    }
    fn_ctx.pop_positional_scope();

    FunctionFragment {
        name: fun_name,
        params: typed_params.unwrap_or(params),
        return_type,
        body,
    }
    .to_frag()
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
        return parse_variable_reference(raw, ctx);
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

fn push_unique_param_name(params: &mut Vec<String>, candidate: String) -> String {
    let base = candidate;
    let mut name = base.clone();
    let mut suffix = 2usize;

    while params.iter().any(|existing| existing == &name) {
        name = format!("{base}_{suffix}");
        suffix += 1;
    }

    params.push(name.clone());
    name
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

fn function_contains_assignment_target(commands: &[Command], variable_name: &str) -> bool {
    commands
        .iter()
        .any(|command| command_contains_assignment_target(command, variable_name))
}

fn command_contains_assignment_target(command: &Command, variable_name: &str) -> bool {
    match command {
        Command::Simple(simple) => simple.words.iter().any(|word| {
            let Some((name, _)) = word.split_once('=') else {
                return false;
            };
            name == variable_name && is_identifier(name)
        }),
        Command::Background(inner) => command_contains_assignment_target(inner, variable_name),
        Command::Connection(connection) => {
            command_contains_assignment_target(&connection.left, variable_name)
                || command_contains_assignment_target(&connection.right, variable_name)
        }
        Command::If(if_cmd) => {
            command_contains_assignment_target(&if_cmd.condition, variable_name)
                || function_contains_assignment_target(&if_cmd.then_body, variable_name)
                || if_cmd
                    .else_body
                    .as_ref()
                    .is_some_and(|body| function_contains_assignment_target(body, variable_name))
        }
        Command::While(while_cmd) => {
            command_contains_assignment_target(&while_cmd.condition, variable_name)
                || function_contains_assignment_target(&while_cmd.body, variable_name)
        }
        Command::For(for_cmd) => function_contains_assignment_target(&for_cmd.body, variable_name),
        Command::CStyleFor(for_cmd) => {
            function_contains_assignment_target(&for_cmd.body, variable_name)
        }
        Command::Case(case_cmd) => case_cmd
            .clauses
            .iter()
            .any(|clause| function_contains_assignment_target(&clause.body, variable_name)),
        Command::Function(function) => {
            function_contains_assignment_target(&function.body, variable_name)
        }
        Command::Group(body) => function_contains_assignment_target(body, variable_name),
        Command::Arithmetic(_) => false,
    }
}

fn function_captures_self_call_value(commands: &[Command], raw_function_name: &str) -> bool {
    commands
        .iter()
        .any(|command| command_contains_self_call(command, raw_function_name))
}

fn command_contains_self_call(command: &Command, raw_function_name: &str) -> bool {
    match command {
        Command::Simple(simple) => {
            // Only check for self-calls inside command substitutions $()
            // Direct calls like `foo arg` don't capture return values
            let merged = simple.words.join(" ");
            extract_command_substitutions(&merged).iter().any(|body| {
                // Check if the command substitution directly calls the function
                // $(func arg) means we're capturing the function's stdout as return value
                if let Ok(parsed) = parser::parse(body.trim(), None) {
                    parsed.statements.iter().any(|stmt| {
                        // Check if statement is a direct call to the function
                        if let Command::Simple(inner) = stmt {
                            if inner
                                .words
                                .first()
                                .is_some_and(|word| word == raw_function_name)
                            {
                                return true;
                            }
                        }
                        // Also recurse for nested command substitutions
                        command_contains_self_call(stmt, raw_function_name)
                    })
                } else {
                    body.split_whitespace()
                        .next()
                        .is_some_and(|name| name == raw_function_name)
                }
            })
        }
        Command::Arithmetic(_) => false,
        Command::Background(inner) => command_contains_self_call(inner, raw_function_name),
        Command::Connection(connection) => {
            command_contains_self_call(&connection.left, raw_function_name)
                || command_contains_self_call(&connection.right, raw_function_name)
        }
        Command::If(if_cmd) => {
            command_contains_self_call(&if_cmd.condition, raw_function_name)
                || function_captures_self_call_value(&if_cmd.then_body, raw_function_name)
                || if_cmd
                    .else_body
                    .as_ref()
                    .map(|body| function_captures_self_call_value(body, raw_function_name))
                    .unwrap_or(false)
        }
        Command::While(while_cmd) => {
            command_contains_self_call(&while_cmd.condition, raw_function_name)
                || function_captures_self_call_value(&while_cmd.body, raw_function_name)
        }
        Command::For(for_cmd) => function_captures_self_call_value(&for_cmd.body, raw_function_name),
        Command::CStyleFor(for_cmd) => {
            function_captures_self_call_value(&for_cmd.body, raw_function_name)
        }
        Command::Case(case_cmd) => case_cmd
            .clauses
            .iter()
            .any(|clause| function_captures_self_call_value(&clause.body, raw_function_name)),
        Command::Function(function) => {
            function_captures_self_call_value(&function.body, raw_function_name)
        }
        Command::Group(body) => function_captures_self_call_value(body, raw_function_name),
    }
}

fn extract_command_substitutions(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut substitutions = Vec::new();
    let mut i = 0usize;

    while i + 1 < chars.len() {
        if chars[i] == '$' && chars[i + 1] == '(' {
            // Skip arithmetic expansion $((...))
            if i + 2 < chars.len() && chars[i + 2] == '(' {
                i += 1;
                continue;
            }
            let start = i + 2;
            let Some(end) = find_command_substitution_end(&chars, start) else {
                break;
            };
            substitutions.push(chars[start..end].iter().collect());
            i = end + 1;
            continue;
        }

        i += 1;
    }

    substitutions
}

fn find_command_substitution_end(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut i = start;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while i < chars.len() {
        let ch = chars[i];

        if escaped {
            escaped = false;
            i += 1;
            continue;
        }

        if ch == '\\' && !in_single {
            escaped = true;
            i += 1;
            continue;
        }

        if ch == '\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }

        if ch == '"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }

        if !in_single && !in_double {
            if ch == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
                depth += 1;
                i += 2;
                continue;
            }

            if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
        }

        i += 1;
    }

    None
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

fn case_fallback(case_cmd: &CaseCommand, ctx: &RenderContext) -> FragmentKind {
    RawFragment {
        value: command_literal_from_command(&Command::Case(case_cmd.clone()), ctx),
    }
    .to_frag()
}

fn render_case(case_cmd: &CaseCommand, ctx: &mut RenderContext, return_tail: bool) -> FragmentKind {
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

fn render_simple(simple: &SimpleCommand, ctx: &mut RenderContext) -> FragmentKind {
    if let Some(assignment) = parse_assignment(simple, ctx) {
        return VarStmtFragment {
            name: assignment.name,
            value: Box::new(render_expression_fragment(&assignment.value)),
            is_reassignment: assignment.is_reassignment,
        }
        .to_frag();
    }

    if let Some(printf_v) = render_printf_v(simple, ctx) {
        return RawFragment { value: printf_v }.to_frag();
    }

    if let Some(call) = render_function_call_statement(simple, ctx) {
        return RawFragment { value: call }.to_frag();
    }

    if simple.words.first().is_some_and(|word| word == "echo") {
        if simple.words.len() == 1 {
            return RawFragment {
                value: "echo(\"\")".to_string(),
            }
            .to_frag();
        }

        if simple.words.len() == 2
            && let Some(expr) = word_to_expr(&simple.words[1], ctx)
        {
            return RawFragment {
                value: format!("echo({expr})"),
            }
            .to_frag();
        }

        if simple.words.len() > 2 {
            let merged = simple.words[1..].join(" ");
            let merged_trimmed = merged.trim();
            if merged_trimmed.starts_with("$((")
                && merged_trimmed.ends_with("))")
                && let Some(expr) = word_to_expr(merged_trimmed, ctx)
            {
                return RawFragment {
                    value: format!("echo({expr})"),
                }
                .to_frag();
            }
        }
    }

    RawFragment {
        value: command_literal_from_command(&Command::Simple(simple.clone()), ctx),
    }
    .to_frag()
}

fn render_expression_fragment(value: &str) -> FragmentKind {
    if let Some(parts) = split_plus_expression(value) {
        let mut items = Vec::new();
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                items.push(
                    RawFragment {
                        value: "+".to_string(),
                    }
                    .to_frag(),
                );
            }
            items.push(render_value_atom_fragment(&part));
        }
        return ListFragment { items }.to_frag();
    }

    let tokens = tokenize_expression_by_whitespace(value);
    if tokens.len() > 1 {
        let items = tokens
            .into_iter()
            .map(|token| render_expression_token_fragment(&token))
            .collect::<Vec<FragmentKind>>();
        return ListFragment { items }.to_frag();
    }

    render_value_atom_fragment(value)
}

fn render_value_atom_fragment(value: &str) -> FragmentKind {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return RawFragment {
            value: "\"\"".to_string(),
        }
        .to_frag();
    }

    if is_double_quoted(trimmed) {
        return parse_interpolable_literal(trimmed).to_frag();
    }

    if is_identifier(trimmed) {
        if is_expression_keyword(trimmed) {
            return RawFragment {
                value: trimmed.to_string(),
            }
            .to_frag();
        }
        return VarExprFragment {
            name: trimmed.to_string(),
        }
        .to_frag();
    }

    RawFragment {
        value: trimmed.to_string(),
    }
    .to_frag()
}

fn render_expression_token_fragment(token: &str) -> FragmentKind {
    if is_expression_operator(token) || is_expression_keyword(token) {
        return RawFragment {
            value: token.to_string(),
        }
        .to_frag();
    }

    render_value_atom_fragment(token)
}

fn tokenize_expression_by_whitespace(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escaped = true;
            continue;
        }

        if !in_double && ch == '\'' {
            in_single = !in_single;
            current.push(ch);
            continue;
        }

        if !in_single && ch == '"' {
            in_double = !in_double;
            current.push(ch);
            continue;
        }

        if !in_single && !in_double && ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.trim().to_string());
                current.clear();
            }
            continue;
        }

        current.push(ch);
    }

    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }

    tokens
}

fn is_expression_operator(token: &str) -> bool {
    matches!(
        token,
        "+" | "-" | "*" | "/" | "%" | "==" | "!=" | "<" | "<=" | ">" | ">=" | ".." | "..="
    )
}

fn is_expression_keyword(token: &str) -> bool {
    matches!(token, "and" | "or" | "then" | "else" | "not")
}

fn split_plus_expression(value: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escaped = true;
            continue;
        }

        if !in_double && ch == '\'' {
            in_single = !in_single;
            current.push(ch);
            continue;
        }

        if !in_single && ch == '"' {
            in_double = !in_double;
            current.push(ch);
            continue;
        }

        if !in_single && !in_double {
            match ch {
                '(' => paren += 1,
                ')' => paren = paren.saturating_sub(1),
                '[' => bracket += 1,
                ']' => bracket = bracket.saturating_sub(1),
                '{' => brace += 1,
                '}' => brace = brace.saturating_sub(1),
                '+' if paren == 0 && bracket == 0 && brace == 0 => {
                    let part = current.trim();
                    if part.is_empty() {
                        return None;
                    }
                    parts.push(part.to_string());
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }

        current.push(ch);
    }

    let tail = current.trim();
    if tail.is_empty() {
        return None;
    }
    parts.push(tail.to_string());

    (parts.len() > 1).then_some(parts)
}

fn parse_interpolable_literal(value: &str) -> InterpolableFragment {
    let mut inner = value;
    if is_double_quoted(value) {
        inner = &value[1..value.len() - 1];
    }

    let mut strings = Vec::new();
    let mut interpolations = Vec::new();
    let mut cursor = 0usize;

    while let Some(open_relative) = inner[cursor..].find('{') {
        let open = cursor + open_relative;
        let Some(close_relative) = inner[open + 1..].find('}') else {
            break;
        };
        let close = open + 1 + close_relative;

        strings.push(inner[cursor..open].to_string());
        interpolations.push(inner[open + 1..close].to_string());
        cursor = close + 1;
    }

    if interpolations.is_empty() {
        return InterpolableFragment {
            strings: vec![value.to_string()],
            interpolations: Vec::new(),
        };
    }

    strings.push(inner[cursor..].to_string());
    if let Some(first) = strings.first_mut() {
        first.insert(0, '"');
    }
    if let Some(last) = strings.last_mut() {
        last.push('"');
    }

    InterpolableFragment {
        strings,
        interpolations,
    }
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
    if sig.render_mode != FunctionRenderMode::Native {
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
