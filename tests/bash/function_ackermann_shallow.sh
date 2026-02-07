ackermann() {
  m="$1"
  n="$2"

  if [ "$m" -eq 0 ]; then
    echo $((n + 1))
  elif [ "$m" -gt 0 ] && [ "$n" -eq 0 ]; then
    m1=$((m - 1))
    ackermann "$m1" 1
  else
    n1=$((n - 1))
    inner=$(ackermann "$m" "$n1")
    m1=$((m - 1))
    ackermann "$m1" "$inner"
  fi
}

ack22=$(ackermann 2 2)
echo "$ack22"
