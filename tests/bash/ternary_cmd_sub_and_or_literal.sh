True() {
  echo "True"
}
False() {
  echo "False"
}
condition="true"
var=$([ "$condition" = "true" ] && "True" || "False")
echo "$var"
