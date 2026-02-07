a=2
b=5
if [ "$a" -lt "$b" ] && [ "$b" -gt 0 ] || [ "$a" -eq 0 ]; then
  echo "true"
else
  echo "false"
fi
