# Amber Language Reference for AI

Amber is a modern programming language that compiles to Bash. It provides type safety, error handling, and clean syntax while generating portable shell scripts.

## Quick Start

```ab
#!/usr/bin/env amber
echo("Hello, world!")
```

Run: `amber run hello.ab`  
Compile: `amber build input.ab output.sh`

## Core Concepts: Rosetta Stone (Bash → Amber)

| Bash | Amber |
|------|-------|
| `echo "Hello"` | `echo("Hello")` |
| `NAME="John"` | `let name = "John"` |
| `readonly PI=3.14` | `const PI = 3.14` |
| `echo "Hi $NAME"` | `echo("Hi {name}")` |
| `arr=(1 2 3)` | `let arr = [1, 2, 3]` |
| `${arr[0]}` | `arr[0]` |
| `${#arr[@]}` | `len(arr)` |
| `if [[ $x -gt 5 ]]; then ... fi` | `if x > 5 { ... }` |
| `for i in "${arr[@]}"; do ... done` | `for item in arr { ... }` |
| `function foo() { ... }` | `fun foo() { ... }` |
| `$(command)` | `$ command $` |
| `if command; then` | `$ command $ succeeded { ... }` |
| `command \|\| echo "failed"` | `$ command $ failed { echo("failed") }` |
| `exit 1` | `exit(1)` |
| `source file.sh` | `import * from "file.ab"` |

---

## Data Types

| Type | Description | Example |
|------|-------------|---------|
| `Text` | String | `"Hello"` |
| `Int` | Integer (64-bit signed) | `42`, `-7` |
| `Num` | Float (requires `bc`) | `3.14` |
| `Bool` | Boolean | `true`, `false` |
| `Null` | Nothing | `null` |
| `[T]` | Array of T | `[1, 2, 3]`, `["a", "b"]` |

**Type inference:** Amber infers types. Specify when needed:
```ab
fun add(a: [], b: []) {
    return a + b
}
add([1], [2]) // `a` and `b` inside are of type `[Int]`
```

**Union types (function params only):**
```ab
fun print_val(val: Int | Text | Bool) {
    echo(val)
}
```

---

## Variables

```ab
let name = "Alice"         // Mutable
const PI = 3.14159         // Immutable
name = "Bob"               // Reassign mutable
let result = 12.5          // Num inferred
let items = [1, 2, 3]      // [Int] inferred
let empty = []             // Type resolved on use
empty += [1]               // Now [Int]
```

## Text & Interpolation

```ab
let name = "World"
echo("Hello, {name}!")           // Hello, World!
echo("1 + 1 = {1 + 1}")          // 1 + 1 = 2
echo("Items: {[1,2,3]}")         // Items: 1 2 3
echo("Flag: {true}")             // Flag: 1
```

Escape sequences: `\n`, `\t`, `\{`, `\\`, `\"`, `\$`

## Arrays

```ab
let arr = [1, 2, 3]
arr[0] = 10                      // Modify
echo(arr[1])                     // Access: 2
echo(len(arr))                   // Length: 3
arr += [4, 5]                    // Append
echo(arr[1..3])                  // Slice: 2 3
let [a, b, c] = arr              // Destructure
```

Ranges:
```ab
0..5      // [0, 1, 2, 3, 4]
0..=5     // [0, 1, 2, 3, 4, 5]
5..2      // [5, 4, 3]
```

## Operators

Arithmetic: `+`, `-`, `*`, `/`, `%`  
Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`  
Logical: `and`, `or`, `not`  
Shorthand: `+=`, `-=`, `*=`, `/=`, `%=`

```ab
let x = 10 + 5 * 2               // 20
let ok = x > 15 and x < 25       // true
x += 5                           // x = 25
```

---

## Conditionals

```ab
// Standard if
if age >= 18 {
    echo("Adult")
} else {
    echo("Minor")
}

// Single-line
if ready: echo("Go!")
else: echo("Wait")

// If-chain (switch-like)
if {
    x == 1: echo("One")
    x == 2: echo("Two")
    else: echo("Other")
}

// Ternary
let label = count > 1 then "items" else "item"
```

---

## Loops

```ab
// Infinite loop
loop {
    if done: break
    continue
}

// For-each
for item in items {
    echo(item)
}

// With index
for i, item in items {
    echo("{i}: {item}")
}

// Range iteration
for n in 0..10 {
    echo(n)
}

// While loop
while x < 100 {
    x *= 2
}
```

---

## Functions

```ab
// Generic function
fun greet(name) {
    echo("Hello, {name}")
}

// Typed function
fun add(a: Int, b: Int): Int {
    return a + b
}

// Default parameters
fun power(base: Int, exp: Int = 2): Int {
    // ...
}

// Reference parameter (mutates original)
fun push(ref arr, val) {
    arr += [val]
}

// Failable function
fun divide(a: Num, b: Num): Num? {
    if b == 0: fail 1
    return a / b
}
```

---

## Commands (Shell Execution)

```ab
// Basic command with error handling
$ ls -la $ failed {
    echo("Command failed")
}

// Capture output
let files = $ ls $ failed { echo("Error") }

// Interpolation in commands
let dir = "/tmp"
$ mkdir -p {dir}/test $ failed { echo("Failed") }

// Error propagation
$ risky_command $?

// Access exit code
let result = $ some_cmd $ failed(code) {
    echo("Exit code: {code}")
}

// Success handler
$ test -f file.txt $ succeeded {
    echo("File exists")
}

// Always-run handler
$ command $ exited(code) {
    echo("Finished with: {code}")
}
```

**Modifiers:**
```ab
silent $ noisy_command $         // Suppress stdout
trust $ may_fail $               // Ignore failures (discouraged unless this makes total sense)
silent trust {                   // Modifier scope
    $ cmd1 $
    $ cmd2 $
}
```

Escape sequences in commands are similar to `Text` literal.

## Imports & Modules

```ab
// Import specific functions
import { split, join } from "std/text"

// Import all
import * from "std/fs"

// Public function (exportable)
pub fun my_utility() { ... }

// Public re-export
pub import * from "other.ab"
```

## Main Block & Entry Point

```ab
main {
    $ risky_command $?           // Propagate errors to shell
    echo("Script complete")
}

// With arguments
main(args) {
    for arg in args {
        echo(arg)
    }
}
```

## Error Handling

```ab
// Failable function
fun parse_data(input: Text): Int? {
    if input == "": fail 1
    return parse_int(input)?
}

// Handle failure
let num = parse_data(text) failed {
    echo("Parse error")
}

// Propagate failure
let result = risky_function()?

// Status code access
trust $ command $
if status != 0 {
    echo("Failed with: {status}")
}
```

## Type Casting

```ab
// Safe casts
let flag: Bool = true
let num = flag as Int            // 1

// Absurd casts (use with caution)
let str = "42"
let n = str as Int               // Warning: prefer parse_int()
```

## Builtins

| Builtin | Usage | Description |
|---------|-------|-------------|
| `echo(x)` | `echo("Hi")` | Print to stdout |
| `cd(path)` | `cd("/tmp")` | Change directory |
| `exit(code)` | `exit(1)` | Exit with code |
| `len(x)` | `len(arr)`, `len(str)` | Length |
| `lines(file)` | `for l in lines("f.txt")` | Read file lines |
| `mv(a, b)` | `mv("old", "new")` | Move/rename (failable) |
| `nameof(var)` | `nameof(x)` | Get compiled variable name (use for referencing Amber's variables in commands!) |
| `status` | `if status != 0` | Last exit code |

## Testing

```ab
import { assert, assert_eq } from "std/test"

test "arithmetic works" {
    assert(2 + 2 == 4)
    assert_eq(3 * 3, 9)
}

test {
    // Unnamed test
    assert(true)
}
```

Run tests: `amber test` or `amber test file.ab "filter"`

## Standard Library Cheat Sheet

### `std/text`
```ab
// Idiomatic way is to keep up to 3 imports per line
import {
    split, join, trim,
    uppercase, lowercase, replace,
    starts_with, ends_with, text_contains,
    parse_int, parse_num, slice,
    capitalized, reversed
} from "std/text"

split("a,b,c", ",")              // ["a", "b", "c"]
join(["a", "b"], "-")            // "a-b"
trim("  hi  ")                   // "hi"
uppercase("hi")                  // "HI"
lowercase("HI")                  // "hi"
replace("foo bar", "bar", "baz") // "foo baz"
starts_with("hello", "he")       // true
ends_with("hello", "lo")         // true
text_contains("hello", "ll")     // true
parse_int("42")?                 // 42
parse_num("3.14")?               // 3.14
slice("hello", 1, 3)             // "ell"
capitalized("hello")             // "Hello"
reversed("abc")                  // "cba"
```

### `std/array`
```ab
import {
    array_contains, array_find, array_first,
    array_last, array_pop, array_shift,
    sort, sorted
} from "std/array"

array_contains([1,2,3], 2)       // true
array_find([1,2,3], 2)           // 1 (index)
array_first([1,2,3])?            // 1
array_last([1,2,3])?             // 3
let arr = [1,2,3]
array_pop(arr)?                  // 3, arr = [1,2]
sort(arr)                        // In-place sort
sorted([3,1,2])                  // [1,2,3] (returns new)
```

### `std/fs`
```ab
import {
    file_read, file_write, file_append,
    file_exists, dir_exists, dir_create,
    file_chmod
} from "std/fs"

let content = file_read("file.txt")?
file_write("out.txt", "data")?
file_append("log.txt", "entry")?
if file_exists("x.txt") { ... }
if dir_exists("/tmp") { ... }
dir_create("/tmp/new")?
file_chmod("script.sh", "755")?
```

### `std/env`
```ab
import {
    env_var_get, env_var_set, is_command,
    is_root, input_prompt, input_confirm,
    echo_info, echo_error
} from "std/env"

let path = env_var_get("PATH")?
env_var_set("DEBUG", "1")?
if is_command("git") { ... }
if is_root() { ... }
let name = input_prompt("Name: ")
if input_confirm("Continue?") { ... }
echo_info("Info message")
echo_error("Error!", 1)
```

### `std/http`
```ab
import { fetch, file_download } from "std/http"

let resp = fetch("https://api.example.com")?
let post = fetch(url, "POST", "data", ["content-type: application/json"])?
file_download("https://example.com/file.zip", "/tmp/file.zip")?
```

### `std/date`
```ab
// Importing up to 4 imports a line is fine too
import { date_now, date_add, date_format_posix, date_from_posix } from "std/date"

let now = date_now()
let future = date_add(now, 7, "days")?
let formatted = date_format_posix(now, "%Y-%m-%d")?
let parsed = date_from_posix("2024-01-15", "%Y-%m-%d")?
```

### `std/math`
```ab
import {
    math_abs, math_floor, math_ceil,
    math_round, math_sum
} from "std/math"

math_abs(-5)                     // 5
math_floor(3.7)                  // 3
math_ceil(3.2)                   // 4
math_round(3.5)                  // 4
math_sum([1,2,3,4])              // 10
```

## EBNF Grammar (Key Constructs)

```ebnf
(* Core types *)
TYPE = 'Text' | 'Num' | 'Bool' | 'Null' | 'Int' | '[', TYPE, ']' ;

(* Variables *)
variable_init = ('let' | 'const'), identifier, '=', expression ;

(* Functions *)
function_def = ['pub'], 'fun', identifier, '(', [params], ')', [':', TYPE, ['?']], block ;

(* Commands *)
command = [modifier], '$', {char | interpolation}, '$', [handler] ;
handler = 'failed', ['(', id, ')'], block 
        | 'succeeded', block 
        | 'exited', '(', id, ')', block 
        | '?' ;

(* Conditionals *)
if_statement = 'if', expression, block, ['else', block] ;
if_chain = 'if', '{', {expression, block}, ['else', block], '}' ;
ternary = expression, 'then', expression, 'else', expression ;

(* Loops *)
loop = 'loop', block ;
for_loop = 'for', [id, ','], id, 'in', expression, block ;
while_loop = 'while', expression, block ;

(* Imports *)
import = ['pub'], 'import', ('*' | '{', ids, '}'), 'from', string ;

(* Main entry *)
main = 'main', ['(', identifier, ')'], block ;

(* Keywords *)
KEYWORDS = 'let' | 'const' | 'if' | 'else' | 'for' | 'in' | 'loop' | 'while' |
           'fun' | 'pub' | 'return' | 'fail' | 'import' | 'from' | 'main' |
           'and' | 'or' | 'not' | 'true' | 'false' | 'null' | 'as' | 'is' |
           'then' | 'ref' | 'break' | 'continue' | 'silent' | 'trust' |
           'failed' | 'succeeded' | 'exited' | 'status' | 'test' ;
```

## Common Patterns

### File Processing
```ab
import { file_read, file_write } from "std/fs"
import { split, join, uppercase } from "std/text"

let content = file_read("input.txt")?
let lines_arr = split(content, "\n")
let upper_lines = [Text]
for line in lines_arr {
    upper_lines += [uppercase(line)]
}
file_write("output.txt", join(upper_lines, "\n"))?
```

### CLI Tool
```ab
main(args) {
    if len(args) < 1 {
        echo("Usage: tool <command>")
        exit(1)
    }
    
    if {
        args[0] == "help": echo("Help text")
        args[0] == "run": run_command()?
        else {
            echo("Unknown command: {args[0]}")
            exit(1)
        }
    }
}
```

### Safe Command Execution
```ab
fun run_safe(cmd: Text): Text? {
    let output = $ {cmd} $ failed(code) {
        echo("Command failed: {code}")
        fail code
    }
    return output
}
```

### Retry Logic
```ab
fun with_retry(max: Int): Bool {
    for attempt in 1..=max {
        $ risky_operation $ succeeded {
            return true
        }
        echo("Attempt {attempt} failed, retrying...")
    }
    return false
}
```

## Quick Reference Card

```
DECLARE:     let x = 1           const Y = 2
TYPES:       Text Int Num Bool Null [T]
PRINT:       echo("text")
STRING:      "Hello {name}"
ARRAY:       [1,2,3]  arr[0]  len(arr)  arr += [4]
RANGE:       0..5  0..=5
IF:          if cond { } else { }
IF-CHAIN:    if { cond1: stmt1  cond2: stmt2  else: stmt3 }
TERNARY:     cond then val1 else val2
LOOP:        loop { break }
FOR:         for item in arr { }  for i, item in arr { }
WHILE:       while cond { }
FUNCTION:    fun name(a, b) { return x }
TYPED FN:    fun name(a: Int): Int { }
FAILABLE:    fun name(): Int? { fail 1 }
COMMAND:     $ cmd $ failed { }
PROPAGATE:   $ cmd $?  or  func()?
IMPORT:      import { fn } from "std/mod"
MAIN:        main(args) { }
TEST:        test "name" { assert(cond) }
```
