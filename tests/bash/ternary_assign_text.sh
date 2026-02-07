mode="prod"
status=""
if [ "$mode" = "prod" ]; then
  status="live"
else
  status="test"
fi
echo "$status"
