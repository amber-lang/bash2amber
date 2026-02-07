state="failed"
if [ "$state" != "ok" ]; then
  echo "retry"
else
  echo "done"
fi
