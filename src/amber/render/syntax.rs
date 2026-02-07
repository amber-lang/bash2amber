use std::sync::LazyLock;

use crate::bash::ast::*;

pub(super) fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(super) fn normalize_identifier(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    if out.is_empty() {
        return "var".to_string();
    }

    if out
        .chars()
        .next()
        .is_some_and(|ch| !(ch.is_ascii_alphabetic() || ch == '_'))
    {
        out.insert(0, '_');
    }

    out
}

pub(super) fn sanitize_function_name(name: &str) -> String {
    let mut out = normalize_identifier(name);
    if is_reserved_keyword(&out) {
        out.push_str("_fn");
    }
    out
}

pub(super) fn is_number(text: &str) -> bool {
    text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok()
}

pub(super) fn is_double_quoted(word: &str) -> bool {
    word.len() >= 2 && word.starts_with('"') && word.ends_with('"')
}

pub(super) fn is_single_quoted(word: &str) -> bool {
    word.len() >= 2 && word.starts_with('\'') && word.ends_with('\'')
}

pub(super) fn is_reserved_keyword(word: &str) -> bool {
    RESERVED_KEYWORDS.contains(&word)
}

pub(super) fn detect_function_arity(commands: &[Command]) -> usize {
    commands.iter().map(detect_command_arity).max().unwrap_or(0)
}

fn detect_command_arity(command: &Command) -> usize {
    match command {
        Command::Simple(simple) => simple
            .words
            .iter()
            .map(|word| max_positional_reference(word))
            .max()
            .unwrap_or(0),
        Command::Arithmetic(arith) => max_positional_reference(&arith.expression),
        Command::Background(inner) => detect_command_arity(inner),
        Command::Connection(connection) => {
            detect_command_arity(&connection.left).max(detect_command_arity(&connection.right))
        }
        Command::If(if_cmd) => {
            let cond = detect_command_arity(&if_cmd.condition);
            let then_max = detect_function_arity(&if_cmd.then_body);
            let else_max = if_cmd
                .else_body
                .as_ref()
                .map(|body| detect_function_arity(body))
                .unwrap_or(0);
            cond.max(then_max).max(else_max)
        }
        Command::While(while_cmd) => {
            detect_command_arity(&while_cmd.condition).max(detect_function_arity(&while_cmd.body))
        }
        Command::For(for_cmd) => detect_function_arity(&for_cmd.body),
        Command::CStyleFor(for_cmd) => {
            let init = max_positional_reference(&for_cmd.init);
            let cond = max_positional_reference(&for_cmd.condition);
            let update = max_positional_reference(&for_cmd.update);
            init.max(cond)
                .max(update)
                .max(detect_function_arity(&for_cmd.body))
        }
        Command::Case(case_cmd) => {
            let subject = max_positional_reference(&case_cmd.word);
            let clauses = case_cmd
                .clauses
                .iter()
                .map(|clause| {
                    let pattern_max = clause
                        .patterns
                        .iter()
                        .map(|pattern| max_positional_reference(pattern))
                        .max()
                        .unwrap_or(0);
                    pattern_max.max(detect_function_arity(&clause.body))
                })
                .max()
                .unwrap_or(0);
            subject.max(clauses)
        }
        Command::Function(function) => detect_function_arity(&function.body),
        Command::Group(body) => detect_function_arity(body),
    }
}

fn max_positional_reference(text: &str) -> usize {
    let raw = strip_outer_double_quotes(text);
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    let mut max_found = 0usize;

    while i < chars.len() {
        if chars[i] != '$' || i + 1 >= chars.len() {
            i += 1;
            continue;
        }

        if chars[i + 1] == '{' {
            let mut j = i + 2;
            while j < chars.len() && chars[j] != '}' {
                j += 1;
            }
            if j < chars.len() {
                let value: String = chars[i + 2..j].iter().collect();
                if let Ok(index) = value.parse::<usize>() {
                    max_found = max_found.max(index);
                }
                i = j + 1;
                continue;
            }
            break;
        }

        if chars[i + 1].is_ascii_digit() {
            let mut j = i + 2;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let value: String = chars[i + 1..j].iter().collect();
            if let Ok(index) = value.parse::<usize>() {
                max_found = max_found.max(index);
            }
            i = j;
            continue;
        }

        i += 1;
    }

    max_found
}

pub(super) fn strip_outer_double_quotes(word: &str) -> &str {
    if is_double_quoted(word) {
        &word[1..word.len() - 1]
    } else {
        word
    }
}

static RESERVED_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "Bool",
        "Null",
        "Number",
        "Text",
        "and",
        "as",
        "break",
        "cd",
        "const",
        "continue",
        "echo",
        "else",
        "exit",
        "exited",
        "fail",
        "failed",
        "false",
        "for",
        "from",
        "fun",
        "if",
        "import",
        "in",
        "is",
        "len",
        "let",
        "lines",
        "loop",
        "main",
        "mv",
        "nameof",
        "touch",
        "not",
        "null",
        "or",
        "pub",
        "ref",
        "return",
        "silent",
        "sleep",
        "status",
        "sudo",
        "succeeded",
        "suppress",
        "then",
        "trust",
        "true",
        "unsafe",
        "while",
    ]
});
