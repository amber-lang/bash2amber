process_item() {
  value="$1"
  echo "input:${value}"
  printf -v value "%s_processed" "$value"
  echo "$value"
}

process_item "amber"
