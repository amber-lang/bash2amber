# The Pipeline and the Process Substitution

This document analyzes the parsing of the command:

```bash
paste -sd+ <(seq 1 5) | bc
```

This command showcases two interesting features: **Process Substitution** and **Pipelines**.

## 1. Input Tokenization (The Lexer)

The lexer (`read_token` in `parse.y`) scans the input string.

### A. Process Substitution `<(seq 1 5)`

This is the most critical part of the lexical analysis.
1.  **Detection**: The scanner reads `<`.
2.  **Lookahead**: It peeks at the next character and sees `(`.
3.  **Recognition**: The scanner recognizes the `<(` sequence as the start of a "shell expansion" (handled in `read_token` around line 5500).
4.  **Consumption**:
    *   It calls `parse_comsub` (Parse Command Substitution) recursively.
    *   `parse_comsub` parses the inner command `seq 1 5` until the matching `)`.
    *   The entire sequence `<(seq 1 5)` is returned as a **single WORD token**.
    *   **Crucial Note**: To the parser grammar, this is indistinguishable from a regular string like `"filename"`. It is *not* a redirection grammar rule.

### B. The Pipe `|`

The scanner identifies the vertical bar `|` and returns the token `|` (referenced in `parse.y` as `'|'`).

## 2. Parsing the Commands

The parser now receives the stream of tokens:
`WORD("paste")`, `WORD("-sd+")`, `WORD("<(seq 1 5)")`, `|`, `WORD("bc")`.

### Step A: Left Command (`paste`)
1.  The grammar matches a `simple_command`:
    *   Element 1: `paste`
    *   Element 2: `-sd+`
    *   Element 3: `<(seq 1 5)`
2.  The result is a `COMMAND` node (Type: `cm_simple`) with a list of 3 words.

### Step B: Right Command (`bc`)
1.  The grammar matches a `simple_command`:
    *   Element 1: `bc`
2.  The result is a `COMMAND` node (Type: `cm_simple`) with 1 word.

## 3. Parsing the Pipeline

The grammar rule for pipelines is defined around line 1471 of `parse.y`:

```yacc
pipeline:      pipeline '|' newline_list pipeline
                        { $$ = command_connect ($1, $4, '|'); }
```

1.  **Reduction**: The parser sees `simple_command` (Left), then `|`, then `simple_command` (Right).
2.  **Action**: It calls `command_connect`.
3.  **Result**: It creates a `COMMAND` connection node (Type: `cm_connection`).
    *   `connector`: `|`
    *   `first`: The `paste` AST.
    *   `second`: The `bc` AST.

## 4. Final AST Structure

```text
COMMAND (Type: cm_connection)
|
+-- Connector: Pipe (|)
|
+-- First (Left): COMMAND (cm_simple)
|     +-- Words: ["paste", "-sd+", "<(seq 1 5)"]
|
+-- Second (Right): COMMAND (cm_simple)
      +-- Words: ["bc"]
```

## 5. Execution Insight

While the Parser treats `<(seq 1 5)` as a simple word, the **Executor** gives it meaning.
1.  During `execute_simple_command` for the left side, the list of words undergoes **Shell Expansion**.
2.  The expander detects the `<(` prefix.
3.  It sets up a pipe, forks a subshell to run `seq 1 5` connected to that pipe, and replaces the string `<(seq 1 5)` with the file path to that pipe (e.g., `/dev/fd/63`).
4.  Finally, `paste` is executed with arguments `-sd+` and `/dev/fd/63`.
