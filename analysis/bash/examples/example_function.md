# The Function Definition and Call

This document analyzes the parsing of valid token sequences for defining and calling functions in Bash. The example code is:

```bash
foo() {
  echo "Hello $1"
}

foo "Pablo"
```

## 1. Defining the Function

The first part of the code defines the function. This is handled by the `function_def` grammar rule in `parse.y`.

### Step A: The Header (`foo()`)
1.  **Scanner**: Reads `foo`. Returns **WORD**.
2.  **Scanner**: Reads `(`. Returns **(**.
3.  **Scanner**: Reads `)`. Returns **)**.
4.  **Grammar**: Matches the rule:
    ```yacc
    function_def: WORD '(' ')' newline_list function_body
    ```
    *   `WORD`: "foo" (The function name).
    *   `(` `)`: The required parentheses for this syntax style.
    *   `newline_list`: Handles the newline after `)`.

### Step B: The Body (`{ ... }`)
The `function_body` rule transitions to `shell_command`, which can be a `group_command`.

1.  **Scanner**: Reads `{`. Returns **{**.
    *   *Note*: The parser tracks compound command nesting (`compoundcmd_top`) to handle newlines correctly.
2.  **Inner Command**:
    *   Scanner reads `echo`, `"Hello $1"`.
    *   Parses as a `simple_command` (Simple Command).
3.  **Scanner**: Reads `}`. Returns **}**.
4.  **Grammar**: The `{ ... }` block matches the `group_command` rule:
    ```yacc
    group_command: '{' compound_list '}'
    ```
    *   `make_group_command` creates a `COMMAND` of type `cm_group`.

### Step C: Constructing the Function AST
Finally, the `function_def` reduction fires:
```c
$$ = make_function_def ($1, $5, ...);
```
*   `$1`: The name "foo".
*   `$5`: The body (the Group Command AST).
*   **Result**: A `COMMAND` node of type `cm_function_def`. It contains the name and the executable body.

## 2. Calling the Function

The second part `foo "Pablo"` is parsed just like any other command.

1.  **Scanner**: Reads `foo`. Returns **WORD**.
2.  **Scanner**: Reads `"Pablo"`. Returns **WORD**.
3.  **Grammar**: Matches `simple_command`.
    *   The parser *does not* know "foo" is a function at this stage. It just sees a command named "foo".
4.  **Result**: A `COMMAND` node (Type: `cm_simple`) with args `["foo", "Pablo"]`.

## 3. Execution (The Connection)

The magic happens at runtime (`execute_cmd.c`):
1.  **Definition**: When the `cm_function_def` node is executed, Bash stores the body AST in its internal hash table of functions, keyed by "foo".
2.  **Call**: When the `cm_simple` node for `foo "Pablo"` is executed:
    *   Bash looks up "foo".
    *   It finds the function definition.
    *   It executes the stored body AST, with arguments (`$1`, etc.) temporarily set to "Pablo".
