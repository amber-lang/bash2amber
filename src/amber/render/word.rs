use crate::bash::ast::*;

use super::arithmetic::parse_arithmetic_expansion;
use super::context::{FunctionRenderMode, RenderContext};
use super::substitution::{
    parse_and_or_command_substitution_expression,
    parse_function_call_command_substitution_expression,
    parse_generic_command_substitution_expression, parse_if_command_substitution_expression,
};
use super::syntax::{
    is_double_quoted, is_identifier, is_number, is_single_quoted, strip_outer_double_quotes,
};

pub(super) struct AssignmentRender {
    pub(super) raw_name: String,
    pub(super) name: String,
    pub(super) value: String,
    pub(super) is_reassignment: bool,
}

pub(super) fn parse_assignment(
    simple: &SimpleCommand,
    ctx: &mut RenderContext,
) -> Option<AssignmentRender> {
    let first_word = simple.words.first()?;
    let (is_local, assignment_index) = if first_word == "local" {
        if simple.words.len() < 2 {
            return None;
        }
        (true, 1usize)
    } else {
        (false, 0usize)
    };

    let assignment_word = simple.words.get(assignment_index)?;
    let (raw_name, first_value) = assignment_word.split_once('=')?;

    if !is_identifier(raw_name) {
        return None;
    }

    let is_reassignment = !is_local && ctx.resolve_var(raw_name).is_some();
    let declare = |ctx: &mut RenderContext, raw_name: &str| -> String {
        if is_local {
            ctx.declare_local_var(raw_name)
        } else {
            ctx.declare_var(raw_name)
        }
    };

    if simple.words.len() > assignment_index + 1 {
        if !first_value.trim_start().starts_with('(') {
            let mut full_value = first_value.to_string();
            for word in simple.words.iter().skip(assignment_index + 1) {
                full_value.push(' ');
                full_value.push_str(word);
            }
            let value = parse_arithmetic_expansion(&full_value, ctx)
                .or_else(|| parse_if_command_substitution_expression(&full_value, ctx))
                .or_else(|| parse_and_or_command_substitution_expression(&full_value, ctx))
                .or_else(|| parse_function_call_command_substitution_expression(&full_value, ctx))
                .or_else(|| parse_generic_command_substitution_expression(&full_value, ctx))?;
            return Some(AssignmentRender {
                raw_name: raw_name.to_string(),
                name: declare(ctx, raw_name),
                value,
                is_reassignment,
            });
        }
        let mut full_value = first_value.to_string();
        for word in simple.words.iter().skip(assignment_index + 1) {
            full_value.push(' ');
            full_value.push_str(word);
        }
        let items = parse_bash_array_items(&full_value, ctx)?;
        return Some(AssignmentRender {
            raw_name: raw_name.to_string(),
            name: declare(ctx, raw_name),
            value: format!("[{}]", items.join(", ")),
            is_reassignment,
        });
    }

    let value = first_value;

    let rendered = if let Some(expression) = parse_arithmetic_expansion(value, ctx) {
        expression
    } else if let Some(expression) = parse_if_command_substitution_expression(value, ctx) {
        expression
    } else if let Some(expression) = parse_and_or_command_substitution_expression(value, ctx) {
        expression
    } else if let Some(expression) = parse_function_call_command_substitution_expression(value, ctx)
    {
        expression
    } else if let Some(expression) = parse_generic_command_substitution_expression(value, ctx) {
        expression
    } else if has_unresolved_shell_var(value, ctx) {
        return None;
    } else if value.is_empty() {
        "\"\"".to_string()
    } else if let Some(var) = parse_variable_reference(value, ctx) {
        var
    } else if is_number(value) {
        value.to_string()
    } else if is_double_quoted(value) {
        render_double_quoted(value, ctx)
    } else if is_single_quoted(value) {
        render_single_quoted(value)
    } else {
        format!("\"{}\"", render_interpolated_text(value, ctx))
    };

    Some(AssignmentRender {
        raw_name: raw_name.to_string(),
        name: declare(ctx, raw_name),
        value: rendered,
        is_reassignment,
    })
}

pub(super) fn render_condition_expr(command: &Command, ctx: &RenderContext) -> Option<String> {
    match command {
        Command::Connection(connection) => match connection.op {
            Connector::And => Some(format!(
                "({}) and ({})",
                render_condition_expr(&connection.left, ctx)?,
                render_condition_expr(&connection.right, ctx)?
            )),
            Connector::Or => Some(format!(
                "({}) or ({})",
                render_condition_expr(&connection.left, ctx)?,
                render_condition_expr(&connection.right, ctx)?
            )),
            Connector::Pipe => None,
        },
        Command::Arithmetic(arith) => {
            super::arithmetic::render_arithmetic_condition_expr(&arith.expression, ctx)
        }
        Command::Simple(simple) => parse_test_expression(&simple.words, ctx),
        _ => None,
    }
}

pub(super) fn render_for_items(items: &[String], ctx: &RenderContext) -> Option<String> {
    if items.is_empty() {
        return Some("[]".to_string());
    }

    if items.len() == 1 {
        if let Some(var) = parse_array_iteration(items[0].as_str(), ctx) {
            return Some(var);
        }

        if let Some(var) = parse_variable_reference(items[0].as_str(), ctx) {
            return Some(var);
        }

        if has_unresolved_shell_var(items[0].as_str(), ctx) {
            return None;
        }
    }

    let entries = items
        .iter()
        .map(|item| word_to_expr(item, ctx))
        .collect::<Option<Vec<String>>>()?
        .join(", ");
    Some(format!("[{entries}]"))
}

pub(crate) fn word_to_expr(word: &str, ctx: &RenderContext) -> Option<String> {
    if let Some(var) = parse_variable_reference(word, ctx) {
        return Some(var);
    }

    if let Some(expr) = parse_arithmetic_expansion(word, ctx) {
        return Some(expr);
    }

    if has_unresolved_shell_var(word, ctx) {
        return None;
    }

    if is_double_quoted(word) {
        return Some(render_double_quoted(word, ctx));
    }

    if is_single_quoted(word) {
        return Some(render_single_quoted(word));
    }

    if is_number(word) {
        return Some(word.to_string());
    }

    Some(format!("\"{}\"", render_interpolated_text(word, ctx)))
}

pub(super) fn parse_variable_reference(word: &str, ctx: &RenderContext) -> Option<String> {
    let raw = strip_outer_double_quotes(word);

    if let Some(inner) = raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        if let Ok(index) = inner.parse::<usize>() {
            if index > 0 {
                return ctx.resolve_positional(index);
            }
            return None;
        }
        if is_identifier(inner) {
            return ctx.resolve_var(inner);
        }
        return None;
    }

    if let Some(inner) = raw.strip_prefix('$') {
        if is_identifier(inner) {
            return ctx.resolve_var(inner);
        }
        if let Ok(index) = inner.parse::<usize>() {
            if index > 0 {
                return ctx.resolve_positional(index);
            }
            return None;
        }
    }

    None
}

pub(super) fn has_unresolved_shell_var(word: &str, ctx: &RenderContext) -> bool {
    let raw = strip_outer_double_quotes(word);
    contains_unresolved_shell_expansion(raw, ctx)
}

fn parse_test_expression(words: &[String], ctx: &RenderContext) -> Option<String> {
    if words.len() < 3 {
        return None;
    }

    let is_bracket = words.first()? == "[" && words.last()? == "]";
    let is_double_bracket = words.first()? == "[[" && words.last()? == "]]";
    if !(is_bracket || is_double_bracket) {
        return None;
    }

    let inner = &words[1..words.len() - 1];
    if inner.len() == 3 {
        if has_unresolved_shell_var(&inner[0], ctx) || has_unresolved_shell_var(&inner[2], ctx) {
            return None;
        }

        let lhs = word_to_expr(&inner[0], ctx)?;
        let rhs = word_to_expr(&inner[2], ctx)?;
        let op = match inner[1].as_str() {
            "=" | "==" => "==",
            "!=" => "!=",
            "-eq" => "==",
            "-ne" => "!=",
            "-gt" => ">",
            "-lt" => "<",
            "-ge" => ">=",
            "-le" => "<=",
            _ => return None,
        };
        return Some(format!("{lhs} {op} {rhs}"));
    }

    if inner.len() == 2 {
        match inner[0].as_str() {
            "-z" => {
                let expr = word_to_expr(&inner[1], ctx)?;
                return Some(format!("len({expr}) == 0"));
            }
            "-n" => {
                let expr = word_to_expr(&inner[1], ctx)?;
                return Some(format!("len({expr}) > 0"));
            }
            _ => {}
        }
    }

    None
}

fn parse_array_iteration(word: &str, ctx: &RenderContext) -> Option<String> {
    let raw = strip_outer_double_quotes(word);
    let inside = raw.strip_prefix("${")?.strip_suffix('}')?;

    if let Some(name) = inside.strip_suffix("[@]") {
        return ctx.resolve_var(name);
    }

    if let Some(name) = inside.strip_suffix("[*]") {
        return ctx.resolve_var(name);
    }

    None
}

fn parse_bash_array_items(value: &str, ctx: &RenderContext) -> Option<Vec<String>> {
    let trimmed = value.trim();
    if !(trimmed.starts_with('(') && trimmed.ends_with(')')) {
        return None;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let tokens = tokenize_bash_array_items(inner);
    let mut rendered = Vec::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        rendered.push(word_to_expr(token, ctx)?);
    }
    Some(rendered)
}

fn tokenize_bash_array_items(inner: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in inner.chars() {
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

        if ch == '\'' && !in_double {
            in_single = !in_single;
            current.push(ch);
            continue;
        }

        if ch == '"' && !in_single {
            in_double = !in_double;
            current.push(ch);
            continue;
        }

        if !in_single && !in_double && (ch == ',' || ch.is_whitespace()) {
            if !current.trim().is_empty() {
                result.push(current.trim().to_string());
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

pub(super) fn contains_unresolved_shell_expansion(text: &str, ctx: &RenderContext) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }

        if i + 1 >= chars.len() {
            return true;
        }

        // Handle arithmetic expansion $((…))
        if i + 2 < chars.len() && chars[i + 1] == '(' && chars[i + 2] == '(' {
            // Find matching ))
            let start = i + 3;
            let mut depth = 1;
            let mut j = start;
            while j + 1 < chars.len() && depth > 0 {
                if chars[j] == '(' && chars[j + 1] == '(' {
                    depth += 1;
                    j += 2;
                } else if chars[j] == ')' && chars[j + 1] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    j += 2;
                } else {
                    j += 1;
                }
            }
            if depth != 0 {
                return true;
            }
            // Recursively check the contents of the arithmetic expansion
            let inner: String = chars[start..j].iter().collect();
            if contains_unresolved_shell_expansion(&inner, ctx) {
                return true;
            }
            i = j + 2;
            continue;
        }

        if chars[i + 1] == '{' {
            let mut j = i + 2;
            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }
            if j >= chars.len() {
                return true;
            }
            let name: String = chars[i + 2..j].iter().collect();
            let resolved = if let Ok(index) = name.parse::<usize>() {
                index > 0 && ctx.resolve_positional(index).is_some()
            } else if is_identifier(&name) {
                ctx.resolve_var(&name).is_some()
            } else {
                false
            };
            if !resolved {
                return true;
            }
            i = j + 1;
            continue;
        }

        let next = chars[i + 1];
        if next.is_ascii_alphabetic() || next == '_' {
            let mut j = i + 2;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let name: String = chars[i + 1..j].iter().collect();
            if ctx.resolve_var(&name).is_none() {
                return true;
            }
            i = j;
            continue;
        }

        if next.is_ascii_digit() {
            let mut j = i + 2;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let index_str: String = chars[i + 1..j].iter().collect();
            let Ok(index) = index_str.parse::<usize>() else {
                return true;
            };
            if index == 0 || ctx.resolve_positional(index).is_none() {
                return true;
            }
            i = j;
            continue;
        }

        return true;
    }
    false
}

pub(super) fn render_double_quoted(word: &str, ctx: &RenderContext) -> String {
    let inner = &word[1..word.len() - 1];
    format!("\"{}\"", render_interpolated_text(inner, ctx))
}

pub(super) fn render_single_quoted(word: &str) -> String {
    let inner = &word[1..word.len() - 1];
    format!("\"{}\"", escape_plain_text(inner))
}

pub(super) fn render_interpolated_text(text: &str, ctx: &RenderContext) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '$' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                if j < chars.len() {
                    let name: String = chars[i + 2..j].iter().collect();
                    let resolved = if let Ok(index) = name.parse::<usize>() {
                        ctx.resolve_positional(index)
                    } else {
                        ctx.resolve_var(&name)
                    };
                    if let Some(alias) = resolved {
                        out.push('{');
                        out.push_str(&alias);
                        out.push('}');
                        i = j + 1;
                        continue;
                    }
                }
            } else {
                let mut j = i + 1;
                if j < chars.len() && (chars[j].is_ascii_alphabetic() || chars[j] == '_') {
                    j += 1;
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    let name: String = chars[i + 1..j].iter().collect();
                    if let Some(alias) = ctx.resolve_var(&name) {
                        out.push('{');
                        out.push_str(&alias);
                        out.push('}');
                        i = j;
                        continue;
                    }
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    let index_str: String = chars[i + 1..j].iter().collect();
                    if let Ok(index) = index_str.parse::<usize>() {
                        if let Some(alias) = ctx.resolve_positional(index) {
                            out.push('{');
                            out.push_str(&alias);
                            out.push('}');
                            i = j;
                            continue;
                        }
                    }
                }
            }
        }

        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '{' | '}' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }

        i += 1;
    }

    out
}

pub(super) fn escape_plain_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '{' | '}' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn render_simple_function_call_expr(
    simple: &SimpleCommand,
    ctx: &RenderContext,
) -> Option<String> {
    let function_name = simple.words.first()?;
    let sig = ctx.resolve_function(function_name)?;
    // Only FullFallback prevents native call site rendering
    if sig.render_mode == FunctionRenderMode::FullFallback {
        return None;
    }
    let raw_args = &simple.words[1..];

    // Group words that form single expressions (e.g., "$((", "$1", "-", "1", "))" → "$((  $1 - 1 ))")
    let grouped_args = group_function_call_args(raw_args);

    // Always require exact arity match - Amber validates function call arguments
    if grouped_args.len() != sig.arity {
        return None;
    }

    let args = grouped_args
        .iter()
        .map(|arg| word_to_expr(arg, ctx))
        .collect::<Option<Vec<String>>>()?;

    Some(format!("{}({})", sig.amber_name, args.join(", ")))
}

/// Groups words that form arithmetic expansions $((…)) into single arguments.
fn group_function_call_args(words: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let word = &words[i];

        if word.starts_with("$((") {
            if word.ends_with("))") && count_arith_depth(word) == 0 {
                result.push(word.clone());
                i += 1;
                continue;
            }

            let mut combined = word.clone();
            let mut depth = count_arith_depth(&combined);
            i += 1;

            while i < words.len() && depth > 0 {
                combined.push(' ');
                combined.push_str(&words[i]);
                depth = count_arith_depth(&combined);
                i += 1;
            }

            result.push(combined);
            continue;
        }

        result.push(word.clone());
        i += 1;
    }

    result
}

fn count_arith_depth(text: &str) -> i32 {
    let mut depth = 0i32;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' && chars[i + 2] == '(' {
            depth += 1;
            i += 3;
        } else if i + 1 < chars.len() && chars[i] == ')' && chars[i + 1] == ')' {
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    depth
}
