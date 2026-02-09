use std::collections::HashMap;

use crate::bash::ast::*;

use super::context::GlobalVarType;
use super::syntax::{classify_assignment_rhs, is_identifier};

pub(super) fn collect_non_local_assignments(commands: &[Command], out: &mut HashMap<String, Option<GlobalVarType>>) {
    for command in commands {
        collect_non_local_assignments_from_command(command, out);
    }
}

fn collect_non_local_assignments_from_command(command: &Command, out: &mut HashMap<String, Option<GlobalVarType>>) {
    match command {
        Command::Simple(simple) => {
            // Skip `local` assignments
            if simple.words.first().is_some_and(|w| w == "local") {
                return;
            }
            if let Some(first_word) = simple.words.first() {
                if let Some((name, value)) = first_word.split_once('=') {
                    if is_identifier(name) {
                        let rhs_type = classify_assignment_rhs(value);
                        let entry = out.entry(name.to_string()).or_insert(Some(rhs_type));
                        if let Some(existing) = *entry {
                            *entry = existing.merge(rhs_type);
                        }
                    }
                }
            }
        }
        Command::If(if_cmd) => {
            collect_non_local_assignments(&if_cmd.then_body, out);
            if let Some(else_body) = &if_cmd.else_body {
                collect_non_local_assignments(else_body, out);
            }
        }
        Command::While(while_cmd) => {
            collect_non_local_assignments(&while_cmd.body, out);
        }
        Command::For(for_cmd) => {
            collect_non_local_assignments(&for_cmd.body, out);
        }
        Command::CStyleFor(for_cmd) => {
            collect_non_local_assignments(&for_cmd.body, out);
        }
        Command::Case(case_cmd) => {
            for clause in &case_cmd.clauses {
                collect_non_local_assignments(&clause.body, out);
            }
        }
        Command::Group(body) => {
            collect_non_local_assignments(body, out);
        }
        Command::Function(_) => {
            // Don't recurse into nested function definitions
        }
        Command::Arithmetic(_) | Command::Background(_) | Command::Connection(_) => {}
    }
}

pub(super) fn commands_reference_variable(commands: &[Command], var_name: &str) -> bool {
    let dollar_var = format!("${var_name}");
    let braced_var = format!("${{{var_name}}}");
    commands
        .iter()
        .any(|cmd| command_references_variable(cmd, &dollar_var, &braced_var))
}

fn command_references_variable(command: &Command, dollar_var: &str, braced_var: &str) -> bool {
    match command {
        Command::Simple(simple) => simple
            .words
            .iter()
            .any(|word| word.contains(dollar_var) || word.contains(braced_var)),
        Command::If(if_cmd) => {
            command_references_variable(&if_cmd.condition, dollar_var, braced_var)
                || if_cmd
                    .then_body
                    .iter()
                    .any(|c| command_references_variable(c, dollar_var, braced_var))
                || if_cmd.else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|c| command_references_variable(c, dollar_var, braced_var))
                })
        }
        Command::While(while_cmd) => {
            command_references_variable(&while_cmd.condition, dollar_var, braced_var)
                || while_cmd
                    .body
                    .iter()
                    .any(|c| command_references_variable(c, dollar_var, braced_var))
        }
        Command::For(for_cmd) => {
            for_cmd.items.iter().any(|w| w.contains(dollar_var) || w.contains(braced_var))
                || for_cmd
                    .body
                    .iter()
                    .any(|c| command_references_variable(c, dollar_var, braced_var))
        }
        Command::CStyleFor(for_cmd) => for_cmd
            .body
            .iter()
            .any(|c| command_references_variable(c, dollar_var, braced_var)),
        Command::Case(case_cmd) => {
            case_cmd.word.contains(dollar_var)
                || case_cmd.word.contains(braced_var)
                || case_cmd.clauses.iter().any(|clause| {
                    clause
                        .body
                        .iter()
                        .any(|c| command_references_variable(c, dollar_var, braced_var))
                })
        }
        Command::Group(body) => body
            .iter()
            .any(|c| command_references_variable(c, dollar_var, braced_var)),
        Command::Function(func) => func
            .body
            .iter()
            .any(|c| command_references_variable(c, dollar_var, braced_var)),
        Command::Background(inner) => command_references_variable(inner, dollar_var, braced_var),
        Command::Connection(conn) => {
            command_references_variable(&conn.left, dollar_var, braced_var)
                || command_references_variable(&conn.right, dollar_var, braced_var)
        }
        Command::Arithmetic(arith) => {
            arith.expression.contains(dollar_var) || arith.expression.contains(braced_var)
        }
    }
}
