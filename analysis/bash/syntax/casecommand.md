# CaseCommand Analysis

## Overview
The `CaseCommand` represents the `case` control structure in Bash, which allows for conditional execution based on pattern matching. It works by expanding a word and comparing it against a list of patterns, executing the corresponding command list for the first match (or subsequent matches if fallthrough or specific flags are set).

## Structure
The `CaseCommand` is implemented using the `CASE_COM` structure defined in `command.h`.

```c
/* The CASE command. */
typedef struct case_com {
  int flags;			/* See description of CMD flags. */
  int line;			/* line number the `case' keyword appears on */
  WORD_DESC *word;		/* The thing to test. */
  PATTERN_LIST *clauses;	/* The clauses to test against, or NULL. */
} CASE_COM;
```

### Components
- **`word`**: A `WORD_DESC` representing the expression to be matched.
- **`clauses`**: A linked list of `PATTERN_LIST` structures, each containing a list of patterns and an action.

```c
/* Pattern/action structure for CASE_COM. */
typedef struct pattern_list {
  struct pattern_list *next;	/* Clause to try in case this one failed. */
  WORD_LIST *patterns;		/* Linked list of patterns to test. */
  COMMAND *action;		/* Thing to execute if a pattern matches. */
  int flags;
} PATTERN_LIST;
```

### Pattern List Flags
- **`CASEPAT_FALLTHROUGH` (0x01)**: Corresponds to `;&`, allowing execution to continue to the next clause's action.
- **`CASEPAT_TESTNEXT` (0x02)**: Corresponds to `;;&`, allowing testing of subsequent clauses after a match.

## Parsing
The parsing logic is defined in `parse.y`. The grammar rule `case_command` constructs the `CASE_COM` object.

```yacc
case_command:	CASE WORD newline_list IN newline_list ESAC
			{ $$ = make_case_command ($2, (PATTERN_LIST *)NULL, compoundcmd_lineno[compoundcmd_top].lineno); }
	|	CASE WORD newline_list IN case_clause_sequence newline_list ESAC
			{ $$ = make_case_command ($2, $5, compoundcmd_lineno[compoundcmd_top].lineno); }
	|	CASE WORD newline_list IN case_clause ESAC
			{ $$ = make_case_command ($2, $5, compoundcmd_lineno[compoundcmd_top].lineno); }
```

Helper functions in `make_cmd.c`:
- `make_case_command`: Allocates and initializes `CASE_COM`.
- `make_pattern_list`: Creates `PATTERN_LIST` nodes from pattern words and actions.

## Execution
The execution logic happens in `execute_case_command` within `execute_cmd.c`.

### 1. Initialization and Expansion
1. **Debug & Trace**: Handles debug traps (`run_debug_trap`) and `set -x` command printing.
2. **Word Expansion**: The subject word (`case_command->word`) is expanded (`expand_word_leave_quoted`) and dequoted (`dequote_string`). This result is what patterns will be matched against.

### 2. Clause Iteration
The function iterates through `case_command->clauses`. Inside this loop, it iterates through `clauses->patterns`.

### 3. Pattern Matching
For each pattern in the clause:
1. **Expansion**: The pattern word is expanded using `expand_word_leave_quoted`.
2. **Quote Handling**: Special handling is applied to preserve quotes for the globbing engine using `quote_string_for_globbing`.
3. **Matching**: The pattern is matched against the expanded word using `strmatch` (a wrapper around `fnmatch`).
   - Flags used: `FNMATCH_EXTFLAG | FNMATCH_IGNCASE`.

### 4. Action Execution
If a match is found:
1. **Execute**: The associated `action` command is executed via `execute_command`.
2. **Control Flow**:
   - **Standard (`;;`)**: The loop terminates (`EXIT_CASE`), stopping further checks.
   - **Fallthrough (`;%`)**: If `CASEPAT_FALLTHROUGH` is set, the loop continues, executing the *next clause's action* immediately without checking its pattern.
   - **Test Next (`;;&`)**: If `CASEPAT_TESTNEXT` is set, the loop breaks the current clause check but continues to the *next clause's pattern check*.

## Code Example Layout
```bash
case "$1" in
    start)
        echo "Starting"
        ;;
    stop)
        echo "Stopping"
        ;;
    reload|restart)  # Multiple patterns in one clause
        echo "Restarting"
        ;&            # Fallthrough (CASEPAT_FALLTHROUGH)
    force)
        echo "Force mode"
        ;;
    *)
        echo "Unknown"
        ;;
esac
```

## Summary
The `CaseCommand` is a complex control structure dependent on pattern matching. Its execution involves a two-stage expansion process (word then patterns) and supports sophisticated control flow via `;;`, `;&`, and `;;&`.
