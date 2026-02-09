process() {
  local first="$1"
  echo "$@"
  echo "$first"
}
process "arg1"
