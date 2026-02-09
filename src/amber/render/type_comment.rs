use std::collections::HashMap;

use super::context::{FunctionHint, FunctionTypeHint, TypeCommentParam, TypeCommentReturnContract, TypeCommentSignature};
use super::syntax::is_identifier;

pub(super) fn collect_function_hints(source: &str) -> HashMap<String, Vec<FunctionHint>> {
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

        return parse_type_comment(trimmed);
    }

    FunctionTypeHint::Missing
}

fn parse_type_comment(comment_line_text: &str) -> FunctionTypeHint {
    let raw = comment_line_text
        .trim()
        .strip_prefix("##")
        .unwrap_or(comment_line_text.trim())
        .trim();
    let Some(signature) = parse_type_comment_signature(raw) else {
        return FunctionTypeHint::Invalid;
    };
    FunctionTypeHint::Typed(signature)
}

fn parse_type_comment_signature(raw: &str) -> Option<TypeCommentSignature> {
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
            if let Some((name, type_name)) = part.split_once(':') {
                // Named param: `name: Type`
                let name = name.trim();
                let type_name = type_name.trim();
                if !is_identifier(name) || !is_valid_type_hint(type_name) {
                    return None;
                }
                params.push(TypeCommentParam {
                    name: Some(name.to_string()),
                    type_name: type_name.to_string(),
                });
            } else if is_valid_type_hint(part) {
                // Unnamed param: just `Type`
                params.push(TypeCommentParam {
                    name: None,
                    type_name: part.to_string(),
                });
            } else {
                return None;
            }
        }
    }

    Some(TypeCommentSignature {
        params,
        return_contract,
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

    if !is_valid_type_hint(type_name) || !is_identifier(variable_name) {
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
