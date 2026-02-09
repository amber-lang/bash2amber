# Test: Direct positional in arithmetic as function argument
countdown_direct() {
  echo "$1"
  if [ "$1" -gt 0 ]; then
    countdown_direct $(($1 - 1))
  fi
}
countdown_direct 3
