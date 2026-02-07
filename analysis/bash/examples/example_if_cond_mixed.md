# Analysis: Mixing `[[ ]]` and `[ ]` with External Logical Operators

This document analyzes how the following Bash code is parsed, which mixes conditional commands (`[[ ... ]]`) and test commands (`[ ... ]`) connected via shell-level logical operators (`&&`, `||`).

```bash
number=64
name="admin"

# Mixing [[ ]] and [ ] using external logical operators
if [[ "$number" -eq 64 ]] && [ "$name" == "admin" ] || [[ "$number" -gt 100 ]]; then
    echo "Condition met!"
fi
```

## 1. Overview

This example demonstrates a critical distinction:
*   **Internal Operators**: The `-eq`, `==`, `-gt` operators *inside* `[[ ]]` or `[ ]`.
*   **External Operators**: The `&&` and `||` operators *between* commands in the shell's list syntax.

The internal operators are parsed as part of the condition expression. The external operators are parsed as **command connectors** by the shell's main grammar, linking independent commands together.

## 2. Parsing the Condition (`if` Test)

The grammar rule for `if`:

```yacc
if_command: IF compound_list THEN compound_list FI
```

The **test expression** of the `if` statement is a `compound_list`. This is where the mixing happens.

### Structure of `compound_list`

The `compound_list` grammar (simplified):

```yacc
compound_list: list0 | list1 ;

list1: list1 AND_AND newline_list list1    // Connects with &&
     | list1 OR_OR newline_list list1      // Connects with ||
     | pipeline_command ;

pipeline_command: pipeline ;
pipeline: command | pipeline '|' ... ;

command: simple_command | shell_command ;
shell_command: cond_command | ... ;
```

The key insight is that `AND_AND` (`&&`) and `OR_OR` (`||`) operate on **commands**, not on test expressions.

## 3. Tokenization and Parsing Flow

### Line: `if [[ "$number" -eq 64 ]] && [ "$name" == "admin" ] || [[ "$number" -gt 100 ]]; then`

#### Step 1: `IF`
*   Lexer returns `IF`.

#### Step 2: `[[ "$number" -eq 64 ]]` (First Command)
1.  `[[` is recognized as `COND_START`, sets `PST_CONDCMD`.
2.  `read_token` intercepts, calls `parse_cond_command()`.
3.  Recursive descent parses `"$number" -eq 64` into a `COND_BINARY` node.
4.  `]]` (`COND_END`) terminates the parsing.
5.  Result: A `COMMAND` of type `cm_cond`.

#### Step 3: `&&`
*   Lexer returns `AND_AND`.
*   The Yacc parser recognizes this as connecting two `list1` items.

#### Step 4: `[ "$name" == "admin" ]` (Second Command)
1.  `[` is lexed as a **`WORD`** (it's the command name).
2.  `"$name"`, `==`, `"admin"`, `]` are all lexed as **`WORD`** tokens.
3.  `make_simple_command` creates a `cm_simple` command with words: `[`, `"$name"`, `==`, `"admin"`, `]`.

#### Step 5: `||`
*   Lexer returns `OR_OR`.
*   The Yacc parser recognizes this as connecting `list1` items.

#### Step 6: `[[ "$number" -gt 100 ]]` (Third Command)
1.  Same flow as Step 2.
2.  Result: Another `COMMAND` of type `cm_cond` with a `COND_BINARY` node for `-gt`.

#### Step 7: `;`
*   Terminates the `compound_list`.

#### Step 8: `THEN`
*   Reserved word, signals the true branch.

## 4. AST Construction via `command_connect`

The grammar actions use `command_connect` to link commands with connectors:

```c
// From parse.y line 1287:
list1: list1 AND_AND newline_list list1
       { $$ = command_connect ($1, $4, AND_AND); }
```

This creates a `CONNECTION` structure (type `cm_connection`):

```c
typedef struct connection {
  int ignore;
  COMMAND *first;
  COMMAND *second;
  int connector;  // AND_AND, OR_OR, ';', '&', '|'
} CONNECTION;
```

### Resulting AST for the `if` Test

The test expression forms a tree:

```
                    OR_OR (||)
                   /         \
             AND_AND (&&)     cmd3 (cm_cond: [[ $number -gt 100 ]])
            /        \
 cmd1 (cm_cond)     cmd2 (cm_simple)
 [[ $number -eq 64 ]]    [ $name == admin ]
```

In C structure terms:

```c
COMMAND *test = {
    .type = cm_connection,
    .value.Connection = {
        .connector = OR_OR,
        .first = {
            .type = cm_connection,
            .value.Connection = {
                .connector = AND_AND,
                .first = { .type = cm_cond, ... },    // [[ $number -eq 64 ]]
                .second = { .type = cm_simple, ... }  // [ $name == admin ]
            }
        },
        .second = { .type = cm_cond, ... }  // [[ $number -gt 100 ]]
    }
};
```

## 5. Operator Precedence

### Shell Operator Precedence (External)

In the shell grammar:
*   `&&` and `||` are **left-associative** at the same precedence level (within `list1`).
*   They are evaluated **left to right** with short-circuit semantics.

Given: `A && B || C`
*   Parsed as: `(A && B) || C`
*   If `A` is true and `B` is true, whole expression is true (C is not evaluated).
*   If `A && B` fails, then `C` is evaluated.

### Internal Operator Precedence (Inside `[[ ]]`)

Inside `[[ ]]`, the recursive descent parser handles:
*   `||` (lower precedence)
*   `&&` (higher precedence)

But this is **separate** from the shell's list parsing.

## 6. Key Differences: `[[ ]]` vs `[ ]`

| Aspect | `[[ ... ]]` | `[ ... ]` |
|--------|-------------|-----------|
| **Type** | Shell keyword (compound command) | External/builtin command |
| **Parsing** | Recursive descent → `cm_cond` | Standard word parsing → `cm_simple` |
| **Internal `&&`/`||`** | Parsed at parse time | Passed as literal arguments |
| **Word splitting** | Suppressed for variables | Must quote variables |
| **Glob patterns** | `=~`, `==` pattern matching | No pattern matching |

## 7. Complete AST for the Entire Script

```
cm_connection (;)
├── first: cm_simple (words: ["number=64"]) -- assignment
└── second: cm_connection (;)
    ├── first: cm_simple (words: ["name=admin"]) -- assignment  
    └── second: cm_if
        ├── test: cm_connection (||)
        │         ├── first: cm_connection (&&)
        │         │         ├── first: cm_cond ([[ $number -eq 64 ]])
        │         │         └── second: cm_simple ([ $name == admin ])
        │         └── second: cm_cond ([[ $number -gt 100 ]])
        ├── true_case: cm_simple (words: ["echo", "Condition met!"])
        └── false_case: NULL
```

## 8. Summary

When mixing `[[ ]]` and `[ ]` with external `&&` and `||`:

1.  Each `[[ ... ]]` becomes a **`cm_cond`** command (parsed via recursive descent).
2.  Each `[ ... ]` becomes a **`cm_simple`** command (just a list of words).
3.  The external `&&` and `||` create **`cm_connection`** nodes linking these commands.
4.  The entire connected structure becomes the **test** of the `if` command (`cm_if`).
5.  Execution evaluates the connection tree with **short-circuit** logic.
