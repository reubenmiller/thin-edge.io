#!/bin/sh
set -e
ACTION="$1"
SERVICE_NAME="$2"
TOPIC="$3"

SYSTEMD_ACTION=
SERVICE_STATUS=

case "$ACTION" in
    START|start)
        SYSTEMD_ACTION=start
        SERVICE_STATUS="up"
        ;;
    STOP|stop)
        SYSTEMD_ACTION=stop
        SERVICE_STATUS="down"
        ;;
    RESTART|restart)
        SYSTEMD_ACTION=restart
        ;;
    *)
        echo "Unsupported service action. name=$SYSTEMD_ACTION. Only START, STOP, RESTART are supported"
        exit 1
        ;;
esac

SUDO=
if command -V sudo >/dev/null 2>&1; then
    SUDO=sudo
fi

$SUDO systemctl "$SYSTEMD_ACTION" "$SERVICE_NAME"

if [ -n "$SERVICE_STATUS" ]; then
    tedge mqtt pub --retain -q 1 "$TOPIC"/status/health "{\"status\":\"$SERVICE_STATUS\"}"
fi
