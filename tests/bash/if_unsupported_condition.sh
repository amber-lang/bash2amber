### No execute
run() {
  if grep -q "needle" /dev/null; then
    echo "found"
  else
    echo "missing"
  fi
  echo "after"
}

run
