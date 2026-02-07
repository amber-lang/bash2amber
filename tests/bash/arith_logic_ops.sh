### No execute
a=10
b=5
if (( a > 5 && b < 10 )); then
  echo "and_true"
fi
if (( a == 10 )); then
  echo "eq_true"
fi
if (( a != 10 )); then
  echo "ne_true"
else
  echo "ne_false"
fi
if (( a >= b )); then
  echo "ge_true"
fi
if (( a <= b )); then
  echo "le_true"
else
  echo "le_false"
fi
if (( ! 1 )); then
  echo "not_true"
else
  echo "not_false"
fi
