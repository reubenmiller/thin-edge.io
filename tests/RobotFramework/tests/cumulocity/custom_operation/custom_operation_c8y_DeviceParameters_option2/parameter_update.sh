#!/bin/sh
set -e
COMMAND="$1"
shift

PARAMETER_PLUGINS="/usr/share/tedge/parameter-plugins"

get_parameter_type() {
    echo "$1" |  jq -r '.operation | keys | .[] | select(. | startswith("c8y_ParameterUpdate_")) | sub("^c8y_ParameterUpdate_"; "")'
}

set_parameters() {
    if [ $# -lt 2 ]; then
        echo "Missing required positional arguments. expected as least 2 arguments" >&2
        exit 1
    fi
    TYPE="$1"
    PARAMETERS="$2"

    if [ ! -x "$PARAMETER_PLUGINS/$TYPE" ]; then
        echo "Could not find a matching parameter plugin. path=$PARAMETER_PLUGINS/$TYPE" >&2
        exit 1
    fi

    "$PARAMETER_PLUGINS/$TYPE" set "$PARAMETERS"
}

case "$COMMAND" in
    prepare)
        MESSAGE="$1"
        TYPE=$(get_parameter_type "$MESSAGE")
        if [ -z "$TYPE" ]; then
            echo "Could not detect the parameter update type in message" >&2
            exit 1
        fi

        PARAMETER_PAYLOAD=$(echo "$MESSAGE" | jq ".operation.${TYPE}")

        echo :::begin-tedge:::
        printf '{"type":"%s","parameters":%s}\n' "$TYPE" "$PARAMETER_PAYLOAD"
        echo :::end-tedge:::
        ;;
    set)
        set_parameters "$@"
        ;;
esac
