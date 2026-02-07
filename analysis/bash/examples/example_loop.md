# The Journey of a Loop: `let` assignments and `for` loops

This document traces the parsing journey of the following Bash code snippet:

```bash
let groceries=("apple" "banana")
for item in "${groceries[@]}"; do
  echo $item
done
```

It explores how the Bash parser handles the seemingly unusual `let` array assignment and the subsequent loop construction.

## 1. Input Processing

The input is read character by character. The lexer (`yylex` in `parse.y`) is responsible for grouping characters into tokens.

## 2. Parsing the First Line: `let groceries=(...)`

This line presents an interesting case. Standard `let` usually expects arithmetic expressions, but the parser handles it specifically to allow assignments.

### Step A: Tokenizing `let`
1.  **Scanner**: Reads `l-e-t`.
2.  **Classification**: It's a standard **WORD**.
3.  **Context Update**: `read_token` calls `command_token_position`. It sees `let` and checks `builtin_address_internal` or string comparison.
    *   Since the token is `let`, the parser sets the `PST_ASSIGNOK` flag in `parser_state`.
    *   **Crucial State**: `parser_state |= PST_ASSIGNOK`. This flag tells the lexer that the *next* word is allowed to be a compound assignment (like `var=(...)`).

### Step B: Tokenizing `groceries=(...)`
1.  **Scanner**: Reads `g-r-o-c-e-r-i-e-s`.
2.  **Assignment Check**: It encounters `=`.
    *   It enters the logic block at `read_token_word` (around line 5653 in `parse.y`) because `PST_ASSIGNOK` is set.
    *   It peeks at the next character: `(`.
3.  **Compound Assignment**:
    *   Because the next char is `(`, it calls `parse_compound_assignment`.
    *   `parse_compound_assignment` consumes everything from `(` to the matching `)`, respecting quotes.
    *   It consumes `"apple"` and `"banana"`.
    *   The entire string `groceries=("apple" "banana")` is constructed as a single token.
4.  **Result**: The token is classified as an **ASSIGNMENT_WORD** (flags include `W_ASSIGNMENT` and `W_COMPASSIGN`).

### Step C: Constructing the Simple Command
The grammar rule `simple_command` matches:
1.  `element` -> `WORD` ("let").
2.  `element` -> `ASSIGNMENT_WORD` ("groceries=(...)").

The parser builds a `simple_command` node with these two words in the list.

## 3. Parsing the Second Line: `for item in ...`

The newline acts as a command separator. The parser now expects a new command.

### Step A: `for` Keyword
1.  **Scanner**: Reads `for`.
2.  **Reserved Word**: `yylex` identifies `for` as a reserved word `FOR`.
3.  **Grammar**: matches `for_command` rule:
    ```yacc
    for_command: FOR WORD newline_list DO compound_list DONE
               | FOR WORD newline_list '{' compound_list '}'
               | FOR WORD ';' newline_list DO compound_list DONE
               | FOR WORD ';' newline_list '{' compound_list '}'
               | FOR WORD IN word_list list_terminator newline_list DO compound_list DONE
               ...
    ```

### Step B: The Loop Variable
1.  **Scanner**: Reads `item`. Returns **WORD**.
2.  **Grammar**: Matches the `WORD` in the `FOR WORD ...` part.

### Step C: The `in` Clause
1.  **Scanner**: Reads `in`. Returns **IN** (reserved word).
2.  **Grammar**: Transforms to the `FOR WORD IN word_list ...` production.

### Step D: The List `"${groceries[@]}"`
1.  **Scanner**: Reads `"${groceries[@]}"`.
    *   It sees `$`.
    *   It parses the variable expansion `${...}`.
    *   It keeps the quotes (double quotes).
    *   Returns a single **WORD** token: `"${groceries[@]}"`.
2.  **Grammar**: This word is added to the `word_list` of the `for` command.

### Step E: Terminator
1.  **Scanner**: Reads `;`. Returns `;`.
2.  **Grammar**: Matches `list_terminator`.

### Step F: The Body Start
1.  **Scanner**: Reads `do`. Returns **DO**.
2.  **Grammar**: Enters the body parsing state (`compound_list`).

## 4. Parsing the Loop Body: `echo $item`

### Step A: Command Parsing
1.  **Scanner**: Reads `echo`. Returns **WORD**.
2.  **Scanner**: Reads `$item`. Returns **WORD** (flagged with `W_HASDOLLAR`).
3.  **Grammar**: Parses as a `simple_command` (`echo`, `$item`).

## 5. Completing the Loop
1.  **Scanner**: Reads newline (separator) and then `done`.
2.  **Scanner**: Returns **DONE**.
3.  **Grammar**:
    *   Matches `DONE` in `for_command`.
    *   Calls `make_for_command` (in `make_cmd.c`).
    *   Constructs a `FOR_COM` structure containing:
        *   `name`: "item"
        *   `map_list`: ["${groceries[@]}"]
        *   `action`: The AST for `echo $item`.

## 6. Final AST Structure

The full parse results in a list of two commands:
1.  **Command 1**: Simple Command (`let`)
    *   Args: `["let", "groceries=(\"apple\" \"banana\")"]`
2.  **Command 2**: For Command
    *   Variable: `item`
    *   Iterate over: `"${groceries[@]}"`
    *   Body: Simple Command (`echo $item`)

This structure is passed to the execution engine (`execute_cmd.c`), which will first run the `let` builtin (processing the assignment) and then execute the loop.
