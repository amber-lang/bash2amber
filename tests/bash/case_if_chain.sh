action="stop"
case "$action" in
  start)
    echo "starting"
    ;;
  stop)
    echo "stopping"
    ;;
  *)
    echo "unknown"
    ;;
esac
