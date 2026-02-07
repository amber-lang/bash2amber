# Bash FunctionDef Analysis

This document provides a deep analysis of how function definitions (`FunctionDef`) work within the Bash codebase, covering their data structure, parsing, execution (registration), and printing.

## 1. Data Structure

The core structure representing a function definition is `FUNCTION_DEF`, defined in `command.h`. It is one of the union members of the `COMMAND` struct.

```c
/* command.h */
typedef struct function_def {
  int flags;			/* See description of CMD flags. */
  int line;			/* Line number the function def starts on. */
  WORD_DESC *name;		/* The name of the function. */
  COMMAND *command;		/* The parsed execution tree (function body). */
  char *source_file;		/* file in which function was defined, if any */
} FUNCTION_DEF;
```

A `COMMAND` of type `cm_function_def` holds a pointer to this structure in its `value.Function_def` field.

## 2. Parsing and Creation

Function definitions are parsed in `parse.y`. The grammar supports multiple syntaxes:

1.  **POSIX style**: `name () { ... }`
2.  **Keyword style**: `function name { ... }` (ksh compatible)
3.  **Hybrid**: `function name () { ... }`

The grammar rules invoke `make_function_def` (from `make_cmd.c`) to create the structure.

```yacc
/* parse.y */
function_def:	WORD '(' ')' newline_list function_body
			{ $$ = make_function_def ($1, $5, function_dstart, function_bstart); ... }
	|	FUNCTION WORD '(' ')' newline_list function_body
			{ $$ = make_function_def ($2, $6, function_dstart, function_bstart); ... }
    /* ... other variations ... */
```

The `make_function_def` function initializes the `FUNCTION_DEF` struct, populating it with the name, the command body, and source file information (derived from `BASH_SOURCE` or context).

```c
/* make_cmd.c */
COMMAND *
make_function_def (WORD_DESC *name, COMMAND *command, int lineno, int lstart)
{
  FUNCTION_DEF *temp = (FUNCTION_DEF *)xmalloc (sizeof (FUNCTION_DEF));
  temp->command = command;
  temp->name = name;
  /* ... source file resolution ... */
  return (make_command (cm_function_def, (SIMPLE_COM *)temp));
}
```

## 3. Execution (Registration)

Unlike other commands that perform an action when executed, executing a `cm_function_def` instruction *defines* the function in the current shell environment. It does **not** run the function body at that time.

The execution flow in `execute_cmd.c`:

1.  `execute_command_internal` encounters `cm_function_def`.
2.  It calls `execute_intern_function`.

```c
/* execute_cmd.c */
case cm_function_def:
  exec_result = execute_intern_function (command->value.Function_def->name,
                                         command->value.Function_def);
```

3.  `execute_intern_function` validates the function name (checking for POSIX compliance or readonly status).
4.  It calls `bind_function` to register the name and command body into the shell's hash table of functions.

```c
/* execute_cmd.c */
static int
execute_intern_function (WORD_DESC *name, FUNCTION_DEF *funcdef)
{
  /* Validation logic ... */
  bind_function (name->word, funcdef->command);
  return (EXECUTION_SUCCESS);
}
```

This effectively "saves" the command tree under that name for later invocation.

## 4. Printing

Bash can pretty-print function definitions (e.g., for `type` or `declare -f`). This is handled in `print_cmd.c` by `print_function_def`.

The printing logic:
1.  Checks `posixly_correct` to decide whether to print the `function` keyword.
2.  Prints the name and parentheses.
3.  Recursively prints the function body (which is usually a `cm_group` command `{ ... }`).
4.  Handles formatting and indentation to reconstruct a readable source representation.
5.  Attaches any redirections associated with the function definition.

```c
/* print_cmd.c */
static void
print_function_def (FUNCTION_DEF *func)
{
  /* logic to choose "function foo ()" vs "foo ()" */
  if (posixly_correct == 0)
    cprintf ("function %s () \n", w->word);
  else ...

  /* Print body */
  make_command_string_internal (func->command);
}
```

## Summary

The `FunctionDef` mechanism in Bash is a bridge between parsing and storage.
- **Parsed** as a command type `cm_function_def`.
- **Executed** by binding the parsed command tree to a name in the global function table.
- **Printed** by reconstructing the source from the stored command tree.
