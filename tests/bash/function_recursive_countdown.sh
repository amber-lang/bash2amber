countdown() {
  echo "$1"
  if [ "$1" -gt 0 ]; then
    local next=$(( $1 - 1 ))
    countdown "$next"
  fi
}

countdown 5
