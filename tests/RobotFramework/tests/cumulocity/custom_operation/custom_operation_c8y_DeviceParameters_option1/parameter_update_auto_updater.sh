#!/bin/sh
set -e
MESSAGE="$1"

echo "Processing the parameters: $MESSAGE" >&2

ENABLED=$(echo "$MESSAGE" | jq -r '.enabled // false')
INTERVAL=$(echo "$MESSAGE" | jq -r '.interval // "weekly"')

# Do something with the values

# create event
text="Changing auto updater. enabled=$ENABLED, interval=$INTERVAL"
timestamp=""
detect_change="false"

tedge mqtt pub -q 1 'c8y/s/us' "408,\"${text}\",${timestamp},${detect_change},AutoUpdater.enabled,BOOLEAN,$ENABLED,AutoUpdater.interval,STRING,\"$INTERVAL\""

# Workaround: manually update the digital twin property though normally this is done by the Device Parameter microservice
tedge mqtt pub -r te/device/main///twin/AutoUpdater "{\"enabled\":$ENABLED,\"interval\":\"$INTERVAL\"}"
