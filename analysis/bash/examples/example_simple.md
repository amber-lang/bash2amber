# The Journey of `touch test.txt`: From String to AST

This document traces the path of the simple command `touch test.txt` as it is analyzed by the Bash repository's parser. It covers the transformation from raw characters to the final Abstract Syntax Tree (AST) node.

## 1. Input: The Character Stream

The journey begins with the raw input string:
```bash
touch test.txt
```

**File:** `input.c`

The shell's input machinery (likely `bash_input` or `shell_getc`) reads characters one by one. The lexer will pull from this stream.

## 2. Phase 1: The Lexer (Tokenization)

**File:** `parse.y` (Lexer functions reside here)

The parsing engine (Bison-generated) calls `yylex()` to get the next token.

### Token 1: "touch"
1. `yylex` calls `read_token(READ)`.
2. `read_token` sees a standard character `t`, not a meta-character (like `|` or `&`).
3. It delegates to the heavyweight function `read_token_word(character)`.
4. `read_token_word` consumes characters `t-o-u-c-h`.
   - It checks for quoting (none).
   - It checks for variable expansion (none).
5. It hits a space ` `. This acts as a delimiter.
6. `read_token_word` classifies the string "touch".
   - It is not a reserved word (like `if` or `while`).
   - It is returned as a **WORD** token.
   - `yylval.word` holds a `WORD_DESC` structure containing the string "touch".

### Token 2: "test.txt"
1. `yylex` is called again.
2. It processes the space (and likely eats it or uses it as a separator).
3. `read_token_word` scans `t-e-s-t-.-t-x-t`.
   - It also checks for quoting (none).
   - It checks for variable expansion (none).
4. It hits the newline or EOF.
5. It returns a **WORD** token for "test.txt".

## 3. Phase 2: The Parser (Grammar Rules)

**File:** `parse.y`

The Grammar Rules define how these tokens fit together.

### Step A: The First Word
The grammar receives the first `WORD` ("touch"). It matches the rule:

```yacc
simple_command_element: WORD
        { $$.word = $1; $$.redirect = 0; }
```

It creates a `simple_command_element` (a temporary union structure) holding the word "touch".

Then, it promotes this element to a `simple_command`:

```yacc
simple_command: simple_command_element
        { $$ = make_simple_command ($1, (COMMAND *)NULL, ...); }
```

**Action (`make_cmd.c`):**
- `make_simple_command` is called.
- Since the `current_command` is NULL, it creates a new `COMMAND` struct of type `cm_simple`.
- It creates a `SIMPLE_COM` struct.
- It adds "touch" to the `words` list.

### Step B: The Second Word
The parser receives the second `WORD` ("test.txt"). It matches `simple_command_element` again.

Now, it matches the *recursive* rule for `simple_command`:

```yacc
simple_command: simple_command simple_command_element
        { $$ = make_simple_command ($2, $1, ...); }
```

- `$1` is the existing command ("touch").
- `$2` is the new element ("test.txt").

**Action (`make_cmd.c`):**
- `make_simple_command` is called with the existing command.
- It *prepends* "test.txt" to the `words` linked list.
- **Current State:** The list is `["test.txt", "touch"]`.
  - *Why reverse order?* It's faster to prepend (O(1)) than append (O(N)) to a singly linked list during parsing.

## 4. Phase 3: Finalization (The Cleanup)

**File:** `parse.y` / `make_cmd.c`

The command is finished (newline encountered). The grammar elevates the `simple_command` to a generic `command`:

```yacc
command: simple_command
        { $$ = clean_simple_command ($1); ... }
```

**Action (`clean_simple_command` in `make_cmd.c`):**
- This function takes the `SIMPLE_COM` struct.
- It calls `REVERSE_LIST` on the `words` list.
- The list flips from `["test.txt", "touch"]` to `["touch", "test.txt"]`.
- The `SIMPLE_COM` is now strictly ordered and ready for execution.

## 5. Result: The AST Node

The final output is a `COMMAND` structure:

```c
COMMAND {
    type: cm_simple,
    value: {
        Simple: {
            words: ["touch" -> "test.txt" -> NULL],
            redirects: NULL
        }
    }
}
```

This static structure is now passed to `execute_command` (specifically `execute_simple_command`), where standard expansion and execution procedures take over.
