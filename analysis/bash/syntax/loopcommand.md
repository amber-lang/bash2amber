# LoopCommand Analysis (`while` and `until`)

This document provides a deep analysis of how `while` and `until` loops are implemented in the Bash codebase. In Bash internal structures, these are represented by `WHILE_COM` and executed via shared logic.

## 1. Internal Structure

Both `while` and `until` commands rely on the `WHILE_COM` structure defined in `command.h`.

```c
typedef struct while_com {
  int flags;			/* See description of CMD flags. */
  COMMAND *test;		/* Thing to test. */
  COMMAND *action;		/* Thing to do while test is non-zero. */
} WHILE_COM;
```

*   **`flags`**: Execution flags (e.g., `CMD_IGNORE_RETURN`).
*   **`test`**: The command executed to determine if the loop should continue (or stop).
*   **`action`**: The body of the loop, executed if the test condition is met.

Internal command types:
*   `cm_while`: For `while` loops.
*   `cm_until`: For `until` loops.

## 2. Command Creation

The Creation of loop commands is handled in `make_cmd.c`. Both `make_while_command` and `make_until_command` delegate to a helper function `make_until_or_while`.

```c
static COMMAND *
make_until_or_while (enum command_type which, COMMAND *test, COMMAND *action)
{
  WHILE_COM *temp;

  temp = (WHILE_COM *)xmalloc (sizeof (WHILE_COM));
  temp->flags = 0;
  temp->test = test;
  temp->action = action;
  return (make_command (which, (SIMPLE_COM *)temp));
}
```

This ensures that both loop types share the same memory layout and initialization, differing only in their `command_type`.

## 3. Execution Logic

Execution is handled in `execute_cmd.c`. The entry points are:
*   `execute_while_command(WHILE_COM *while_command)`
*   `execute_until_command(WHILE_COM *while_command)`

Both functions immediately call `execute_while_or_until(while_command, type)`, where `type` is either `CMD_WHILE` or `CMD_UNTIL`.

### Core Execution Loop (`execute_while_or_until`)

The shared execution logic relies on an infinite C loop (`while(1)`) that implements the shell loop cycle.

1.  **Setup**:
    *   `loop_level` is incremented (tracking nesting depth).
    *   `interrupt_execution` is incremented.
    *   `CMD_IGNORE_RETURN` is set on the `test` command (since test failure is a control condition, not necessarily an error).

2.  **The Loop Cycle**:
    *   **Execute Test**: `execute_command(while_command->test)` is called.
    *   **Check Condition**:
        *   **`while`**: If `return_value != EXECUTION_SUCCESS` (0), the loop terminates (`break`).
        *   **`until`**: If `return_value == EXECUTION_SUCCESS` (0), the loop terminates (`break`).
    *   **Handle Interrupts**: Checks for `breaking` or `continuing` inside the test (rare but possible with jobs).
    *   **Execute Body**: `execute_command(while_command->action)` is called.
    *   **Reap Jobs**: `REAP()` is called to handle background processes.
    *   **Loop Control (`break`/`continue`)**:
        *   **`breaking`**: If `break` was called (setting `breaking > 0`), decrement `breaking` and exit the C loop.
        *   **`continuing`**: If `continue` was called (setting `continuing > 0`):
            *   Decrement `continuing`.
            *   If `continuing` is still non-zero (e.g., `continue 2`), break the C loop to propagate to the outer loop.
            *   If `continuing` is zero, loop repeats (implicit continue).

3.  **Cleanup**:
    *   `loop_level` and `interrupt_execution` are decremented.
    *   Returns `body_status` (the exit status of the last executed command in the body, or `EXECUTION_SUCCESS` if the body never ran).

### Key Execution Differences

The only logical difference between `while` and `until` is the condition check:

```c
if (type == CMD_WHILE && return_value != EXECUTION_SUCCESS)
  break;
if (type == CMD_UNTIL && return_value == EXECUTION_SUCCESS)
  break;
```

## 4. Loop Control Mechanism

Bash handles `break` and `continue` using global counters:
*   **`loop_level`**: Tracks how deep we are in nested loops.
*   **`breaking`**: Number of loops to break out of (`break n`).
*   **`continuing`**: Number of loops to continue out of (`continue n`).

When `execute_while_or_until` sees `breaking > 0`, it decrements and exits. If `breaking` is still non-zero after returning, the outer loop will see it and do the same, effectively unwinding the stack of loops.
