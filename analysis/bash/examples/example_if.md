# Analysis: Parsing a Conditional Statement

This document analyzes how the following Bash code snippet is parsed by the bash shell, tracing its journey from raw text to an Abstract Syntax Tree (AST).

```bash
status="200"
site="google.com"
if [ "$status" == "200" ]; then
    echo "[UP]   $site is responding (Status: $status)"
fi
```

## 1. Overview

The parsing process involves two main stages:
1.  **Lexical Analysis (Scanning)**: Converting the character stream into tokens (e.g., `WORD`, `ASSIGNMENT_WORD`, `IF`).
2.  **Syntactic Analysis (Parsing)**: assembling these tokens into grammatical structures (e.g., `simple_command`, `if_command`) and building the AST using `make_cmd.c`.

## 2. Phase 1: Lexical Analysis

The lexer (`yylex` in `parse.y` and `read_token` function) reads the input one character at a time. It uses `read_token_word` to form words and categorizes them based on context and content.

### Line 1: `status="200"`

1.  **Scanning**: The lexer reads `status="200"`.
    *   It identifies the `=` character.
    *   It sees the double quotes `"` and processes the content "200".
    *   `quoted` flag is set in the `WORD_DESC`.
2.  **Classification**: `read_token_word` calls `assignment()` to check if the word looks like `name=value`.
    *   Since it is at the start of a command (or valid assignment position), `assignment_acceptable` returns true.
    *   The token is classified as **`ASSIGNMENT_WORD`**.

### Line 2: `site="google.com"`

*   Similar processing occurs. The lexer identifies this as another **`ASSIGNMENT_WORD`**.

### Line 3: `if [ "$status" == "200" ]; then`

1.  **`if`**: The lexer reads `if`. It checks a table of reserved words (`process_reserved_word`). It returns the token **`IF`**.
2.  **`[`**: The lexer reads `[`. This is **not** a special token in the grammar (unlike `[[`). It is returned as a **`WORD`**.
3.  **`"$status"`**: Read as a **`WORD`** with flags `W_QUOTED | W_HASDOLLAR`.
4.  **`==`**: Read as a **`WORD`**.
5.  **`"200"`**: Read as a **`WORD`** with flag `W_QUOTED`.
6.  **`]`**: Read as a **`WORD`**.
7.  **`;`**: The lexer identifies the semicolon. It returns the token **`;`** (or `yacc` equivalent for command separator).
8.  **`then`**: The lexer reads `then`. It matches a reserved word and returns the token **`THEN`**.

### Line 4: `echo "[UP] ..."`

1.  **`echo`**: Read as a **`WORD`**.
2.  **`"[UP] ..."`**: Read as a single **`WORD`** (quoted string).

### Line 5: `fi`

1.  **`fi`**: The lexer reads `fi`. It matches a reserved word and returns the token **`FI`**.

## 3. Phase 2: Syntactic Analysis (Parsing)

The parser (`parse.y`) uses a Yacc/Bison grammar to consume the tokens and trigger AST construction functions.

### Parsing the Assignments

The grammar rule for a simple command allows for `simple_command_element`s which include `ASSIGNMENT_WORD`.

```yacc
simple_command: simple_command_element
              | simple_command simple_command_element
```

1.  **`status="200"`**: Reduced to `simple_command`.
    *   Action: `make_simple_command` is called. It creates a `COMMAND` of type `cm_simple`. The word `status="200"` is added to its `words` list.
2.  **`site="google.com"`**: Similarly, `make_simple_command` creates another `cm_simple` command.

### Parsing the `if` Command

The grammar rule for `if` is:

```yacc
if_command: IF compound_list THEN compound_list FI
```

1.  **`IF`**: Parser attempts to match `if_command`.
2.  **`compound_list` (Condition)**: The parser consumes `[`, `"$status"`, `==`, `"200"`, `]`.
    *   These are all `WORD` tokens.
    *   They form a `simple_command`.
    *   The command name is `[`.
    *   `make_simple_command` creates a `cm_simple` node with words: `[` `"$status"` `==` `"200"` `]`.
    *   The `;` terminates the `simple_list`, completing the first `compound_list`.
3.  **`THEN`**: Matched.
4.  **`compound_list` (Body)**: The parser consumes `echo` and its argument.
    *   They form a `simple_command`.
    *   `make_simple_command` creates a `cm_simple` node with words: `echo` `"[UP] ... "`.
    *   The newline (implied or explicit) completes this `compound_list`.
5.  **`FI`**: Matched.
6.  **`make_if_command`**:
    *   The grammar action calls `make_if_command($2, $4, NULL)`.
    *   `$2` is the AST for the condition (`[ ... ]`).
    *   `$4` is the AST for the body (`echo ...`).
    *   `NULL` is passed for the `else` clause.

## 4. AST Construction

The final result is a `COMMAND` structure (defined in `command.h`) with type `cm_if`.

```c
COMMAND *if_cmd = ...;
if_cmd->type = cm_if;
if_cmd->value.If = {
    .test = COMMAND(cm_simple, words=["[", "$status", "==", "200", "]"]),
    .true_case = COMMAND(cm_simple, words=["echo", "[UP] ..."]),
    .false_case = NULL
};
```

### Key Differences from `[[ ... ]]`

It is important to note that because the user used `[ ... ]` (single brackets):
*   The parser sees `[` as just another command name (a `WORD`).
*   It does **not** use `cond_command` or specific conditional parsing logic (like `parse_cond_command`).
*   The actual "test" logic happens at **execution time** when the `[` command (which is an alias for `test`) is invoked.
*   If `[[ ... ]]` were used, `cond_command` (line 1207 of `parse.y`) would have been triggered, involving a recursive descent parser for the conditional expression itself.

## 5. Summary

The journey of the code snippet involves:
1.  **Lexer**: Identifying assignments as `ASSIGNMENT_WORD` and keywords like `if`, `then`, `fi`. Treating `[`, `echo`, and operands as via `WORD`.
2.  **Parser**: Grouping words into `simple_command`s.
3.  **Parser**: Recognizing the `if ... then ... fi` structure.
4.  **Builder**: constructing a `cm_if` command node containing two `cm_simple` command nodes (one for the test, one for the true branch).
