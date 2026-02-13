mod analysis;
mod arithmetic;
pub(crate) mod context;
mod control_flow;
mod fallback;
mod fragment_expr;
mod function;
mod simple_cmd;
mod substitution;
mod syntax;
mod type_comment;
pub(crate) mod word;

use std::collections::{HashMap, HashSet};

use crate::amber::fragments::{
    BlockFragment, ForFragment, FragmentKind, FragmentRenderable, Fragments, IfFragment,
    RawFragment, TranslateMetadata, VarStmtFragment, WhileFragment,
};
use crate::bash::ast::*;

use context::{GlobalVarType, RenderContext};
use control_flow::{
    parse_c_style_for_range, render_case, render_echo_ternary_connection,
    render_if_ternary_assignment, render_tail_return_expr,
};
use fallback::command_literal_from_command;
use fragment_expr::render_expression_fragment;
use function::{preregister_function_signatures, render_function};
use simple_cmd::render_simple;
use type_comment::collect_function_hints;
use word::{render_condition_expr, render_for_items};

pub fn render_program(
    program: &Program,
    source: Option<&str>,
    source_path: Option<&str>,
) -> String {
    let hints = source.map(collect_function_hints).unwrap_or_default();
    let mut ctx = RenderContext::new(hints, source_path.map(ToOwned::to_owned));
    let items = render_commands(&program.statements, &mut ctx, false);

    let fragments = Fragments {
        fragment: FragmentKind::block(items),
    };
    fragments.to_string(&mut TranslateMetadata::default())
}

fn render_commands(
    commands: &[Command],
    ctx: &mut RenderContext,
    return_tail: bool,
) -> Vec<FragmentKind> {
    preregister_function_signatures(commands, ctx);

    // Collect global vars from function signatures and pre-declare them
    let mut all_global_vars: Vec<(String, GlobalVarType)> = Vec::new();
    let mut global_var_set: HashSet<String> = HashSet::new();
    let mut function_global_vars: HashMap<String, Vec<(String, GlobalVarType)>> = HashMap::new();
    for command in commands {
        if let Command::Function(function) = command {
            if let Some(sig) = ctx.resolve_function(&function.name) {
                for (var, ty) in &sig.global_vars {
                    if global_var_set.insert(var.clone()) {
                        all_global_vars.push((var.clone(), *ty));
                    }
                }
                if !sig.global_vars.is_empty() {
                    function_global_vars
                        .insert(function.name.clone(), sig.global_vars.clone());
                }
            }
        }
    }
    // Pre-declare all global vars so child scopes inherit them
    for (var, _) in &all_global_vars {
        ctx.declare_var(var);
    }

    let mut fragments = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        // Emit `let var = 0` or `let var = ""` right before the function that introduces them
        if let Command::Function(function) = command {
            if let Some(vars) = function_global_vars.get(&function.name) {
                for (var, ty) in vars {
                    let amber_name = ctx
                        .resolve_var(var)
                        .unwrap_or_else(|| var.clone());
                    let default_value = match ty {
                        GlobalVarType::Text => "\"\"".to_string(),
                        GlobalVarType::Int | GlobalVarType::Unknown => "0".to_string(),
                    };
                    fragments.push(
                        VarStmtFragment {
                            name: amber_name,
                            value: Box::new(
                                RawFragment {
                                    value: default_value,
                                }
                                .to_frag(),
                            ),
                            is_reassignment: false,
                        }
                        .to_frag(),
                    );
                }
            }
        }

        let tail_return = return_tail && index + 1 == commands.len();
        fragments.extend(render_command(command, ctx, tail_return));
    }
    fragments
}

fn render_command(
    command: &Command,
    ctx: &mut RenderContext,
    tail_return: bool,
) -> Vec<FragmentKind> {
    match command {
        Command::If(if_cmd) => {
            if let Some(ternary_assignment) = render_if_ternary_assignment(if_cmd, ctx) {
                return vec![
                    RawFragment {
                        value: ternary_assignment,
                    }
                    .to_frag(),
                ];
            }

            let condition = render_condition_expr(&if_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&if_cmd.condition, ctx));

            let mut then_ctx = ctx.with_child_scope();
            let then_body = BlockFragment::new(
                render_commands(&if_cmd.then_body, &mut then_ctx, tail_return),
                true,
            );

            let else_body = if let Some(else_body) = &if_cmd.else_body {
                let mut else_ctx = ctx.with_child_scope();
                let rendered = BlockFragment::new(
                    render_commands(else_body, &mut else_ctx, tail_return),
                    true,
                );
                ctx.merge_from_child(else_ctx);
                Some(rendered)
            } else {
                None
            };

            ctx.merge_from_child(then_ctx);
            vec![
                IfFragment {
                    condition: Box::new(render_expression_fragment(&condition)),
                    then_body,
                    else_body,
                }
                .to_frag(),
            ]
        }
        Command::While(while_cmd) => {
            let condition = render_condition_expr(&while_cmd.condition, ctx)
                .unwrap_or_else(|| command_literal_from_command(&while_cmd.condition, ctx));

            let mut loop_ctx = ctx.with_child_scope();
            let body =
                BlockFragment::new(render_commands(&while_cmd.body, &mut loop_ctx, false), true);
            ctx.merge_from_child(loop_ctx);
            vec![
                WhileFragment {
                    condition: Box::new(render_expression_fragment(&condition)),
                    body,
                }
                .to_frag(),
            ]
        }
        Command::For(for_cmd) => {
            let items = render_for_items(&for_cmd.items, ctx).unwrap_or_else(|| {
                let command = Command::Simple(SimpleCommand {
                    words: for_cmd.items.clone(),
                });
                command_literal_from_command(&command, ctx)
            });

            let var_name = ctx.declare_var(&for_cmd.variable);

            let mut loop_ctx = ctx.with_child_scope();
            loop_ctx.declare_var(&for_cmd.variable);
            let body =
                BlockFragment::new(render_commands(&for_cmd.body, &mut loop_ctx, false), true);
            ctx.merge_from_child(loop_ctx);

            vec![
                ForFragment {
                    variable: var_name,
                    items,
                    body,
                }
                .to_frag(),
            ]
        }
        Command::CStyleFor(for_cmd) => {
            if let Some(spec) = parse_c_style_for_range(for_cmd, ctx) {
                let var_name = ctx.declare_var(&spec.variable);
                let range_op = if spec.inclusive { "..=" } else { ".." };
                let items = format!("{}{range_op}{}", spec.start, spec.end);

                let mut loop_ctx = ctx.with_child_scope();
                loop_ctx.declare_var(&spec.variable);
                let body =
                    BlockFragment::new(render_commands(&for_cmd.body, &mut loop_ctx, false), true);
                ctx.merge_from_child(loop_ctx);

                vec![
                    ForFragment {
                        variable: var_name,
                        items,
                        body,
                    }
                    .to_frag(),
                ]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
        Command::Arithmetic(arith) => {
            if let Some(rendered) =
                arithmetic::render_arithmetic_statement_expr(&arith.expression, ctx)
            {
                vec![RawFragment { value: rendered }.to_frag()]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
        Command::Background(_) => vec![
            RawFragment {
                value: command_literal_from_command(command, ctx),
            }
            .to_frag(),
        ],
        Command::Case(case_cmd) => {
            vec![render_case(case_cmd, ctx, tail_return, render_commands)]
        }
        Command::Function(function) => {
            vec![render_function(function, ctx, render_commands)]
        }
        Command::Group(body) => render_commands(body, ctx, tail_return),
        Command::Simple(simple) => {
            if tail_return && let Some(expr) = render_tail_return_expr(simple, ctx) {
                return vec![
                    RawFragment {
                        value: format!("return {expr}"),
                    }
                    .to_frag(),
                ];
            }
            render_simple(simple, ctx).into_iter().collect()
        }
        Command::Connection(connection) => {
            if let Some(rendered) = render_echo_ternary_connection(connection, ctx) {
                vec![RawFragment { value: rendered }.to_frag()]
            } else {
                vec![
                    RawFragment {
                        value: command_literal_from_command(command, ctx),
                    }
                    .to_frag(),
                ]
            }
        }
    }
}
