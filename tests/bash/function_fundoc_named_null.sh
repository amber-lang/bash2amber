## (name: Text): Null
greet() {
  local name="$1"
  echo "Hello $name"
}
greet "world"
