#!/bin/sh
set -e

FRAGMENT=""
TARGET=""
PARAMETERS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --topic)
            TARGET="$2"
            shift
            ;;
        --fragment)
            FRAGMENT="$2"
            shift
            ;;
        --parameters)
            PARAMETERS="$2"
            shift
            ;;
    esac
    shift
done

echo "Processing the parameters: $MESSAGE" >&2

ENABLED=$(echo "$PARAMETERS" | jq -r '.enabled // false')
INTERVAL=$(echo "$PARAMETERS" | jq -r '.interval // "weekly"')

# Do something with the values

# create event
TEXT="Changing auto updater. enabled=$ENABLED, interval=$INTERVAL"

# Option 1: Send event and also update the managed object manually
PAYLOAD=$(printf '{"text":"%s","%s":%s}' "$TEXT" "$FRAGMENT" "$PARAMETERS" )
tedge mqtt pub -q 1 "$TARGET/e/c8y_ParameterUpdate" "$PAYLOAD"
tedge mqtt pub -r "$TARGET/twin/AutoUpdater" "{\"enabled\":$ENABLED,\"interval\":\"$INTERVAL\"}"

# Option 2: Let the server (Device Parameter microservice) update the managed object fragment
# by processing the c8y_ParameterUpdate event
# TIMESTAMP=""
# DETECT_CHANGE="false"
# tedge mqtt pub -q 1 'c8y/s/us' "408,\"${TEXT}\",${TIMESTAMP},${DETECT_CHANGE},AutoUpdater.enabled,BOOLEAN,$ENABLED,AutoUpdater.interval,STRING,\"$INTERVAL\""
