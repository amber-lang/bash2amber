## (n: Int): Int(result)
fact() {
  if [ "$1" -le 1 ]; then
    result=1
  else
    prev=$(( $1 - 1 ))
    fact "$prev"
    sub="$result"
    result=$(( $1 * sub ))
  fi
}

fact 5

echo "Here is a result: ${result}"
echo "The result is: ${result}"
