## (msg: Text): Text(output)
wrap() {
  local msg="$1"
  output="[${msg}]"
}
wrap "hello"
echo $output
