# Bash Arithmetic Operations Parsing - Deep Research

This document provides a comprehensive analysis of how arithmetic operations like `(( a + 2 ))` are parsed and evaluated in Bash, based on the Bash source code.

## Overview

Bash supports two primary forms of arithmetic:
1. **Arithmetic Commands**: `(( expression ))` - evaluates expression and sets exit status
2. **Arithmetic Expansion**: `$(( expression ))` - evaluates expression and substitutes result

The parsing pipeline involves multiple stages:
1. **Lexical Analysis** (tokenization) - `parse.y`
2. **Grammar Rules** (syntax tree construction) - `parse.y`
3. **Command Structure Creation** - `make_cmd.c`, `command.h`
4. **Expression Evaluation** - `expr.c`
5. **Command Execution** - `execute_cmd.c`

---

## 1. Lexical Analysis (parse.y)

### Entry Point: `parse_dparen()`

When the parser encounters `((`, the function `parse_dparen()` (line 4900 of `parse.y`) handles recognition:

```c
static int
parse_dparen (int c)
{
  int cmdtyp, sline;
  char *wval;
  WORD_DESC *wd;

#if defined (ARITH_FOR_COMMAND)
  if (last_read_token == FOR)
    {
      // Handle for (( init; test; step )) loops
      arith_for_lineno = compoundcmd_top[compoundcmd_lineno].lineno;
      cmdtyp = parse_arith_cmd (&wval, 0);
      if (cmdtyp == 1)
        return (ARITH_FOR_EXPRS);
      else
        return -1;  // ERROR
    }
#endif

#if defined (DPAREN_ARITHMETIC)
  if (reserved_word_acceptable (last_read_token))
    {
      cmdtyp = parse_arith_cmd (&wval, 0);
      if (cmdtyp == 1)  // arithmetic command
        {
          wd = alloc_word_desc ();
          wd->word = wval;
          wd->flags = W_QUOTED|W_NOSPLIT|W_NOGLOB|W_NOTILDE|W_NOPROCSUB;
          yylval.word_list = make_word_list (wd, (WORD_LIST *)NULL);
          return (ARITH_CMD);  // Token #285
        }
      else if (cmdtyp == 0)  // nested subshell - backwards compatibility
        {
          push_string (wval, 0, (alias_t *)NULL);
          return (c);  // Return '(' for subshell parsing
        }
    }
#endif
  return -2;
}
```

### Parsing the Expression: `parse_arith_cmd()`

This function (line 4962) finds the matching `))`:

```c
static int
parse_arith_cmd (char **ep, int adddq)
{
  int exp_lineno, rval, c;
  char *ttok, *tokstr;
  size_t ttoklen;

  exp_lineno = line_number;
  ttok = parse_matched_pair (0, '(', ')', &ttoklen, P_ARITH);
  rval = 1;
  
  if (ttok == &matched_pair_error)
    return -1;
    
  // Check that next character is the closing right paren
  c = shell_getc (0);
  if MBTEST(c != ')')
    rval = 0;  // Not arithmetic - treat as nested subshell

  // Build the token string (expression content)
  tokstr = (char *)xmalloc (ttoklen + 4);
  if (rval == 1)  // arithmetic command
    {
      strncpy (tokstr, ttok, ttoklen - 1);
      tokstr[ttoklen-1] = '\0';
    }
  // ... handle subshell case
  
  *ep = tokstr;
  return rval;
}
```

Key flag `P_ARITH` (0x0080) tells `parse_matched_pair()` this is arithmetic context.

---

## 2. Grammar Rules (parse.y)

### Token Definition

```yacc
%token <word_list> ARITH_CMD ARITH_FOR_EXPRS
```

Token `ARITH_CMD` has value 285.

### Grammar Production

Line 1203 defines the grammar rule:

```yacc
arith_command: ARITH_CMD
              { $$ = make_arith_command ($1); }
    ;
```

`arith_command` is part of `shell_command`:

```yacc
shell_command: for_command
    | case_command
    | while_command DO compound_list DONE
    | select_command
    | if_command
    | subshell
    | group_command
    | arith_command      /* (( ... )) */
    | cond_command       /* [[ ... ]] */
    | arith_for_command  /* for (( ; ; )) */
    ;
```

---

## 3. Command Structure (command.h, make_cmd.c)

### ARITH_COM Structure

Defined in `command.h` (line 312):

```c
#if defined (DPAREN_ARITHMETIC)
typedef struct arith_com {
  int flags;
  int line;
  WORD_LIST *exp;  /* Expression as word list */
} ARITH_COM;
#endif
```

### Command Type Enum

```c
enum command_type { 
  cm_for, cm_case, cm_while, cm_if, cm_simple, cm_select,
  cm_connection, cm_function_def, cm_until, cm_group,
  cm_arith,      /* Arithmetic command */
  cm_cond, cm_arith_for, cm_subshell, cm_coproc 
};
```

### Command Union

```c
typedef struct command {
  enum command_type type;
  int flags;
  int line;
  REDIRECT *redirects;
  union {
    // ... other types ...
    struct arith_com *Arith;      /* (( )) commands */
    struct arith_for_com *ArithFor; /* for (( ; ; )) */
    // ...
  } value;
} COMMAND;
```

### make_arith_command() (make_cmd.c, line 388)

```c
COMMAND *
make_arith_command (WORD_LIST *exp)
{
#if defined (DPAREN_ARITHMETIC)
  COMMAND *command;
  ARITH_COM *temp;

  command = (COMMAND *)xmalloc (sizeof (COMMAND));
  command->value.Arith = temp = (ARITH_COM *)xmalloc (sizeof (ARITH_COM));

  temp->flags = 0;
  temp->line = line_number;
  temp->exp = exp;

  command->type = cm_arith;
  command->redirects = (REDIRECT *)NULL;
  command->flags = 0;

  return (command);
#else
  set_exit_status (2);
  return ((COMMAND *)NULL);
#endif
}
```

---

## 4. Expression Evaluation (expr.c)

### Overview

The expression evaluator in `expr.c` is a **recursive-descent parser** implementing full C-like arithmetic with these features:

- **Data type**: All arithmetic uses `intmax_t` (64-bit integers)
- **No overflow checking** (except division by zero)
- **Variable expansion** with shell integration
- **Array subscript support**

### Operator Precedence (Highest to Lowest)

| Level | Operators | Function | Description |
|-------|-----------|----------|-------------|
| 1 | `id++` `id--` | `exp0()` | Post-increment/decrement |
| 2 | `++id` `--id` | `exp0()` | Pre-increment/decrement |
| 3 | `-` `+` `!` `~` | `expunary()` | Unary operators |
| 4 | `**` | `exppower()` | Exponentiation (right-assoc) |
| 5 | `*` `/` `%` | `expmuldiv()` | Multiplication, Division, Modulo |
| 6 | `+` `-` | `expaddsub()` | Addition, Subtraction |
| 7 | `<<` `>>` | `expshift()` | Bit shifts |
| 8 | `<` `<=` `>` `>=` | `expcompare()` | Relational |
| 9 | `==` `!=` | `expeq()` | Equality |
| 10 | `&` | `expband()` | Bitwise AND |
| 11 | `^` | `expbxor()` | Bitwise XOR |
| 12 | `\|` | `expbor()` | Bitwise OR |
| 13 | `&&` | `expland()` | Logical AND (short-circuit) |
| 14 | `\|\|` | `explor()` | Logical OR (short-circuit) |
| 15 | `?:` | `expcond()` | Ternary conditional |
| 16 | `=` `+=` `-=` etc. | `expassign()` | Assignment (right-assoc) |
| 17 | `,` | `expcomma()` | Comma (lowest) |

### Main Entry Point: `evalexp()`

```c
intmax_t
evalexp (const char *expr, int flags, int *validp)
{
  intmax_t val;
  int c;
  procenv_t oevalbuf;

  val = 0;
  noeval = 0;
  already_expanded = (flags & EXP_EXPANDED);

  FASTCOPY (evalbuf, oevalbuf, sizeof (evalbuf));

  c = setjmp_nosigs (evalbuf);  // Error handling setup

  if (c)  // Error occurred
    {
      FREE (tokstr);
      FREE (expression);
      expr_unwind ();
      if (validp) *validp = 0;
      return (0);
    }

  val = subexpr (expr);  // Main evaluation

  if (validp) *validp = 1;
  return (val);
}
```

### Sub-expression Evaluation: `subexpr()`

```c
static intmax_t
subexpr (const char *expr)
{
  intmax_t val;

  // Skip leading whitespace
  for (p = expr; p && *p && cr_whitespace (*p); p++) ;
  if (p == NULL || *p == '\0') return (0);

  pushexp ();           // Save context on stack
  expression = savestring (expr);
  tp = expression;

  curtok = lasttok = 0;
  tokstr = (char *)NULL;
  tokval = 0;

  readtok ();           // Get first token
  val = EXP_LOWEST ();  // Start recursive descent (expcomma)

  if (curtok != 0)
    evalerror (_("arithmetic syntax error in expression"));

  popexp ();            // Restore context
  return val;
}
```

### Lexical Analyzer: `readtok()`

The tokenizer (line 1318) handles:

```c
static void
readtok (void)
{
  // Skip whitespace (including newlines for $((...)))
  while (cp && (c = *cp) && cr_whitespace (c)) cp++;

  if (c == '\0') { curtok = 0; return; }

  // Variable names (identifiers)
  if (legal_variable_starter (c))
    {
      // Scan identifier
      while (legal_variable_char (c)) c = *cp++;
      
      // Handle array subscripts: var[index]
      if (c == '[')
        {
          e = expr_skipsubscript (tp, cp);
          if (cp[e] == ']') { cp += e + 1; e = ']'; }
        }
      
      tokstr = savestring (tp);  // Save variable name
      tokval = expr_streval (tokstr, e, &curlval);  // Evaluate
      curtok = STR;
    }
  // Numbers (including bases like 16#FF, 0xFF, 0777)
  else if (DIGIT(c))
    {
      while (ISALNUM (c) || c == '#' || c == '@' || c == '_') c = *cp++;
      tokval = strlong (tp);
      curtok = NUM;
    }
  // Operators
  else
    {
      c1 = *cp++;
      if (c == '=' && c1 == '=')      c = EQEQ;   // ==
      else if (c == '!' && c1 == '=') c = NEQ;    // !=
      else if (c == '<' && c1 == '<') c = LSH;    // <<
      else if (c == '>' && c1 == '>') c = RSH;    // >>
      else if (c == '&' && c1 == '&') c = LAND;   // &&
      else if (c == '|' && c1 == '|') c = LOR;    // ||
      else if (c == '*' && c1 == '*') c = POWER;  // **
      else if (c == '+' && c1 == '+') c = PREINC/POSTINC;
      else if (c == '-' && c1 == '-') c = PREDEC/POSTDEC;
      else if (c1 == '=' && member(c, "*/%+-&^|"))
        { assigntok = c; c = OP_ASSIGN; }  // +=, -=, etc.
      // ... single-char operators
      curtok = c;
    }
}
```

### Number Parsing: `strlong()`

Supports multiple bases (line 1551):

```c
static intmax_t
strlong (char *num)
{
  // 0nnn      -> octal (base 8)
  // 0xNN      -> hexadecimal (base 16)
  // base#num  -> arbitrary base (2-64)
  
  // Base > 36: uses 0-9, a-z, A-Z, @, _
  // Base <= 36: case-insensitive letters
}
```

### Variable Evaluation: `expr_streval()`

```c
static intmax_t
expr_streval (char *tok, int e, struct lvalue *lvalue)
{
  SHELL_VAR *v;
  char *value;
  intmax_t tval;

  if (noeval) return (0);  // Short-circuit for conditional branches

  // Find the variable
  v = (e == ']') ? array_variable_part (tok, ...) : find_variable (tok);

  // Handle unbound variable error
  if ((v == 0 || invisible_p (v)) && unbound_vars_is_error)
    {
      err_unboundvar (value);
      // ... error handling
    }

  // Get variable value
  value = (e == ']') ? get_array_value (tok, ...) : get_variable_value (v);

  // Recursively evaluate if value contains expression
  tval = (value && *value) ? subexpr (value) : 0;

  // Store lvalue info for assignment
  if (lvalue)
    {
      lvalue->tokstr = tok;
      lvalue->tokval = tval;
      lvalue->tokvar = v;
      lvalue->ind = ind;  // Array index if applicable
    }

  return (tval);
}
```

### Expression Context Stack

The evaluator maintains a context stack for nested expressions:

```c
typedef struct {
  int curtok, lasttok;
  char *expression, *tp, *lasttp;
  intmax_t tokval;
  char *tokstr;
  int noeval;
  struct lvalue lval;
} EXPR_CONTEXT;

static EXPR_CONTEXT **expr_stack;
static int expr_depth;
#define MAX_EXPR_RECURSION_LEVEL 1024
```

---

## 5. Command Execution (execute_cmd.c)

### execute_arith_command() (line 3893)

```c
static int
execute_arith_command (ARITH_COM *arith_command)
{
  int expok, save_line_number, retval, eflag;
  intmax_t expresult;
  WORD_LIST *new;
  char *exp, *t;

  expresult = 0;
  save_line_number = line_number;
  this_command_name = "((";  /* )) for balance */

  SET_LINE_NUMBER (arith_command->line);
  ADJUST_LINE_NUMBER ();

  // Print command for debugging/xtrace
  command_string_index = 0;
  print_arith_command (arith_command->exp);

  // Run DEBUG trap
  retval = run_debug_trap ();

  // Get expression string from word list
  t = (char *)NULL;
  new = arith_command->exp;
  exp = (new->next) ? (t = string_list (new)) : new->word->word;

  // Expand variables in expression
  exp = expand_arith_string (exp, Q_DOUBLE_QUOTES|Q_ARITH);
  FREE (t);

  // Xtrace output
  if (echo_command_at_execute)
    {
      new = make_word_list (make_word (exp ? exp : ""), (WORD_LIST *)NULL);
      xtrace_print_arith_cmd (new);
      dispose_words (new);
    }

  // Evaluate the expression
  if (exp)
    {
      eflag = (shell_compatibility_level > 51) ? 0 : EXP_EXPANDED;
      expresult = evalexp (exp, eflag, &expok);
      free (exp);
    }
  else
    {
      expresult = 0;
      expok = 1;
    }

  line_number = save_line_number;

  if (expok == 0)
    return (EXECUTION_FAILURE);

  // Return: 0 (success) if non-zero, 1 (failure) if zero
  return (expresult == 0 ? EXECUTION_FAILURE : EXECUTION_SUCCESS);
}
```

---

## 6. Arithmetic Expansion: $(()) in subst.c

### Handling in Parameter Expansion

In `subst.c` line 10832, when `$(` is followed by `(`:

```c
case LPAREN:
  temp = extract_command_subst (string, &t_index, ...);

  // Check for Posix.2-style $(( )) arithmetic substitution
  if (temp && *temp == LPAREN)
    {
      char *temp2;
      temp1 = temp + 1;
      temp2 = savestring (temp1);
      t_index = strlen (temp2) - 1;

      if (temp2[t_index] != RPAREN)
        goto comsub;  // Not arithmetic, do command substitution

      temp2[t_index] = '\0';  // Remove trailing )

      // Verify it's valid arithmetic syntax
      if (chk_arithsub (temp2, t_index) == 0)
        goto comsub;

      // Expand variables in expression
      temp1 = expand_arith_string (temp2, Q_DOUBLE_QUOTES|Q_ARITH);
      free (temp2);

arithsub:
      // Evaluate expression
      savecmd = this_command_name;
      this_command_name = (char *)NULL;

      eflag = (shell_compatibility_level > 51) ? 0 : EXP_EXPANDED;
      number = evalexp (temp1, eflag, &expok);

      this_command_name = savecmd;
      free (temp);
      free (temp1);

      if (expok == 0)
        return (&expand_wdesc_error);

      // Convert result to string
      temp = itos (number);
      break;
    }
```

### expand_arith_string() (subst.c, line 3980)

Handles variable expansion within arithmetic expressions:

```c
char *
expand_arith_string (char *string, int quoted)
{
  WORD_DESC td;
  WORD_LIST *list, *tlist;
  char *ret;

  // Check if expansion is needed
  while (string[i])
    {
      if (ARITH_EXP_CHAR (string[i]))  // $, `, CTLESC
        break;
      else if (string[i] == '\'' || string[i] == '\\' || string[i] == '"')
        saw_quote = string[i];
      ADVANCE_CHAR (string, slen, i);
    }

  if (string[i])  // Found expandable character
    {
      // No process substitution or tilde expansion in arithmetic
      td.flags = W_NOPROCSUB|W_NOTILDE;
      td.word = savestring (string);
      list = call_expand_word_internal (&td, quoted, 0, NULL, NULL);

      if (list)
        {
          tlist = word_list_split (list);
          dispose_words (list);
          list = tlist;
          if (list)
            dequote_list (list);
        }

      ret = list ? string_list (list) : NULL;
      dispose_words (list);
      FREE (td.word);
    }
  else if (saw_quote)
    ret = string_quote_removal (string, quoted);
  else
    ret = savestring (string);

  return ret;
}
```

---

## 7. Special Features

### Short-Circuit Evaluation

Logical operators (`&&`, `||`) and ternary (`?:`) use `noeval` flag:

```c
static intmax_t
explor (void)  // Logical OR
{
  val1 = expland ();

  while (curtok == LOR)
    {
      set_noeval = 0;
      if (val1 != 0)  // Short-circuit: true || anything
        {
          noeval++;
          set_noeval = 1;
        }
      readtok ();
      val2 = expland ();
      if (set_noeval) noeval--;
      val1 = val1 || val2;
    }
  return (val1);
}
```

### Pre/Post Increment/Decrement

Handled in `exp0()`:

```c
// Pre-increment: ++var
if (curtok == PREINC || curtok == PREDEC)
{
  stok = curtok;
  readtok ();
  v2 = tokval + ((stok == PREINC) ? 1 : -1);
  vincdec = itos (v2);
  if (noeval == 0)
    expr_bind_variable (tokstr, vincdec);
  val = v2;
}

// Post-increment: var++
if (stok == POSTINC || stok == POSTDEC)
{
  v2 = val + ((stok == POSTINC) ? 1 : -1);
  vincdec = itos (v2);
  if (noeval == 0)
    expr_bind_variable (tokstr, vincdec);
  // val remains original value
}
```

### Array Support

Array subscripts are fully supported:

```c
// In readtok()
if (c == '[')
{
  e = expr_skipsubscript (tp, cp);
  if (cp[e] == ']')
    {
      cp += e + 1;
      e = ']';  // Flag for array access
    }
}

// In expr_bind_array_element()
void
expr_bind_array_element (const char *tok, arrayind_t ind, const char *rhs)
{
  // Rewrite var[expr] to var[computed_index]
  sprintf (lhs, "%s[%s]", vname, istr);
  expr_bind_variable (lhs, rhs);
}
```

---

## 8. Error Handling

### evalerror()

```c
static void
evalerror (const char *msg)
{
  internal_error (_("%s%s%s: %s (error token is \"%s\")"),
                  name ? name : "", name ? ": " : "",
                  t ? t : "", msg, 
                  (lasttp && *lasttp) ? lasttp : "");
  sh_longjmp (evalbuf, 1);  // Non-local return to evalexp
}
```

Common errors:
- `"division by 0"`
- `"exponent less than 0"`
- `"arithmetic syntax error in expression"`
- `"attempted assignment to non-variable"`
- `"expression recursion level exceeded"` (MAX_EXPR_RECURSION_LEVEL = 1024)

---

## 9. Summary: Complete Flow

```
Input: (( a + 2 ))

1. Lexer sees "((" 
   → parse_dparen() called
   → parse_arith_cmd() extracts " a + 2 "
   → Returns ARITH_CMD token

2. Parser matches grammar rule:
   arith_command: ARITH_CMD
   → make_arith_command() creates ARITH_COM structure

3. Execution via execute_command():
   → Dispatches to execute_arith_command()

4. execute_arith_command():
   → expand_arith_string() expands $variables
   → evalexp() evaluates expression

5. evalexp() → subexpr() → expcomma() → ... → exp0()
   → Recursive descent through precedence levels
   → readtok() tokenizes input
   → expr_streval() resolves variable values
   → Returns computed intmax_t result

6. Exit status:
   → 0 (success) if result != 0
   → 1 (failure) if result == 0
```

---

## Key Source Files

| File | Purpose |
|------|---------|
| [parse.y](file:///Users/pawelkaras/Desktop/bash/parse.y) | Lexical analysis, grammar rules |
| [command.h](file:///Users/pawelkaras/Desktop/bash/command.h) | ARITH_COM structure definition |
| [make_cmd.c](file:///Users/pawelkaras/Desktop/bash/make_cmd.c) | Command structure creation |
| [expr.c](file:///Users/pawelkaras/Desktop/bash/expr.c) | Expression evaluator (recursive-descent parser) |
| [execute_cmd.c](file:///Users/pawelkaras/Desktop/bash/execute_cmd.c) | Command execution |
| [subst.c](file:///Users/pawelkaras/Desktop/bash/subst.c) | $(()) arithmetic expansion |
