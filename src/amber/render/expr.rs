use crate::bash::ast::*;
use crate::bash::parser;

use super::context::RenderContext;
use super::fallback::{command_literal_from_command, command_literal_from_shell};
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
    let (raw_name, first_value) = first_word.split_once('=')?;

    if !is_identifier(raw_name) {
        return None;
    }

    let is_reassignment = ctx.resolve_var(raw_name).is_some();

    if simple.words.len() > 1 {
        if !first_value.trim_start().starts_with('(') {
            let mut full_value = first_value.to_string();
            for word in simple.words.iter().skip(1) {
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
                name: ctx.declare_var(raw_name),
                value,
                is_reassignment,
            });
        }
        let mut full_value = first_value.to_string();
        for word in simple.words.iter().skip(1) {
            full_value.push(' ');
            full_value.push_str(word);
        }
        let items = parse_bash_array_items(&full_value, ctx)?;
        return Some(AssignmentRender {
            raw_name: raw_name.to_string(),
            name: ctx.declare_var(raw_name),
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
        name: ctx.declare_var(raw_name),
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
        Command::Arithmetic(arith) => render_arithmetic_condition_expr(&arith.expression, ctx),
        Command::Simple(simple) => parse_test_expression(&simple.words, ctx),
        _ => None,
    }
}

pub(super) fn render_arithmetic_statement_expr(
    expression: &str,
    ctx: &mut RenderContext,
) -> Option<String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rendered) = parse_arithmetic_assignment_statement(trimmed, ctx) {
        return Some(rendered);
    }

    if let Some(rendered) = parse_arithmetic_increment_statement(trimmed, ctx) {
        return Some(rendered);
    }

    parse_arithmetic_expression(trimmed, ctx)
}

pub(super) fn render_arithmetic_condition_expr(
    expression: &str,
    ctx: &RenderContext,
) -> Option<String> {
    let rendered = parse_arithmetic_expression(expression, ctx)?;
    if arithmetic_expression_is_boolean(&rendered) {
        return Some(rendered);
    }
    Some(format!("({rendered}) != 0"))
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

pub(super) fn word_to_expr(word: &str, ctx: &RenderContext) -> Option<String> {
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

fn parse_arithmetic_expansion(value: &str, ctx: &RenderContext) -> Option<String> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("$((")?.strip_suffix("))")?.trim();
    if inner.is_empty() {
        return None;
    }
    parse_arithmetic_expression(inner, ctx)
}

fn parse_arithmetic_expression(value: &str, ctx: &RenderContext) -> Option<String> {
    let tokens = parse_arithmetic_tokens(value, ctx)?;
    let tokens = convert_c_style_ternary_tokens(&tokens)?;
    if tokens.is_empty() {
        return None;
    }
    Some(tokens.join(" "))
}

fn parse_arithmetic_tokens(value: &str, ctx: &RenderContext) -> Option<Vec<String>> {
    let chars: Vec<char> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];

        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        if ch.is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            tokens.push(chars[i..j].iter().collect::<String>());
            i = j;
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let name: String = chars[i..j].iter().collect();
            if matches!(name.as_str(), "and" | "or" | "not") {
                tokens.push(name);
            } else {
                tokens.push(ctx.resolve_var(&name)?);
            }
            i = j;
            continue;
        }

        if ch == '$' {
            if i + 1 >= chars.len() {
                return None;
            }

            if chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                if j >= chars.len() {
                    return None;
                }

                let name: String = chars[i + 2..j].iter().collect();
                let resolved = if let Ok(index) = name.parse::<usize>() {
                    if index == 0 {
                        return None;
                    }
                    ctx.resolve_positional(index)
                } else {
                    ctx.resolve_var(&name)
                }?;
                tokens.push(resolved);
                i = j + 1;
                continue;
            }

            if chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_' {
                let mut j = i + 2;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                tokens.push(ctx.resolve_var(&name)?);
                i = j;
                continue;
            }

            if chars[i + 1].is_ascii_digit() {
                let mut j = i + 2;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let index: String = chars[i + 1..j].iter().collect();
                let index = index.parse::<usize>().ok()?;
                if index == 0 {
                    return None;
                }
                tokens.push(ctx.resolve_positional(index)?);
                i = j;
                continue;
            }

            return None;
        }

        let (operator, next) = parse_arithmetic_operator(&chars, i)?;
        let normalized = normalize_arithmetic_operator(operator)?;
        tokens.push(normalized);
        i = next;
    }

    Some(tokens)
}

fn parse_arithmetic_assignment_statement(
    expression: &str,
    ctx: &mut RenderContext,
) -> Option<String> {
    let (lhs_raw, op, rhs_raw) = parse_arithmetic_assignment_parts(expression)?;
    if !is_identifier(lhs_raw) {
        return None;
    }

    let rhs = parse_arithmetic_expression(rhs_raw, ctx)?;
    let existing = ctx.resolve_var(lhs_raw);

    if op == "=" {
        if let Some(name) = existing {
            return Some(format!("{name} = {rhs}"));
        }

        let name = ctx.declare_var(lhs_raw);
        return Some(format!("let {name} = {rhs}"));
    }

    let base = parse_compound_assignment_operator(op)?;
    let name = existing?;
    Some(format!("{name} = {name} {base} {rhs}"))
}

fn parse_arithmetic_increment_statement(
    expression: &str,
    ctx: &mut RenderContext,
) -> Option<String> {
    let compact = expression
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    if let Some(raw) = compact.strip_suffix("++")
        && is_identifier(raw)
    {
        let name = ctx.resolve_var(raw)?;
        return Some(format!("{name} = {name} + 1"));
    }

    if let Some(raw) = compact.strip_prefix("++")
        && is_identifier(raw)
    {
        let name = ctx.resolve_var(raw)?;
        return Some(format!("{name} = {name} + 1"));
    }

    if let Some(raw) = compact.strip_suffix("--")
        && is_identifier(raw)
    {
        let name = ctx.resolve_var(raw)?;
        return Some(format!("{name} = {name} - 1"));
    }

    if let Some(raw) = compact.strip_prefix("--")
        && is_identifier(raw)
    {
        let name = ctx.resolve_var(raw)?;
        return Some(format!("{name} = {name} - 1"));
    }

    None
}

fn parse_arithmetic_assignment_parts(expression: &str) -> Option<(&str, &str, &str)> {
    for op in ["+=", "-=", "*=", "/=", "%=", "="] {
        let Some((lhs, rhs)) = expression.split_once(op) else {
            continue;
        };

        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.is_empty() || rhs.is_empty() {
            continue;
        }

        if op == "=" {
            let lhs_tail = lhs.chars().last();
            if matches!(lhs_tail, Some('<' | '>' | '!' | '=')) {
                continue;
            }

            let rhs_head = rhs.chars().next();
            if matches!(rhs_head, Some('=')) {
                continue;
            }
        }

        return Some((lhs, op, rhs));
    }

    None
}

fn parse_compound_assignment_operator(op: &str) -> Option<&'static str> {
    match op {
        "+=" => Some("+"),
        "-=" => Some("-"),
        "*=" => Some("*"),
        "/=" => Some("/"),
        "%=" => Some("%"),
        _ => None,
    }
}

fn arithmetic_expression_is_boolean(expression: &str) -> bool {
    let tokens = expression.split_whitespace().collect::<Vec<&str>>();
    if tokens.iter().any(|token| matches!(*token, "then" | "else")) {
        return false;
    }

    expression.split_whitespace().any(|token| {
        matches!(
            token,
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or" | "not"
        )
    })
}

fn parse_arithmetic_operator(chars: &[char], start: usize) -> Option<(&'static str, usize)> {
    let remaining = chars.len().saturating_sub(start);
    if remaining >= 3 {
        let triple: String = chars[start..start + 3].iter().collect();
        if let Some(op) = match triple.as_str() {
            "<<=" => Some("<<="),
            ">>=" => Some(">>="),
            _ => None,
        } {
            return Some((op, start + 3));
        }
    }

    if remaining >= 2 {
        let pair: String = chars[start..start + 2].iter().collect();
        if let Some(op) = match pair.as_str() {
            "++" => Some("++"),
            "--" => Some("--"),
            "**" => Some("**"),
            "<<" => Some("<<"),
            ">>" => Some(">>"),
            "<=" => Some("<="),
            ">=" => Some(">="),
            "==" => Some("=="),
            "!=" => Some("!="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            "+=" => Some("+="),
            "-=" => Some("-="),
            "*=" => Some("*="),
            "/=" => Some("/="),
            "%=" => Some("%="),
            "&=" => Some("&="),
            "|=" => Some("|="),
            "^=" => Some("^="),
            _ => None,
        } {
            return Some((op, start + 2));
        }
    }

    match chars[start] {
        '+' => Some(("+", start + 1)),
        '-' => Some(("-", start + 1)),
        '*' => Some(("*", start + 1)),
        '/' => Some(("/", start + 1)),
        '%' => Some(("%", start + 1)),
        '(' => Some(("(", start + 1)),
        ')' => Some((")", start + 1)),
        '<' => Some(("<", start + 1)),
        '>' => Some((">", start + 1)),
        '=' => Some(("=", start + 1)),
        '!' => Some(("!", start + 1)),
        '&' => Some(("&", start + 1)),
        '|' => Some(("|", start + 1)),
        '^' => Some(("^", start + 1)),
        '~' => Some(("~", start + 1)),
        '?' => Some(("?", start + 1)),
        ':' => Some((":", start + 1)),
        ',' => Some((",", start + 1)),
        _ => None,
    }
}

fn normalize_arithmetic_operator(operator: &str) -> Option<String> {
    match operator {
        "+" | "-" | "*" | "/" | "%" | "(" | ")" | "<" | ">" | "<=" | ">=" | "==" | "!=" => {
            Some(operator.to_string())
        }
        "&&" => Some("and".to_string()),
        "||" => Some("or".to_string()),
        "!" => Some("not".to_string()),
        "?" | ":" => Some(operator.to_string()),
        _ => None,
    }
}

fn convert_c_style_ternary_tokens(tokens: &[String]) -> Option<Vec<String>> {
    if !tokens.iter().any(|token| token == "?") {
        return Some(tokens.to_vec());
    }

    let question = find_top_level_question(tokens)?;
    let colon = find_matching_colon(tokens, question)?;

    if question == 0 || colon <= question + 1 || colon + 1 >= tokens.len() {
        return None;
    }

    let condition = convert_c_style_ternary_tokens(&tokens[..question])?;
    let when_true = convert_c_style_ternary_tokens(&tokens[question + 1..colon])?;
    let when_false = convert_c_style_ternary_tokens(&tokens[colon + 1..])?;

    let mut result = Vec::new();
    result.extend(condition);
    result.push("then".to_string());
    result.extend(when_true);
    result.push("else".to_string());
    result.extend(when_false);
    Some(result)
}

fn find_top_level_question(tokens: &[String]) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "?" if depth == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn find_matching_colon(tokens: &[String], question_index: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut nested_questions = 0usize;

    for (index, token) in tokens.iter().enumerate().skip(question_index + 1) {
        match token.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "?" if depth == 0 => nested_questions += 1,
            ":" if depth == 0 => {
                if nested_questions == 0 {
                    return Some(index);
                }
                nested_questions -= 1;
            }
            _ => {}
        }
    }

    None
}

fn parse_if_command_substitution_expression(value: &str, ctx: &RenderContext) -> Option<String> {
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

fn parse_and_or_command_substitution_expression(
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

fn parse_function_call_command_substitution_expression(
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

fn parse_generic_command_substitution_expression(
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
        return Some(command_literal_from_command(
            parsed.statements.first()?,
            ctx,
        ));
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

pub(super) fn render_simple_function_call_expr(
    simple: &SimpleCommand,
    ctx: &RenderContext,
) -> Option<String> {
    let function_name = simple.words.first()?;
    let sig = ctx.resolve_function(function_name)?;
    let args = &simple.words[1..];

    if args.len() != sig.arity {
        return None;
    }

    let args = args
        .iter()
        .map(|arg| word_to_expr(arg, ctx))
        .collect::<Option<Vec<String>>>()?;

    Some(format!("{}({})", sig.amber_name, args.join(", ")))
}

fn contains_unresolved_shell_expansion(text: &str, ctx: &RenderContext) -> bool {
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

fn render_double_quoted(word: &str, ctx: &RenderContext) -> String {
    let inner = &word[1..word.len() - 1];
    format!("\"{}\"", render_interpolated_text(inner, ctx))
}

fn render_single_quoted(word: &str) -> String {
    let inner = &word[1..word.len() - 1];
    format!("\"{}\"", escape_plain_text(inner))
}

fn render_interpolated_text(text: &str, ctx: &RenderContext) -> String {
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

fn escape_plain_text(text: &str) -> String {
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
