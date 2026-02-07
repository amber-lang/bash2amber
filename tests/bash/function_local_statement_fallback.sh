local_fallback() {
  printf "%s" "x" | tr x y
  echo "done"
}

local_fallback
