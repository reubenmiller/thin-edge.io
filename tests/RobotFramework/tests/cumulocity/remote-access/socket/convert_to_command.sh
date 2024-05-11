#!/bin/sh
#
# Script to publish a message to a TCP or AF UNIX socket using socat.
# After writing to the socket, the socket will be read from to check
# if the response contains a specific string to mark the request as
# being successful or not.
#
set -e

MESSAGE_PROPERTY=
MESSAGE_VALUE=

help() {
    cat << EOT >&2
Create a thin-edge.io command from a given input

USAGE

   $0 <CONNECTION_PROPERTY> <MESSAGE>

POSITIONAL ARGS

    MESSAGE                 Message to be sent to the socket

FLAGS

  --socket <address>        TCP or Unix socket path. e.g. /run/example.sock or 127.0.0.1:4444
  --response-ok <string>    String to match against to determine that the request sent to the socket
                            was received by the component reacting to the request.
  --response-fail <string>  String to match against to determine an error. This will override any
                            existing ok response.
  --help                    Show this help


EXAMPLES

   $0 socket=/run/example.socket response_ok=CONNECTED response_fail=STOPPING connection_string "530,TST_throw_crabby_exception,127.0.0.1,22,18f7c014-8180-40e0-b272-03c9dec8f327"
   # Publish a c8y-remote-access-plugin message to a socket, and check for a successful connection

EOT
}

# Parse cli options
PAYLOAD='{"status":"init"}'

while [ $# -gt 0 ]; do
    case "$1" in
        --help|-h)
            help
            exit
            ;;

        --) # End all options
            shift
            break
            ;;

        *=*)
            PAYLOAD=$(echo "$PAYLOAD" | jo -p -f - -- "$1")
            ;;
        
        --*|-*)
            ;;

        *)
            if [ $# -ne 2 ]; then
                echo "Warning: expected 2 non-json arguments at the end" >&2
            fi
            MESSAGE_PROPERTY="$1"
            MESSAGE_VALUE="$2"
            PAYLOAD=$(echo "$PAYLOAD" | jo -p -f - -- "$MESSAGE_PROPERTY=$MESSAGE_VALUE")
            shift
            shift
            break
            ;;
    esac
    shift
done

# Get local topic id where the command should be sent to
DEVICE_ID=${DEVICE_ID:-}
if [ -z "$DEVICE_ID" ]; then
    DEVICE_ID=$(tedge config get device.id)
fi

TOPIC_ROOT=${TOPIC_ROOT:-}
if [ -z "$TOPIC_ROOT" ]; then
    TOPIC_ROOT=$(tedge config get mqtt.topic_root)
fi

TARGET_EXT_ID=$(echo "$MESSAGE_VALUE" | cut -d, -f2 | tr -d '"')
TOPIC_ID=
if [ "$TARGET_EXT_ID" = "$DEVICE_ID" ]; then
    TOPIC_ID="device/main//"
else
    TOPIC_ID=$(echo "$TARGET_EXT_ID" | sed "s/${DEVICE_ID}://" | tr ':' '/')
fi

# Format topic id so it has 4 slashes
case "$TOPIC_ID" in
    */*/*/*)
        # do nothing
        ;;
    */*/*)
        TOPIC_ID="$TOPIC_ID/"
        ;;
    */*)
        TOPIC_ID="$TOPIC_ID//"
        ;;
    *)
        TOPIC_ID="$TOPIC_ID///"
        ;;
esac

# Send command request
TOPIC="$TOPIC_ROOT/$TOPIC_ID/cmd/remote_access/c8y-123"
echo "Publishing command request. topic=$TOPIC, payload=$PAYLOAD" >&2
tedge mqtt pub -r -q 1 "$TOPIC" "$PAYLOAD"
