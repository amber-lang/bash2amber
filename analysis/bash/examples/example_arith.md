# Step-by-Step Parsing Story: `(( a + 2 / 5 ))`

This document traces the complete journey of the arithmetic command `(( a + 2 / 5 ))` through the Bash source code.

---

## Phase 1: Lexical Recognition

### Step 1.1: First `(` Encountered

The main lexer in `parse.y` reads input character by character. When it sees `(`, it peeks at the next character.

```
Input stream: (( a + 2 / 5 ))
              ^
              Current position
```

### Step 1.2: Double Paren Detection

The lexer sees `((` and calls `parse_dparen('(')` (line 4900 in parse.y).

```c
// In read_token() when '(' is seen
if (character == '(' && peek_char == '(')
{
  return parse_dparen (character);
}
```

### Step 1.3: parse_dparen() Executes

```c
static int parse_dparen (int c)
{
  // last_read_token is checked - we're at command position
  if (reserved_word_acceptable (last_read_token))
  {
    cmdtyp = parse_arith_cmd (&wval, 0);
    // cmdtyp = 1 (arithmetic command confirmed)
    
    wd = alloc_word_desc ();
    wd->word = wval;  // " a + 2 / 5 "
    wd->flags = W_QUOTED|W_NOSPLIT|W_NOGLOB|W_NOTILDE|W_NOPROCSUB;
    
    yylval.word_list = make_word_list (wd, NULL);
    return (ARITH_CMD);  // Token 285
  }
}
```

### Step 1.4: parse_arith_cmd() Extracts Expression

```c
static int parse_arith_cmd (char **ep, int adddq)
{
  // Called after first '(' consumed, looking for matching '))'
  ttok = parse_matched_pair (0, '(', ')', &ttoklen, P_ARITH);
  // ttok = " a + 2 / 5 )" (content between first (( and ))
  
  c = shell_getc (0);  // Peek next char
  // c = ')' - confirms this is )) not just )
  
  tokstr = xmalloc (ttoklen + 4);
  strncpy (tokstr, ttok, ttoklen - 1);
  tokstr[ttoklen-1] = '\0';
  // tokstr = " a + 2 / 5 "
  
  *ep = tokstr;
  return 1;  // Success - arithmetic command
}
```

**Result**: Token `ARITH_CMD` returned with value `" a + 2 / 5 "`

---

## Phase 2: Grammar Parsing

### Step 2.1: Parser Receives Token

The yacc/bison parser receives `ARITH_CMD` token and matches:

```yacc
arith_command: ARITH_CMD
             { $$ = make_arith_command ($1); }
    ;
```

### Step 2.2: make_arith_command() Creates Structure

```c
COMMAND *make_arith_command (WORD_LIST *exp)
{
  COMMAND *command = xmalloc (sizeof (COMMAND));
  ARITH_COM *temp = xmalloc (sizeof (ARITH_COM));

  temp->flags = 0;
  temp->line = line_number;  // e.g., 1
  temp->exp = exp;  // WORD_LIST containing " a + 2 / 5 "

  command->type = cm_arith;
  command->value.Arith = temp;
  command->redirects = NULL;
  command->flags = 0;

  return command;
}
```

**Result**: `COMMAND` structure of type `cm_arith` created

---

## Phase 3: Command Execution

### Step 3.1: execute_command() Dispatch

When Bash executes the command tree:

```c
// In execute_command_internal()
switch (command->type)
{
  case cm_arith:
    exec_result = execute_arith_command (command->value.Arith);
    break;
  // ...
}
```

### Step 3.2: execute_arith_command() Prepares Evaluation

```c
static int execute_arith_command (ARITH_COM *arith_command)
{
  this_command_name = "((";
  SET_LINE_NUMBER (arith_command->line);

  // Get expression from word list
  new = arith_command->exp;
  exp = new->word->word;  // " a + 2 / 5 "

  // Expand any shell variables in expression
  exp = expand_arith_string (exp, Q_DOUBLE_QUOTES|Q_ARITH);
  // exp = " a + 2 / 5 " (unchanged if 'a' expansion happens in evalexp)

  // Evaluate!
  expresult = evalexp (exp, eflag, &expok);
  
  // Return based on result
  return (expresult == 0 ? EXECUTION_FAILURE : EXECUTION_SUCCESS);
}
```

---

## Phase 4: Expression Evaluation

### Step 4.1: evalexp() Entry Point

```c
intmax_t evalexp (const char *expr, int flags, int *validp)
{
  // expr = " a + 2 / 5 "
  
  noeval = 0;
  c = setjmp_nosigs (evalbuf);  // Error handling setup
  
  val = subexpr (expr);  // Main evaluation
  // val = result of a + 2 / 5
  
  *validp = 1;
  return (val);
}
```

### Step 4.2: subexpr() Initializes Context

```c
static intmax_t subexpr (const char *expr)
{
  // Skip leading whitespace
  for (p = expr; *p && cr_whitespace (*p); p++);
  // p now points to "a + 2 / 5 "

  pushexp ();  // Save context on stack
  expression = savestring (expr);
  tp = expression;  // Token pointer

  curtok = lasttok = 0;
  tokstr = NULL;
  tokval = 0;

  readtok ();  // Get first token
  val = EXP_LOWEST ();  // Start recursive descent = expcomma()

  popexp ();
  return val;
}
```

---

## Phase 5: Tokenization & Recursive Descent

### Step 5.1: First readtok() - Get 'a'

```
Expression: " a + 2 / 5 "
Position:    ^
```

```c
static void readtok (void)
{
  // Skip whitespace
  while (*cp && cr_whitespace (*cp)) cp++;
  // cp points to 'a'

  if (legal_variable_starter ('a'))  // TRUE
  {
    // Scan variable name
    while (legal_variable_char (c)) c = *cp++;
    // tokstr = "a"
    
    // Evaluate variable 'a'
    tokval = expr_streval ("a", 0, &curlval);
    // Let's say a=10, so tokval = 10
    
    curtok = STR;
  }
}
```

**Token 1**: `STR` with `tokstr="a"`, `tokval=10`

### Step 5.2: Recursive Descent Begins

```
Call Stack:
  expcomma()
    └── expassign()
          └── expcond()
                └── explor()
                      └── expland()
                            └── expbor()
                                  └── expbxor()
                                        └── expband()
                                              └── expeq()
                                                    └── expcompare()
                                                          └── expshift()
                                                                └── expaddsub()  ← WE ARE HERE
```

### Step 5.3: expaddsub() Gets First Operand

```c
static intmax_t expaddsub (void)
{
  val1 = expmuldiv ();
  // Descends further, eventually returns 10 (value of 'a')
  // curtok is now '+' after readtok()
```

### Step 5.4: Second readtok() - Get '+'

```
Expression: " a + 2 / 5 "
Position:      ^
```

```c
// In readtok()
// c = '+'
// c1 = '2' (not '+', so not PREINC)
cp--;  // Unget '2'
curtok = '+';  // PLUS token
```

**Token 2**: `PLUS` ('+')

### Step 5.5: expaddsub() Sees Addition

```c
static intmax_t expaddsub (void)
{
  val1 = expmuldiv ();  // val1 = 10
  
  while (curtok == PLUS || curtok == MINUS)
  {
    int op = curtok;  // op = PLUS
    
    readtok ();  // Get next token -> '2'
    val2 = expmuldiv ();  // val2 = result of 2/5
    
    if (op == PLUS)
      val1 += val2;
  }
  return val1;
}
```

### Step 5.6: Third readtok() - Get '2'

```
Expression: " a + 2 / 5 "
Position:        ^
```

```c
// In readtok()
if (DIGIT('2'))  // TRUE
{
  while (ISALNUM (c) || c == '#'...) c = *cp++;
  // Scans '2', stops at ' '
  
  tokval = strlong ("2");  // tokval = 2
  curtok = NUM;
}
```

**Token 3**: `NUM` with `tokval=2`

### Step 5.7: expmuldiv() Gets 2

```c
static intmax_t expmuldiv (void)
{
  val1 = exppower ();
  // exppower() → expunary() → exp0()
  // exp0() sees NUM, returns tokval = 2
  // readtok() is called, gets '/'
  
  // Back in expmuldiv:
  // val1 = 2, curtok = '/'
```

### Step 5.8: Fourth readtok() - Get '/'

```
Expression: " a + 2 / 5 "
Position:          ^
```

```c
// In readtok()
// c = '/'
// Not followed by '=' (so not /=)
curtok = '/';  // DIV token
```

**Token 4**: `DIV` ('/')

### Step 5.9: expmuldiv() Processes Division

```c
static intmax_t expmuldiv (void)
{
  val1 = exppower ();  // val1 = 2

  while (curtok == MUL || curtok == DIV || curtok == MOD)
  {
    int op = curtok;  // op = DIV
    
    readtok ();  // Get '5'
    val2 = exppower ();  // val2 = 5
    
    if (op == DIV)
      val1 = val1 / val2;  // val1 = 2 / 5 = 0
  }
  return val1;  // Returns 0
}
```

### Step 5.10: Fifth readtok() - Get '5'

```
Expression: " a + 2 / 5 "
Position:            ^
```

**Token 5**: `NUM` with `tokval=5`

### Step 5.11: Sixth readtok() - End of Expression

```
Expression: " a + 2 / 5 "
Position:              ^ (trailing whitespace, then end)
```

```c
// In readtok()
while (*cp && cr_whitespace (*cp)) cp++;
// *cp = '\0'

curtok = 0;  // End of expression
```

**Token 6**: `0` (end marker)

---

## Phase 6: Evaluation Completes

### Step 6.1: expmuldiv() Returns

```c
// 2 / 5 = 0 (integer division)
return 0;
```

### Step 6.2: Back to expaddsub()

```c
static intmax_t expaddsub (void)
{
  val1 = 10;  // from 'a'
  
  // After processing val2 = expmuldiv() which returned 0:
  val1 += val2;  // val1 = 10 + 0 = 10
  
  // curtok is now 0 (end), exit while loop
  return val1;  // Returns 10
}
```

### Step 6.3: Unwind to evalexp()

```c
// In subexpr():
val = EXP_LOWEST ();  // val = 10

if (curtok != 0)
  evalerror (...);  // curtok IS 0, so no error

popexp ();
return 10;

// In evalexp():
*validp = 1;
return 10;
```

---

## Phase 7: Execution Result

### Step 7.1: execute_arith_command() Finishes

```c
static int execute_arith_command (ARITH_COM *arith_command)
{
  // ...
  expresult = evalexp (exp, eflag, &expok);
  // expresult = 10, expok = 1
  
  if (expok == 0)
    return (EXECUTION_FAILURE);  // Not taken
  
  // Result is non-zero (10), so SUCCESS
  return (expresult == 0 ? EXECUTION_FAILURE : EXECUTION_SUCCESS);
  // Returns EXECUTION_SUCCESS (0)
}
```

### Step 7.2: Shell Exit Status

```
$ (( a + 2 / 5 ))
$ echo $?
0
```

The command succeeded because `10 + 0 = 10`, which is non-zero.

---

## Summary: Token Stream

| Order | Token Type | Value | Function Location |
|-------|------------|-------|-------------------|
| 1 | STR | "a" (=10) | expaddsub → expmuldiv → exp0 |
| 2 | PLUS | '+' | expaddsub |
| 3 | NUM | 2 | expmuldiv → exp0 |
| 4 | DIV | '/' | expmuldiv |
| 5 | NUM | 5 | expmuldiv → exp0 |
| 6 | 0 | end | expaddsub |

## Evaluation Tree

```
        expaddsub
        /       \
       a         expmuldiv
      (10)       /       \
                2         5
               (/)
              = 0

Result: 10 + 0 = 10
Exit:   SUCCESS (non-zero)
```

## Key Points

1. **Division before addition**: Due to operator precedence, `2 / 5` is evaluated first (= 0)
2. **Integer arithmetic**: `2 / 5 = 0` not `0.4`
3. **Variable lookup**: `a` is resolved via `expr_streval()` calling shell variable functions
4. **Exit status**: Non-zero result → exit status 0 (success)
