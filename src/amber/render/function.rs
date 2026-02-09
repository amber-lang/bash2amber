use std::collections::{HashMap, HashSet};

use crate::bash::ast::*;
use crate::bash::parser;

use crate::amber::fragments::{BlockFragment, FragmentKind, FragmentRenderable, FunctionFragment, RawFragment};

use super::analysis::{collect_non_local_assignments, commands_reference_variable};
use super::context::{
    FunctionRenderMode, FunctionSig, FunctionTypeHint, GlobalVarType, RenderContext,
    TypeCommentReturnContract,
};
use super::fallback::command_literal_from_command;
use super::syntax::{
    detect_function_traits, is_double_quoted, is_identifier, is_reserved_keyword,
    normalize_identifier, sanitize_function_name,
};
pub(super) fn preregister_function_signatures(commands: &[Command], ctx: &mut RenderContext) {
    for (index, command) in commands.iter().enumerate() {
        let Command::Function(function) = command else {
            continue;
        };

        let amber_name = sanitize_function_name(&function.name);
        let traits = detect_function_traits(&function.body);
        let arity = traits.arity;
        let positional_bindings = infer_positional_bindings_prefix(&function.body, arity);
        let function_body = &function.body[positional_bindings.len()..];
        let hint = ctx.next_function_hint(&function.name);

        let mut sig = FunctionSig {
            amber_name,
            arity,
            returns_value: false,
            // Variadic functions use ShallowFallback (native call site, body may have trust $)
            render_mode: if traits.is_variadic {
                FunctionRenderMode::ShallowFallback
            } else {
                FunctionRenderMode::Native
            },
            typed_signature: None,
            global_vars: Vec::new(),
        };

        // Check if function has a type comment - if so, use it
        if let Some(hint) = hint.as_ref() {
            if let FunctionTypeHint::Typed(type_signature) = &hint.type_hint {
                if type_signature.params.len() == arity {
                    match &type_signature.return_contract {
                        TypeCommentReturnContract::TypedVariable { variable_name, .. } => {
                            if function_contains_assignment_target(function_body, variable_name) {
                                sig.returns_value = true;
                                sig.typed_signature = Some(type_signature.clone());
                                ctx.register_function(&function.name, sig);
                                continue;
                            }
                        }
                        TypeCommentReturnContract::Null => {
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
            // Needs type comment but doesn't have one - full fallback
            sig.render_mode = FunctionRenderMode::FullFallback;
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
            ctx.register_function(&function.name, sig);
            continue;
        }

        // For Native non-returning functions, detect global output variables
        if sig.render_mode == FunctionRenderMode::Native && !sig.returns_value {
            let binding_names: HashSet<String> = positional_bindings
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            let mut non_local: HashMap<String, Option<GlobalVarType>> = HashMap::new();
            collect_non_local_assignments(function_body, &mut non_local);
            // Remove positional binding names - those are parameter aliases, not globals
            for name in &binding_names {
                non_local.remove(name);
            }
            // Filter to variables actually referenced after this function definition
            let rest = &commands[index + 1..];
            let referenced: Vec<(String, Option<GlobalVarType>)> = non_local
                .into_iter()
                .filter(|(var, _)| commands_reference_variable(rest, var))
                .collect();

            // Check for type conflicts (Int vs Text in different branches)
            if referenced.iter().any(|(_, ty)| ty.is_none()) {
                let line = hint.as_ref().map(|item| item.function_line).unwrap_or(0);
                let reason = "global output variable has conflicting types (Int vs Text) across branches";
                if line > 0 {
                    ctx.warn(
                        &format!(
                            "Function '{}' at line {line} has conflicting global variable types. Falling back to trust literal.",
                            function.name
                        ),
                        Some(reason),
                    );
                } else {
                    ctx.warn(
                        &format!(
                            "Function '{}' has conflicting global variable types. Falling back to trust literal.",
                            function.name
                        ),
                        Some(reason),
                    );
                }
                sig.render_mode = FunctionRenderMode::FullFallback;
                ctx.register_function(&function.name, sig);
                continue;
            }

            let mut globals: Vec<(String, GlobalVarType)> = referenced
                .into_iter()
                .map(|(name, ty)| (name, ty.unwrap_or(GlobalVarType::Unknown)))
                .collect();
            globals.sort_by(|a, b| a.0.cmp(&b.0));
            sig.global_vars = globals;
        }

        ctx.register_function(&function.name, sig);
    }
}

pub(super) fn render_function(
    function: &FunctionDef,
    ctx: &mut RenderContext,
    render_commands: fn(&[Command], &mut RenderContext, bool) -> Vec<FragmentKind>,
) -> FragmentKind {
    let sig = ctx.resolve_function(&function.name).cloned();
    if sig
        .as_ref()
        .is_some_and(|sig| sig.render_mode == FunctionRenderMode::FullFallback)
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
        .unwrap_or_else(|| detect_function_traits(&function.body).arity);
    let positional_bindings = infer_positional_bindings_prefix(&function.body, arity);
    let mut params = build_function_params(arity, &positional_bindings);
    let mut typed_params = None::<Vec<String>>;
    let mut return_type = None::<String>;
    let mut return_var_name = None::<String>;
    let mut return_tail = false;

    if let Some(type_comment) = sig.as_ref().and_then(|s| s.typed_signature.as_ref()) {
        let mut comment_names = Vec::new();
        let mut rendered_typed = Vec::new();

        for (index, param) in type_comment.params.iter().enumerate() {
            let name = if let Some(n) = &param.name {
                sanitize_param_name(n)
            } else {
                // Unnamed param: use positional binding name or default arg{N}
                positional_bindings
                    .iter()
                    .find(|(_, idx)| *idx == index + 1)
                    .map(|(name, _)| sanitize_param_name(name))
                    .unwrap_or_else(|| format!("arg{}", index + 1))
            };
            let name = push_unique_param_name(&mut comment_names, name);
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
            return_tail = true;
        }
    } else if sig.as_ref().is_some_and(|s| s.returns_value) {
        return_tail = true;
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

pub(super) fn infer_positional_bindings_prefix(commands: &[Command], arity: usize) -> Vec<(String, usize)> {
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

pub(super) fn function_contains_assignment_target(commands: &[Command], variable_name: &str) -> bool {
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
