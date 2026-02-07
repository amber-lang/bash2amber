## (n: Int): Int(result)
fact() {
  local n="$1"
  if [ "$n" -le 1 ]; then
    result=1
  else
    local prev=$((n - 1))
    fact "$prev"
    local sub="$result"
    result=$((n * sub))
  fi
}

fact 5
echo $result
