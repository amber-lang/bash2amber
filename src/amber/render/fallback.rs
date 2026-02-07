use crate::bash::ast::*;

use super::context::RenderContext;

pub(super) fn command_literal_from_command(command: &Command, ctx: &RenderContext) -> String {
    let shell = render_shell_command_with_nameof(command, ctx);
    command_literal_from_shell(shell.trim())
}

pub(super) fn command_literal_from_shell(shell: &str) -> String {
    let escaped = escape_for_command_literal(shell);
    let expanded = expand_nameof_placeholders(&escaped);
    format!("trust $ {expanded} $")
}

fn escape_for_command_literal(shell: &str) -> String {
    shell
        .replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('{', "\\{")
}

fn render_shell_command_with_nameof(command: &Command, ctx: &RenderContext) -> String {
    match command {
        Command::Simple(simple) => simple
            .words
            .iter()
            .map(|word| rewrite_shell_word(word, ctx))
            .collect::<Vec<String>>()
            .join(" "),
        Command::Background(inner) => {
            format!("{} &", render_shell_command_with_nameof(inner, ctx))
        }
        Command::Arithmetic(arith) => {
            format!(
                "(( {} ))",
                rewrite_arithmetic_expression(&arith.expression, ctx)
            )
        }
        Command::Connection(connection) => {
            let left = render_shell_command_with_nameof(&connection.left, ctx);
            let right = render_shell_command_with_nameof(&connection.right, ctx);
            let op = match connection.op {
                Connector::Pipe => "|",
                Connector::And => "&&",
                Connector::Or => "||",
            };
            format!("{left} {op} {right}")
        }
        Command::If(if_cmd) => {
            let condition = render_shell_command_with_nameof(&if_cmd.condition, ctx);
            let then_body = render_shell_block_with_nameof(&if_cmd.then_body, ctx);
            if let Some(else_body) = &if_cmd.else_body {
                format!(
                    "if {condition}; then {then_body}; else {}; fi",
                    render_shell_block_with_nameof(else_body, ctx)
                )
            } else {
                format!("if {condition}; then {then_body}; fi")
            }
        }
        Command::While(while_cmd) => {
            let condition = render_shell_command_with_nameof(&while_cmd.condition, ctx);
            let body = render_shell_block_with_nameof(&while_cmd.body, ctx);
            format!("while {condition}; do {body}; done")
        }
        Command::For(for_cmd) => {
            let items = for_cmd
                .items
                .iter()
                .map(|word| rewrite_shell_word(word, ctx))
                .collect::<Vec<String>>()
                .join(" ");
            let body = render_shell_block_with_nameof(&for_cmd.body, ctx);
            format!("for {} in {items}; do {body}; done", for_cmd.variable)
        }
        Command::CStyleFor(for_cmd) => {
            let body = render_shell_block_with_nameof(&for_cmd.body, ctx);
            format!(
                "for (( {}; {}; {} )); do {body}; done",
                for_cmd.init, for_cmd.condition, for_cmd.update
            )
        }
        Command::Case(case_cmd) => {
            let clauses = case_cmd
                .clauses
                .iter()
                .map(|clause| {
                    let patterns = clause
                        .patterns
                        .iter()
                        .map(|word| rewrite_shell_word(word, ctx))
                        .collect::<Vec<String>>()
                        .join("|");
                    let body = render_shell_block_with_nameof(&clause.body, ctx);
                    let suffix = match clause.terminator {
                        CaseClauseTerminator::Break => " ;;",
                        CaseClauseTerminator::Fallthrough => " ;&",
                        CaseClauseTerminator::TestNext => " ;;&",
                        CaseClauseTerminator::End => "",
                    };

                    if body.is_empty() {
                        format!("{patterns}){suffix}")
                    } else {
                        format!("{patterns}) {body}{suffix}")
                    }
                })
                .collect::<Vec<String>>()
                .join(" ");

            format!(
                "case {} in {clauses} esac",
                rewrite_shell_word(&case_cmd.word, ctx)
            )
        }
        Command::Function(function) => {
            let body = render_shell_block_with_nameof(&function.body, ctx);
            format!("{}() {{ {body}; }}", function.name)
        }
        Command::Group(body) => format!("{{ {}; }}", render_shell_block_with_nameof(body, ctx)),
    }
}

fn render_shell_block_with_nameof(commands: &[Command], ctx: &RenderContext) -> String {
    let rendered = commands
        .iter()
        .map(|command| render_shell_command_with_nameof(command, ctx))
        .collect::<Vec<String>>();

    let mut output = String::new();
    for (index, command) in rendered.iter().enumerate() {
        if index > 0 {
            if rendered[index - 1].trim_end().ends_with('&') {
                output.push(' ');
            } else {
                output.push_str("; ");
            }
        }
        output.push_str(command);
    }
    output
}

fn rewrite_shell_word(word: &str, ctx: &RenderContext) -> String {
    if let Some(alias) = ctx.resolve_var(word) {
        return nameof_placeholder(&alias);
    }
    word.to_string()
}

fn rewrite_arithmetic_expression(expression: &str, ctx: &RenderContext) -> String {
    let chars: Vec<char> = expression.chars().collect();
    let mut output = String::new();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while i < chars.len() {
        let ch = chars[i];

        if escaped {
            output.push(ch);
            escaped = false;
            i += 1;
            continue;
        }

        if ch == '\\' {
            output.push(ch);
            escaped = true;
            i += 1;
            continue;
        }

        if ch == '\'' && !in_double {
            in_single = !in_single;
            output.push(ch);
            i += 1;
            continue;
        }

        if ch == '"' && !in_single {
            in_double = !in_double;
            output.push(ch);
            i += 1;
            continue;
        }

        if in_single || in_double {
            output.push(ch);
            i += 1;
            continue;
        }

        if ch == '$' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                if j < chars.len() {
                    let name: String = chars[i + 2..j].iter().collect();
                    if let Some(alias) = ctx.resolve_var(&name) {
                        output.push_str(&nameof_placeholder(&alias));
                    } else {
                        output.push('$');
                        output.push('{');
                        output.push_str(&name);
                        output.push('}');
                    }
                    i = j + 1;
                    continue;
                }
            }

            if i + 1 < chars.len() && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_') {
                let mut j = i + 2;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let name: String = chars[i + 1..j].iter().collect();
                if let Some(alias) = ctx.resolve_var(&name) {
                    output.push_str(&nameof_placeholder(&alias));
                } else {
                    output.push('$');
                    output.push_str(&name);
                }
                i = j;
                continue;
            }
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let name: String = chars[i..j].iter().collect();
            if let Some(alias) = ctx.resolve_var(&name) {
                output.push_str(&nameof_placeholder(&alias));
            } else {
                output.push_str(&name);
            }
            i = j;
            continue;
        }

        output.push(ch);
        i += 1;
    }

    output
}

fn nameof_placeholder(alias: &str) -> String {
    format!("__B2A_NAMEOF_{alias}__")
}

fn expand_nameof_placeholders(text: &str) -> String {
    let prefix = "__B2A_NAMEOF_";
    let mut output = String::new();
    let mut cursor = 0;

    while let Some(relative) = text[cursor..].find(prefix) {
        let start = cursor + relative;
        output.push_str(&text[cursor..start]);

        let after_prefix = start + prefix.len();
        if let Some(end_relative) = text[after_prefix..].find("__") {
            let end = after_prefix + end_relative;
            let alias = &text[after_prefix..end];
            output.push_str(&format!("{{nameof {alias}}}"));
            cursor = end + 2;
        } else {
            output.push_str(&text[start..]);
            cursor = text.len();
            break;
        }
    }

    if cursor < text.len() {
        output.push_str(&text[cursor..]);
    }

    output
}
