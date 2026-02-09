## (first: Text, second: Text): Null
process() {
  local first="$1"
  echo "$@"
  echo "$first"
  local second="$2"
  echo "$second"
  local third="${second}"
  echo "$third"
}
process "arg1" "arg2"
