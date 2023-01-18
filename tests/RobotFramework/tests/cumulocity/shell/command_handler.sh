#!/bin/bash

info() {
    echo "$(date --iso-8601=seconds 2>/dev/null || date -Iseconds) : INFO : $*" >&2
}

# COMMAND=$(echo "$1" | cut -d, -f3- | sed 's/""/"/g'  )
COMMAND=$(echo "$1" | cut -d, -f3- | sed -e 's/^"//' -e 's/"$//' )

info "Executing command: $COMMAND"
bash -c "$COMMAND"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    info "Command returned a non-zero exit code. code=$EXIT_CODE"
fi
exit $EXIT_CODE
