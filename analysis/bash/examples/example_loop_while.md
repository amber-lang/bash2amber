# The Loop and the Redirection: Parsing `while` with input

This document traces the parsing of a `while` loop that reads from a file. The code snippet is:

```bash
while read line; do
  echo "Processing: $line"
done < input.txt
```

This structure involves constructing a complex command (the loop) and then decorating it with a redirection.

## 1. Top-Level Grammar

The entire snippet is parsed as a single `command`. The relevant grammar rule in `parse.y` is:

```yacc
command: shell_command redirection_list
```

Here:
1.  **`shell_command`**: Matches the entire `while ... done` block.
2.  **`redirection_list`**: Matches the trailing `< input.txt`.

The parser first builds the loop (the `shell_command`), then builds the redirection, and finally attaches the redirection to the loop command.

## 2. Phase 1: Constructing the `while_command`

The grammar rule for the `while` loop is:

```yacc
while_command: WHILE compound_list DO compound_list DONE
```

### Step A: The Test Condition (`read line`)
1.  **`WHILE`**: The scanner identifies the reserved word `while`.
2.  **`compound_list`**: The parser expects a list of commands.
    *   Scanner reads `read` (WORD).
    *   Scanner reads `line` (WORD).
    *   Grammar builds a `simple_command` corresponding to `read line`.
    *   The semicolon `;` (or newline) marks the end of this list.
    *   **Result**: This list becomes the "test" part of the loop.

### Step B: The Body (`echo ...`)
1.  **`DO`**: The reserved word `do` transitions the parser to the loop body.
2.  **`compound_list`**:
    *   Scanner reads `echo` (WORD).
    *   Scanner reads `"Processing: $line"`. This is parsed as a single `WORD` with variable expansion flags (due to double quotes and `$`).
    *   Grammar builds a `simple_command` for the echo statement.
3.  **`DONE`**: The reserved word `done` terminates the loop body.

### Step C: Assembly (`make_cmd.c`)
Upon seeing `DONE`, the parser reduction fires:
```c
$$ = make_while_command ($2, $4);
```
*   `$2` is the AST for `read line`.
*   `$4` is the AST for `echo "Processing: $line"`.
*   `make_while_command` creates a `COMMAND` struct of type `cm_while`. inside it is a `WHILE_COM` struct containing `test` and `action` pointers.

## 3. Phase 2: Parsing the Redirection

After constructing the `while` command, the parser looks ahead.

1.  **Scanner**: Reads `<`.
2.  **Scanner**: Reads `input.txt` (WORD).
3.  **Grammar**: Matches the `redirection` rule:
    ```yacc
    redirection: '<' WORD
    ```
    *   Calls `make_redirection`.
    *   Creates a `REDIRECT` struct:
        *   `instruction`: `r_input_direction` (input redirection).
        *   `redirector`: Standard Input (file descriptor 0) by default.
        *   `redirectee`: "input.txt".

This forms a `redirection_list`.

## 4. Phase 3: The Attachment

The detailed construction happens in the `command` rule reduction (around line 841 in `parse.y`):

```c
command: shell_command redirection_list
        {
          COMMAND *tc = $1;
          if (tc->redirects)
            { /* append to existing list */ }
          else
            tc->redirects = $2;
          $$ = $1;
        }
```

1.  `$1` (`tc`) is the `WHILE` command built in Phase 1.
2.  `$2` is the `< input.txt` redirection built in Phase 2.
3.   The parser attaches the redirection to `tc->redirects`.

## 5. Final AST Structure

The resulting `COMMAND` structure represents the following logic:

```text
COMMAND (Type: cm_while)
|
+-- Redirects: [ 0 < "input.txt" ]  <-- Attached at the top level
|
+-- Value (WHILE_COM):
    |
    +-- Test: COMMAND (read line)
    |
    +-- Action: COMMAND (echo "Processing: $line")
```

When `execute_command` runs this AST:
1.  It sees the `redirects`. It performs the redirection (opening `input.txt` on stdin).
2.  It executes the `while` loop within that redirected context.
3.  The `read` command, using default stdin, now reads from `input.txt`.
