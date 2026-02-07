# Deep Analysis of ForCommand in Bash

This document provides a deep analysis of how the `for` command is implemented in the Bash source code. It covers the data structures, AST construction, and execution logic for both standard `for` loops and arithmetic `for ((...))` loops.

## 1. Data Structures

The internal representation of `for` commands is defined in `command.h`. There are two distinct structures depending on the type of loop.

### 1.1 Standard `for` Loop (`FOR_COM`)

Used for `for name in words; do ...; done`.

```c
typedef struct for_com {
  int flags;            /* See description of CMD flags. */
  int line;             /* line number the `for' keyword appears on */
  WORD_DESC *name;      /* The variable name to get mapped over. */
  WORD_LIST *map_list;  /* The things to map over. This is never NULL. */
  COMMAND *action;      /* The action to execute. */
} FOR_COM;
```

*   **`name`**: The variable identifier (e.g., "i" in `for i in ...`).
*   **`map_list`**: The raw list of words to iterate over *before* expansion.
*   **`action`**: The command (usually a `GroupCommand` or `Connection`) executed in each iteration.

### 1.2 Arithmetic `for` Loop (`ARITH_FOR_COM`)

Used for `for (( init; test; step )); do ...; done`.

```c
typedef struct arith_for_com {
  int flags;
  int line;
  WORD_LIST *init;      /* Initialization expression */
  WORD_LIST *test;      /* Loop condition */
  WORD_LIST *step;      /* Iteration step */
  COMMAND *action;      /* The loop body */
} ARITH_FOR_COM;
```

*   **`init`, `test`, `step`**: These are stored as `WORD_LIST`s but effectively treated as arithmetic strings to be evaluated.

## 2. AST Construction

The construction of these nodes happens in `make_cmd.c`, typically driven by `parse.y` (the Yacc parser).

### 2.1 Making `FOR_COM`
The function `make_for_command` allocates and initializes the struct.

```c
COMMAND *
make_for_command (WORD_DESC *name, WORD_LIST *map_list, COMMAND *action, int lineno)
{
  return (make_for_or_select (cm_for, name, map_list, action, lineno));
}
```

It wraps the `FOR_COM` in a generic `COMMAND` structure with type `cm_for`.

### 2.2 Making `ARITH_FOR_COM`
The function `make_arith_for_command` is more complex because it has to parse the single string inside `((...))` into three distinct parts logic (init, test, step).

1.  It iterates through the tokens, looking for semicolons `;` to split the expressions.
2.  It handles errors if there aren't exactly 3 parts (or fewer with valid syntax).
3.  It defaults missing parts to `"1"` (true), making `for ((;;))` an infinite loop.

## 3. Execution Logic

The execution logic resides in `execute_cmd.c`.

### 3.1 Standard `for` Execution (`execute_for_command`)

**Key Steps:**

1.  **Identifier Check**: Verifies that `name` is a valid shell identifier.
2.  **Expansion**: Calls `expand_words_no_vars(map_list)` to expand wildcards, variables, etc., into the final list of items to iterate over.
3.  **Looping**: Iterates through the expanded list.
    *   **Binding**: Binds the current item to the variable `name`.
        *   Standard variables use `bind_variable`.
        *   `nameref` variables are handled specially (`bind_variable_value`).
    *   **Execution**: Calls `execute_command(for_command->action)`.
    *   **Control Flow Checks**: After every execution, it checks the global variables `breaking` and `continuing`:
        ```c
        if (breaking) { breaking--; break; }
        if (continuing) { continuing--; if (continuing) break; }
        ```
        This handling allows `break n` and `continue n` to work across nested loops by decrementing the counters.

4.  **Cleanup**: Disposes of the expanded word list and cleans up the unwind stack.

### 3.2 Arithmetic `for` Execution (`execute_arith_for_command`)

This function effectively translates the loop into:
```bash
eval (( init ))
while eval (( test )); do
    body
    eval (( step ))
done
```

**Key Steps:**

1.  **Initialization**: Evaluates `init` using `eval_arith_for_expr`.
2.  **Loop**:
    *   **Test**: Evaluates `test`. If result is 0 (false), break the loop.
    *   **Body**: Executes `action`.
    *   **Control Flow**: Checks `breaking` and `continuing`.
    *   **Step**: Evaluates `step`.

### 3.3 Helper: `eval_arith_for_expr`
This helper function expands the arithmetic string and uses `evalexp()` (the arithmetic evaluator) to compute the result.

## 4. Key Observations

1.  **Macro-Like Expansion**: The standard `for` loop expands the `map_list` *once* before the loop starts. This means updates to variables affecting the list *during* the loop do not affect the iteration set.
    *   *Contrast*: The arithmetic `for` loop evaluates `test` and `step` *every iteration*.

2.  **Unwind Protection**: Bash uses `unwind_protect` extensively (similar to try/finally) to ensure resources (variable bindings, expanded lists) are freed even if the loop is interrupted by a signal or error.

3.  **Global Control Flow**: `break` and `continue` are implemented via global integers (`breaking`, `continuing`), which the loop logic checks explicitly. This is how Bash handles `break 2` — the inner loop decrements `breaking` and returns, and the outer loop sees `breaking` is still > 0 and breaks essentially.
