# Deep Analysis: Compound Commands in Bash

This document analyzes the implementation of Compound Commands in the Bash source code.

## 1. Concept and Definition

In Bash, a **Compound Command** is a high-level construct that groups other commands or provides control flow. Unlike `Simple Commands` (which execute a single process or builtin), Compound Commands include:
*   **Looping Constructs**: `for`, `while`, `until`
*   **Conditionals**: `if`, `case`, `[[ ... ]]`
*   **Grouping**: `{ ... }` (Group), `( ... )` (Subshell)
*   **Arithmetic**: `(( ... ))`
*   **Function Definitions** (structurally treated similarly in command execution)

## 2. Data Structures (`command.h`)

Bash uses a unified `COMMAND` structure to represent both simple and compound commands. It acts as a tagged union.

```c
/* command.h */
typedef struct command {
  enum command_type type;   /* cm_for, cm_case, cm_while, cm_if, cm_simple, ... */
  int flags;                /* Execution flags (e.g., CMD_WANT_SUBSHELL) */
  int line;                 /* Line number */
  REDIRECT *redirects;      /* Redirections applied to the entire compound command */
  union {
    struct for_com *For;
    struct case_com *Case;
    struct while_com *While;
    struct if_com *If;
    struct connection *Connection;
    struct simple_com *Simple;
    struct function_def *Function_def;
    struct group_com *Group;
    /* ... others ... */
  } value;
} COMMAND;
```

Each specific compound command type has its own auxiliary structure holding its specific data (e.g., `FOR_COM` holds the iteration variable description, list to map over, and the action command).

## 3. Parsing (`parse.y` and `make_cmd.c`)

The Yacc grammar (`parse.y`) differentiates between `simple_command` and `shell_command` (which largely corresponds to compound commands).

### Grammar
The `command` non-terminal parses into:
*   `simple_command`
*   `shell_command` (Compound commands)
*   `function_def`
*   `coproc`

The `shell_command` rule aggregates:
*   `for_command`
*   `case_command`
*   `WHILE/UNTIL` constructs
*   `if_command`
*   `subshell`
*   `group_command`
*   `cond_command` (`[[ ... ]]`)
*   `arith_command` (`(( ... ))`)

### Creation
The `make_cmd.c` file provides factory functions (e.g., `make_for_command`, `make_if_command`) that:
1.  Allocate the specific structure (e.g., `FOR_COM`).
2.  Populate it with parsed data (words, lists, inner action commands).
3.  Wrap it in a generic `COMMAND` struct using `make_command`.

## 4. Execution Flow (`execute_cmd.c`)

The core execution logic resides in `execute_command_internal`. This function handles the common behaviors of compound commands (like redirection and subshell execution) before dispatching to specific handlers.

### 4.1. Common Handling

Before executing the specific logic of a compound command, `execute_command_internal` performs several critical steps:

1.  **Subshells**: It checks if the command requires a subshell (`CMD_WANT_SUBSHELL` or piped contexts). If so, it forks and recursively calls execution in the child.
2.  **Redirections**: It applies redirections attached to the compound command (`do_redirections`).
    *   *Crucially*, it manages `unwind_protect` frames to undo these redirections after the compound command finishes, restoring the parent shell's file descriptor state.
3.  **Traps**: It checks specifically for the `ERR` trap and `set -e` conditions.

### 4.2. Dispatch

A large `switch` statement on `command->type` delegates execution to specific functions:

*   `cm_for` -> `execute_for_command`
*   `cm_case` -> `execute_case_command`
*   `cm_while` -> `execute_while_command`
*   `cm_if` -> `execute_if_command`
*   `cm_group` -> `execute_command_internal` (recursively on the inner command)
*   `cm_connection` -> `execute_connection` (handles `;`, `&&`, `||`)

### 4.3. Specific Implementations

*   **For Loop (`execute_for_command`)**:
    *   Expands the `map_list` (the words after `in`).
    *   Iterates through items, binding the variable.
    *   Repeatedly calls `execute_command` on the loop body `action`.
*   **If Statement (`execute_if_command`)**:
    *   Executes the `test` command.
    *   Based on the return code, executes either `true_case` or `false_case`.
*   **Group Command (`cm_group`)**:
    *   If synchronous, it simply executes the inner command.
    *   If asynchronous (`&`), it forces a subshell execution.

## 5. Summary

Compound Commands in Bash are implemented as recursive structures. The `COMMAND` struct serves as the node in the Abstract Syntax Tree (AST). The execution engine traverses this tree, handling environment setup (redirections, variable scopes) at each node before diving deeper. This architecture allows complex nesting of structures (e.g., `if` inside `for` inside `case`) to be handled naturally by the recursive `execute_command` logic.
