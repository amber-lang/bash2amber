### No execute
while grep -q "needle" /dev/null; do
  echo "loop"
done
echo "done"
