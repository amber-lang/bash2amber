# Analysis: Parsing `if [[ ... ]]` with Conditional Command

This document analyzes how the following Bash code is parsed, specifically focusing on the `[[ ... ]]` construct which behaves differently from `[ ... ]`.

```bash
number=64
if [[ "$number" -eq 64 ]]; then
    echo "This is the correct answer"
fi
```

## 1. Overview

Unlike `[`, which is technically a regular command (an alias for `test`), `[[` is a keywords that triggers a **Conditional Command** (`cond_command`). This involves a hybrid parsing approach:
1.  **Standard Yacc Grammar**: Recognized the high-level struct (`if`, `[[`, `]]`).
2.  **Recursive Descent Parser**: A specialized function (`parse_cond_command`) parses the internal expression structure (operators like `-eq`, `&&`, `||`, `==`).

## 2. Phase 1: Lexical Analysis & Reserved Words

The lexer (`yylex`) reads tokens.

### `number=64`
This is parsed as an `ASSIGNMENT_WORD` (see [example_if.md](example_if.md)).

### `if`
Recognized as the reserved word `IF`.

### `[[` (The Trigger)
1.  The lexer reads `[[`.
2.  `check_for_reserved_word` identifies this as the token `COND_START`.
3.  **Critical Side Effect**: It sets the `PST_CONDCMD` flag in the `parser_state`. This flag tells the lexer "we are entering a conditional expression; handle the next tokens differently."

## 3. Phase 2: Hybrid Parsing Mechanism

The grammar rule in `parse.y` for `cond_command` is:

```yacc
cond_command: COND_START COND_CMD COND_END
```

The parsing flow is complex here because `COND_CMD` is **not** a single text token. It is a "super-token" returned by `read_token` representing the entire parsed expression tree.

### Step-by-Step Flow

1.  **Shift `COND_START`**: `yyparse` consumes the `[[` (token `COND_START`).
2.  **Request Next Token**: `yyparse` asks for the next token to match `COND_CMD`.
3.  **`read_token` Interception**:
    *   `read_token` checks `if (parser_state & PST_CONDCMD)`.
    *   Since the flag is set, it **hijacks** the control flow.
    *   It calls `parse_cond_command()`.

### `parse_cond_command()` (Recursive Descent)

This function (in `parse.y`) parses the expression inside `[[ ... ]]`.

1.  **`cond_expr()`**: Calls `cond_or()`.
2.  **`cond_or()`**: Calls `cond_and()`.
3.  **`cond_and()`**: Calls `cond_term()`.
4.  **`cond_term()`**:
    *   Reads `"$number"` (`WORD`).
    *   Creates a `COND_EXPR` node (LHS).
    *   Reads `-eq` (`WORD`).
    *   Recognizes it as a binary operator (`test_binop`).
    *   Reads `64` (`WORD`).
    *   Creates a `COND_EXPR` node (RHS).
    *   Combines them into a `COND_BINARY` node.
    *   Reads the next token: `]]`.
    *   Since `]]` corresponds to `COND_END`, `cond_term` finishes and returns the tree.

### Returning Control to Yacc

1.  `parse_cond_command` returns the `COND_COM` pointer (the AST of the expression).
2.  `read_token`:
    *   Verifies that the last token read was indeed `COND_END` (`]]`).
    *   Stores `COND_END` in `token_to_read` (a buffer for the *next* call).
    *   Clears `PST_CONDCMD` flag.
    *   **Returns `COND_CMD`**. This token carries the `COND_COM` tree in `yylval.command`.

3.  **Shift `COND_CMD`**: `yyparse` receives the `COND_CMD` token and shifts it.
4.  **Shift `COND_END`**: `yyparse` asks for the next token. `read_token` returns the buffered `COND_END`.

## 4. AST Construction

The result is a `COMMAND` structure of type `cm_cond`.

```c
// High-Level IF Command
COMMAND *if_cmd = {
    .type = cm_if,
    .value.If = {
        .test = COMMAND {
            .type = cm_cond,  // <--- The [[ ... ]] part
            .value.Cond = {
                .type = COND_BINARY,
                .op = "-eq",
                .left = { .type = COND_TERM, .word = "$number" },
                .right = { .type = COND_TERM, .word = "64" }
            }
        },
        .true_case = COMMAND(cm_simple, words=["echo", ...]),
        .false_case = NULL
    }
};
```

## 5. Key Differences from `[ ... ]`

| Feature | `[ ... ]` (Test Command) | `[[ ... ]]` (Conditional Command) |
| :--- | :--- | :--- |
| **Parsing** | Standard command parsing (list of words). | Specialized recursive descent parsing. |
| **Operators** | `-eq`, `>`, `<` are just arguments to the command. | Recognized as operators at parse time. |
| **AST** | `cm_simple` (flat list of words). | `cm_cond` (structured binary tree). |
| **Expansion** | Variables expanded *before* execution (word splitting applies). | Variables parsed as part of expression (word splitting suppressed). |
| **Handling** | Logic happens at *runtime* (`test.c`). | Logic structure built at *parse time*. |

## 6. Summary

The parsing of `if [[ "$number" -eq 64 ]]; then ...` involves a seamless handoff between the generated Yacc parser and a handwritten recursive descent parser. The `[[` token triggers a mode switch where Bash parses the internal expression into a rich binary tree (`COND_COM`), encapsulating it into a single `COND_CMD` token for the main grammar.
