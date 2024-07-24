#!/bin/sh
set -e

#
# Main
#
if [ $# -lt 1 ]; then
    echo "Missing required positional argument" >&2
    exit 2
fi

COMMAND="$1"
TYPE="$2"
shift
shift

case "$COMMAND" in
    postprocess)
        case "$TYPE" in
            custom1)
                echo "Restarting mosquitto service" >&2
                systemctl restart mosquitto
                ;;
            *)
                ;;
        esac
        ;;

    preprocess)
        case "$TYPE" in
            custom1)
                echo "Backing up file: $1" >&2
                ;;
            *)
                ;;
        esac
        ;;
esac

exit 0
