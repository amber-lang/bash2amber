use super::context::RenderContext;
use super::syntax::is_identifier;

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

pub(super) fn parse_arithmetic_expansion(value: &str, ctx: &RenderContext) -> Option<String> {
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
