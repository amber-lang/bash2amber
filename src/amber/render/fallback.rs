use crate::bash::ast::*;

use super::context::RenderContext;

pub(super) fn command_literal_from_command(command: &Command, ctx: &RenderContext) -> String {
    let shell = render_shell_command_with_nameof(command, ctx);
    let escaped = escape_for_command_literal(shell.trim());
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
    commands
        .iter()
        .map(|command| render_shell_command_with_nameof(command, ctx))
        .collect::<Vec<String>>()
        .join("; ")
}

fn rewrite_shell_word(word: &str, ctx: &RenderContext) -> String {
    if let Some(alias) = ctx.resolve_var(word) {
        return nameof_placeholder(&alias);
    }
    word.to_string()
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
