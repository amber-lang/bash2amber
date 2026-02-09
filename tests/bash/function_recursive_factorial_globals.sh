fact() {
  n="$1"
  if [ "$n" -le 1 ]; then
    echo 1
  else
    prev=$((n - 1))
    sub=$(fact "$prev")
    echo $((n * sub))
  fi
}

fact 5
