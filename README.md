# bash2amber

A transpiler that converts Bash scripts into [Amber](https://amber-lang.com) — a modern, type-safe language that compiles back to shell.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Print Amber output to stdout
bash2amber script.sh

# Write Amber output to a file
bash2amber script.sh script.ab
```

## What It Transforms

### Variables and strings

```bash
name="World"
echo "Hello $name"
```

```amber
let name = "World"
echo("Hello {name}")
```

### Arrays and loops

```bash
groceries=("apple" "banana" "cherry")
for item in "${groceries[@]}"; do
  echo "$item"
done
```

```amber
let groceries = ["apple", "banana", "cherry"]
for item in groceries {
    echo(item)
}
```

### C-style for loops

```bash
for (( i=0; i<=5; i++ )); do
  echo "$i"
done
```

```amber
for i in 0..=5 {
    echo(i)
}
```

### Conditionals

```bash
status="200"
if [ "$status" == "200" ]; then
  echo "UP"
else
  echo "DOWN"
fi
```

```amber
let status_var = "200"
if status_var == "200" {
    echo("UP")
} else {
    echo("DOWN")
}
```

### Case statements

Case blocks are converted into if-chains.

```bash
fruit="pear"
case "$fruit" in
  apple|banana)
    echo "common"
    ;;
  pear)
    echo "pear"
    ;;
esac
```

```amber
let fruit = "pear"
if {
    fruit == "apple" or fruit == "banana" {
        echo("common")
    }
    fruit == "pear" {
        echo("pear")
    }
}
```

### Arithmetic

```bash
base=10
result=$((base + 4 / 2))
echo "$result"
```

```amber
let base = 10
let result = base + 4 / 2
echo(result)
```

### Functions

Annotate your Bash functions with `##` fundoc comments to get typed Amber output.

```bash
## (msg: Text): Text(output)
wrap() {
  local msg="$1"
  output="[${msg}]"
}
wrap "hello"
echo $output
```

```amber
fun wrap(msg: Text): Text {
    return "[{msg}]"
}
let output = wrap("hello")
echo(output)
```

### Ternary expressions

Simple if/else assignments are collapsed into ternary form.

```bash
mode="prod"
if [ "$mode" = "prod" ]; then
  status="live"
else
  status="test"
fi
echo "$status"
```

```amber
let mode = "prod"
let status_var = mode == "prod" then "live" else "test"
echo(status_var)
```

## Unsupported constructs

When bash2amber encounters a construct it can't convert, it wraps the original shell code in a `trust` block so the output still compiles:

```amber
trust $ original-command $
```