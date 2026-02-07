left=1
right=2
result=""
if [ "$left" -lt "$right" ] && [ "$right" -gt 1 ]; then
  result="yes"
else
  result="no"
fi
echo "$result"
