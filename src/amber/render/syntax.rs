use std::sync::LazyLock;

use crate::bash::ast::*;

use super::context::GlobalVarType;

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

/// Result of analyzing a function body for positional parameters
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FunctionTraits {
    /// Maximum positional parameter used ($1, $2, etc.)
    pub(super) arity: usize,
    /// Whether the function uses variadic parameters ($@ or $*)
    pub(super) is_variadic: bool,
}

impl FunctionTraits {
    fn merge(self, other: Self) -> Self {
        Self {
            arity: self.arity.max(other.arity),
            is_variadic: self.is_variadic || other.is_variadic,
        }
    }
}

/// Analyze function body for arity and variadic usage in a single pass
pub(super) fn detect_function_traits(commands: &[Command]) -> FunctionTraits {
    commands
        .iter()
        .map(detect_command_traits)
        .fold(FunctionTraits::default(), FunctionTraits::merge)
}

fn detect_command_traits(command: &Command) -> FunctionTraits {
    match command {
        Command::Simple(simple) => simple
            .words
            .iter()
            .map(|word| analyze_text(word))
            .fold(FunctionTraits::default(), FunctionTraits::merge),
        Command::Arithmetic(arith) => analyze_text(&arith.expression),
        Command::Background(inner) => detect_command_traits(inner),
        Command::Connection(connection) => {
            detect_command_traits(&connection.left).merge(detect_command_traits(&connection.right))
        }
        Command::If(if_cmd) => {
            let cond = detect_command_traits(&if_cmd.condition);
            let then_traits = detect_function_traits(&if_cmd.then_body);
            let else_traits = if_cmd
                .else_body
                .as_ref()
                .map(|body| detect_function_traits(body))
                .unwrap_or_default();
            cond.merge(then_traits).merge(else_traits)
        }
        Command::While(while_cmd) => {
            detect_command_traits(&while_cmd.condition).merge(detect_function_traits(&while_cmd.body))
        }
        Command::For(for_cmd) => detect_function_traits(&for_cmd.body),
        Command::CStyleFor(for_cmd) => {
            let init = analyze_text(&for_cmd.init);
            let cond = analyze_text(&for_cmd.condition);
            let update = analyze_text(&for_cmd.update);
            init.merge(cond).merge(update).merge(detect_function_traits(&for_cmd.body))
        }
        Command::Case(case_cmd) => {
            let subject = analyze_text(&case_cmd.word);
            let clauses = case_cmd
                .clauses
                .iter()
                .map(|clause| {
                    let pattern_traits = clause
                        .patterns
                        .iter()
                        .map(|pattern| analyze_text(pattern))
                        .fold(FunctionTraits::default(), FunctionTraits::merge);
                    pattern_traits.merge(detect_function_traits(&clause.body))
                })
                .fold(FunctionTraits::default(), FunctionTraits::merge);
            subject.merge(clauses)
        }
        Command::Function(function) => detect_function_traits(&function.body),
        Command::Group(body) => detect_function_traits(body),
    }
}

/// Analyze text for positional references and variadic usage
fn analyze_text(text: &str) -> FunctionTraits {
    FunctionTraits {
        arity: max_positional_reference(text),
        is_variadic: text.contains("$@") || text.contains("$*") || text.contains("${@") || text.contains("${*"),
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

pub(super) fn classify_assignment_rhs(value: &str) -> GlobalVarType {
    if value.is_empty() {
        return GlobalVarType::Text;
    }
    if is_number(value) {
        return GlobalVarType::Int;
    }
    if value.starts_with("$((") && value.ends_with("))") {
        return GlobalVarType::Int;
    }
    if is_double_quoted(value) {
        let inner = &value[1..value.len() - 1];
        // Pure variable ref like "$result" → Unknown
        if inner.starts_with('$')
            && !inner.contains(' ')
            && inner[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '{' || c == '}')
        {
            return GlobalVarType::Unknown;
        }
        return GlobalVarType::Text;
    }
    if is_single_quoted(value) {
        return GlobalVarType::Text;
    }
    if value.starts_with('$') {
        return GlobalVarType::Unknown;
    }
    GlobalVarType::Unknown
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
