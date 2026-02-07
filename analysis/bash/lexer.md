# Bash Lexer Analysis

## 1. Overview
The Bash lexer is NOT a standalone component (like a Flex/Lex specification). It is a hand-written, state-heavy scanner integrated tightly with the parser (`parse.y`). It resides primarily in `parse.y` (historically linked to `y.tab.c`) and operates on a character stream provided by `input.c`.

**Key Characteristics:**
- **Context-Sensitive:** Tokenization depends heavily on the parser state (e.g., whether we are inside a `case` statement, a here-document, or a command substitution).
- **Recursive:** The lexer calls back into itself recursively to parse nested constructs like command substitutions `$(...)` or arithmetic expansions `$((...))`.
- **Integrated Alias Expansion:** Alias expansion happens at the character input layer (`shell_getc`) but is triggered and managed by the lexer's state.

## 2. Core Components

### 2.1 Entry Point: `yylex`
The `yylex` function is the standard interface called by the Bison parser.
- It maintains a small history of tokens (`two_tokens_ago`, `token_before_that`, `last_read_token`) to help with context decisions (e.g., identifying keywords vs. identifiers).
- It calls `read_token(READ)` to fetch the actual next token.

### 2.2 The Dispatcher: `read_token`
`read_token` is the primary switchboard.
- **Resets**: Handles `RESET` command to clear parser state (e.g., on Ctrl-C).
- **Lookahead**: Checks `token_to_read` (a single-token lookahead buffer used for specific grammar hacks).
- **Meta-characters**: It scans for shell meta-characters (`|`, `&`, `;`, `(`, `)`, `<`, `>`).
  - Identifies multi-character operators like `&&`, `||`, `;;`, `<<`, `>>`.
  - Handles process substitution `<(...)` and `>(...)`.
- **Delegation**: If the character is not a meta-character, it jumps to `read_token_word`.

### 2.3 The Worker: `read_token_word`
This is the most complex function (thousands of lines). It accumulates characters into a token string until a delimiter is hit.
- **Quoting**: Handles single quotes `'`, double quotes `"`, and backslashes `\`.
- **Expansions**: Detects `$`, `` ` ``, and triggers recursive parsing for:
  - `${...}` (Parameter expansion)
  - `$(...)` (Command substitution)
  - `$((...))` (Arithmetic expansion)
  - `[...]` (Array subscripts)
- **Word Classification**: Determines if the scanned word is a `NUMBER`, `WORD`, or `ASSIGNMENT_WORD`.
- **Keyword Check**: Checks if the word is a reserved word (like `if`, `while`, `function`) using `CHECK_FOR_RESERVED_WORD`, but ONLY if the position allows it (start of command).

## 3. State Management

The lexer uses several mechanisms to track "where we are":

### 3.1 `parser_state`
A global bitmask integer that fundamentally changes how tokens are read. Key flags include:
- `PST_CASEPAT`: We are parsing a pattern in a `case` statement (pipes `|` are treated as pattern separators, not pipeline operators).
- `PST_HEREDOC`: We are reading a here-document body.
- `PST_CMDSUBST`: We are inside a command substitution.
- `PST_CONDCMD`: We are inside `[[ ... ]]` (keywords change meanings, pattern matching is active).
- `PST_EXTPAT`: Extended globbing is active.

### 3.2 `dstack` (Delimiter Stack)
A stack of characters (`(`, `{`, `'`, `"`, `` ` ``) used to handle nested quoting and grouping.
- Used to know when a nested construct ends (e.g., finding the matching `}` for `${...}`).

### 3.3 `pushed_string_list` & Alias Expansion
Aliases are handled by "pushing" a string onto the input stack.
- When `shell_getc` reaches the end of the current input, it pops the stack.
- This allows an alias like `alias foo='ls -la'` to seamlessly insert `ls -la` into the token stream.

## 4. Complex Flows

### 4.1 Recursive Parsing (`parse_matched_pair`)
When `read_token_word` encounters an opening construct like `${`, it calls `parse_matched_pair`.
- This function enters a mini-loop, consuming characters and respecting quotes/nesting until the matching delimiter is found.
- The entire substring is returned as part of the current `WORD` token. The parser does *not* see the inside of `${...}` as separate tokens; it sees one giant `WORD`.

### 4.2 Code Injection (Command Substitution)
For `$(...)` or `` `...` ``, the lexer extracts the body.
- Interestingly, `parse_comsub` is used.
- The content is treated as a single word by the outer parser.
- Execution happens later during expansion, where the string is fed back into a *new* parser instance.

### 4.3 Here-Documents
Here-docs are one of the trickiest parts.
1. `read_token` sees `<<`.
2. It parses the delimiter word.
3. It pushes the delimiter onto `redir_stack`.
4. The parser continues until the *end of the current command line* (newline).
5. `read_token` detects the newline and calls `gather_here_documents`.
6. `gather_here_documents` reads raw lines from input until the delimiter is matched, saving them aside.

## 5. Summary of Files
- **`parse.y`**: Contains the grammar (Bison) AND the lexer (`yylex`, `read_token`, `read_token_word`).
- **`input.c` / `input.h`**: Low-level character input, buffering, and encoding.
- **`shell.h`**: Defines key structures like `WORD_DESC`.
- **`shmbutil.h`**: Multibyte character handling macros (MBTEST).
