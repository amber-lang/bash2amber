use crate::amber::fragments::{
    InterpolableFragment, ListFragment, RawFragment, VarExprFragment, FragmentKind,
    FragmentRenderable,
};

use super::syntax::{is_double_quoted, is_identifier};

pub(super) fn render_expression_fragment(value: &str) -> FragmentKind {
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

pub(super) fn parse_interpolable_literal(value: &str) -> InterpolableFragment {
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
