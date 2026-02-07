### No execute
foobar() {
  echo 7
}
foo() {
  x=$(foobar)
  echo "$x"
}
foo
